//! Minimal native FoundryVTT client: authenticate headlessly and write
//! documents over the Engine.IO/Socket.IO `modifyDocument` op.
//!
//! Protocol reverse-engineered + validated against Foundry **v14.364** (Phase
//! 23). The three v14 essentials, each load-bearing:
//!  1. Log in with the user's 16-char `_id` (v14's anonymous join socket no
//!     longer resolves a username → id).
//!  2. The websocket is authenticated by the **session cookie on the upgrade
//!     request**, not a `?session=` query param — query-only yields an
//!     anonymous socket whose writes are silently dropped.
//!  3. Document mutations are the `modifyDocument` event's **ack**, and so is
//!     the initial `world` payload (not a pushed event).

use crate::error::{AppError, AppResult};
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::Message;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

const IO_TIMEOUT: Duration = Duration::from_secs(20);

type Ws = WebSocketStream<MaybeTlsStream<TcpStream>>;

/// An authenticated, connected Foundry session ready to mutate documents.
pub struct FoundryClient {
    ws: Ws,
    ack: u64,
    base: String,
    session: String,
}

impl FoundryClient {
    /// Runs the full 3-step auth (GET cookie → POST /join with `user_id` →
    /// authenticated websocket) and returns a live client.
    pub async fn connect(base_url: &str, user_id: &str, password: &str) -> AppResult<Self> {
        let base = base_url.trim_end_matches('/');
        let session = authenticate(base, user_id, password).await?;

        // Socket.IO over websocket; the session rides as a Cookie header.
        let ws_url = format!(
            "{}/socket.io/?EIO=4&transport=websocket",
            base.replacen("http", "ws", 1)
        );
        let mut req = ws_url
            .into_client_request()
            .map_err(|e| AppError::BadRequest(format!("bad foundry url: {e}")))?;
        req.headers_mut().insert(
            "Cookie",
            HeaderValue::from_str(&format!("session={session}"))
                .map_err(|e| AppError::Internal(anyhow::anyhow!("cookie header: {e}")))?,
        );

        let (ws, _resp) = connect_async(req)
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry websocket: {e}")))?;

        let mut client = Self {
            ws,
            ack: 1,
            base: base.to_string(),
            session,
        };
        client.handshake().await?;
        Ok(client)
    }

    /// Uploads file bytes into Foundry's user data under `target` (a directory
    /// relative to the data root, e.g. `chronicle-keeper/myworld`), returning
    /// the stored path usable as a Scene `background.src`. Uses the HTTP
    /// `/upload` route (the websocket op can't move files), session-authed.
    pub async fn upload_file(
        &self,
        target: &str,
        filename: &str,
        bytes: Vec<u8>,
    ) -> AppResult<String> {
        let http = reqwest::Client::builder()
            .build()
            .map_err(|e| AppError::Internal(anyhow::anyhow!("http client: {e}")))?;
        let part = reqwest::multipart::Part::bytes(bytes)
            .file_name(filename.to_string())
            .mime_str("application/octet-stream")
            .map_err(|e| AppError::Internal(anyhow::anyhow!("upload part: {e}")))?;
        let form = reqwest::multipart::Form::new()
            .text("source", "data")
            .text("target", target.to_string())
            .part("upload", part);
        let resp = http
            .post(format!("{}/upload", self.base))
            .header("Cookie", format!("session={}", self.session))
            .multipart(form)
            .send()
            .await
            .map_err(|e| AppError::BadRequest(format!("foundry upload: {e}")))?;
        let body: Value = resp
            .json()
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry upload body: {e}")))?;
        if body.get("status").and_then(|s| s.as_str()) != Some("success") {
            let msg = body
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("upload failed");
            return Err(AppError::Internal(anyhow::anyhow!("foundry upload: {msg}")));
        }
        body.get("path")
            .and_then(|p| p.as_str())
            .map(|s| s.to_string())
            .ok_or_else(|| AppError::Internal(anyhow::anyhow!("foundry upload: no path returned")))
    }

    /// Completes the Engine.IO/Socket.IO handshake: wait for the engine open
    /// (`0{...}`), send the namespace connect (`40`), wait for its ack.
    async fn handshake(&mut self) -> AppResult<()> {
        loop {
            let frame = self.read_frame().await?;
            if frame.starts_with("0{") {
                self.send("40").await?;
            } else if frame.starts_with("40") {
                return Ok(());
            }
            // Ignore anything else (e.g. an early `2["session",...]`).
        }
    }

    /// Emits a Socket.IO event with arguments and returns the first element of
    /// its ack array. Pings are answered transparently while waiting.
    async fn emit_ack(&mut self, args: Value) -> AppResult<Value> {
        let ack_id = self.ack;
        self.ack += 1;
        self.send(&format!("42{ack_id}{args}")).await?;
        let prefix = format!("43{ack_id}");
        loop {
            let frame = self.read_frame().await?;
            if let Some(rest) = frame.strip_prefix(&prefix) {
                let arr: Value = serde_json::from_str(rest)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry ack json: {e}")))?;
                return Ok(arr.get(0).cloned().unwrap_or(Value::Null));
            }
            // Other acks / event broadcasts are not ours — keep waiting.
        }
    }

