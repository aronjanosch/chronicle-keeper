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

        let mut client = Self { ws, ack: 1 };
        client.handshake().await?;
        Ok(client)
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
        let ack_id = self.ack;
        self.ack += 1;
        let payload =
            json!(["modifyDocument", { "type": doc_type, "action": action, "operation": op }]);
        self.send(&format!("42{ack_id}{payload}")).await?;

        let prefix = format!("43{ack_id}");
        loop {
            let frame = self.read_frame().await?;
            if let Some(rest) = frame.strip_prefix(&prefix) {
                let arr: Value = serde_json::from_str(rest)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("foundry ack json: {e}")))?;
                let resp = arr.get(0).cloned().unwrap_or(Value::Null);
                if let Some(err) = resp.get("error").and_then(|e| e.as_str()) {
                    return Err(AppError::Internal(anyhow::anyhow!(
                        "foundry rejected {action} {doc_type}: {err}"
                    )));
                }
                return Ok(resp);
            }
            // Other acks / event broadcasts are not ours — keep waiting.
        }
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