    /// Emits a `modifyDocument` op and returns the server's ack payload object
    /// (carries `result`, the created/updated docs or deleted ids).
    pub async fn modify_document(
        &mut self,
        doc_type: &str,
        action: &str,
        operation: Value,
    ) -> AppResult<Value> {
        let mut op = operation;
        // Defaults Foundry expects on every mutation operation.
        if let Value::Object(ref mut m) = op {
            m.entry("broadcast").or_insert(json!(true));
            m.entry("pack").or_insert(Value::Null);
            m.entry("modifiedTime").or_insert(json!(now_ms()));
        }
        let resp = self
            .emit_ack(
                json!(["modifyDocument", { "type": doc_type, "action": action, "operation": op }]),
            )
            .await?;
        if let Some(err) = resp.get("error").and_then(|e| e.as_str()) {
            return Err(AppError::Internal(anyhow::anyhow!(
                "foundry rejected {action} {doc_type}: {err}"
            )));
        }
        Ok(resp)
    }

    /// Creates a directory under the user data root (`storage = "data"`),
    /// building each path level. The `/upload` route won't create dirs, so the
    /// scene-art target must be made first. An already-existing level is fine.
    pub async fn create_directory(&mut self, path: &str) -> AppResult<()> {
        let mut cumulative = String::new();
        for comp in path.split('/').filter(|c| !c.is_empty()) {
            if !cumulative.is_empty() {
                cumulative.push('/');
            }
            cumulative.push_str(comp);
            let resp = self
                .emit_ack(json!([
                    "manageFiles",
                    { "action": "createDirectory", "storage": "data", "target": cumulative },
                    {}
                ]))
                .await?;
            if let Some(err) = resp.get("error").and_then(|e| e.as_str()) {
                // Foundry reports an existing dir as an error; that is success.
                if !err.contains("EEXIST") && !err.to_lowercase().contains("exist") {
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "foundry create dir {cumulative}: {err}"
                    )));
                }
            }
        }
        Ok(())
    }

    pub async fn close(mut self) {
        let _ = self.ws.send(Message::Close(None)).await;
    }

    async fn send(&mut self, text: &str) -> AppResult<()> {
        self.ws
            .send(Message::Text(text.to_string()))
            .await
            .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry send: {e}")))
    }

    /// Reads the next Engine.IO text frame, transparently answering pings
    /// (`2` → `3`) so the connection survives a multi-page sync.
    async fn read_frame(&mut self) -> AppResult<String> {
        loop {
            let msg = tokio::time::timeout(IO_TIMEOUT, self.ws.next())
                .await
                .map_err(|_| AppError::Internal(anyhow::anyhow!("foundry read timeout")))?
                .ok_or_else(|| AppError::Internal(anyhow::anyhow!("foundry socket closed")))?
                .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry read: {e}")))?;
            match msg {
                Message::Text(t) => {
                    if t == "2" {
                        self.send("3").await?;
                        continue;
                    }
                    return Ok(t);
                }
                Message::Ping(_) | Message::Pong(_) => continue,
                Message::Close(_) => {
                    return Err(AppError::Internal(anyhow::anyhow!("foundry socket closed")))
                }
                _ => continue,
            }
        }
    }
}

/// GET `/join` for a session cookie, then POST `/join` with the user `_id` to
/// authenticate it. Returns the session token bound to the user.
async fn authenticate(base: &str, user_id: &str, password: &str) -> AppResult<String> {
    let http = reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .build()
        .map_err(|e| AppError::Internal(anyhow::anyhow!("http client: {e}")))?;

    let get = http
        .get(format!("{base}/join"))
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("foundry GET /join: {e}")))?;
    let session = get
        .headers()
        .get_all("set-cookie")
        .iter()
        .filter_map(|v| v.to_str().ok())
        .find_map(|c| {
            c.split(';')
                .next()
                .and_then(|kv| kv.strip_prefix("session="))
                .map(|s| s.to_string())
        })
        .ok_or_else(|| {
            AppError::Internal(anyhow::anyhow!("foundry /join set no session cookie"))
        })?;

    let post = http
        .post(format!("{base}/join"))
        .header("Cookie", format!("session={session}"))
        .json(&json!({ "action": "join", "userid": user_id, "password": password }))
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("foundry POST /join: {e}")))?;
    let body: Value = post
        .json()
        .await
        .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry /join body: {e}")))?;
    if body.get("status").and_then(|s| s.as_str()) != Some("success") {
        let msg = body
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("authentication failed");
        return Err(AppError::BadRequest(format!("foundry auth: {msg}")));
    }
    Ok(session)
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// Best-effort fetch of Foundry's unauthenticated `/api/status` JSON (long-standing
/// monitoring endpoint: `version`, `world`, `system`, `active`). Returns `None` on
/// any failure — version reporting must never break the connection test.
pub async fn fetch_status(base_url: &str) -> Option<Value> {
    let base = base_url.trim_end_matches('/');
    let http = reqwest::Client::builder().build().ok()?;
    let resp = http
        .get(format!("{base}/api/status"))
        .timeout(Duration::from_secs(8))
        .send()
        .await
        .ok()?;
    resp.json().await.ok()
}
