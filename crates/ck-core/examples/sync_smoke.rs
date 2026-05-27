//! End-to-end wire test: the Rust sync DTOs against a live sync server.
//!
//! Validates JSON shape compatibility (serde <-> the Python server's pydantic)
//! and the push -> pull round-trip over real HTTP. Not a CI test — needs a
//! running server.
//!
//! Run (server on :8899, open mode):
//!   cargo run --example sync_smoke -p ck-core -- http://127.0.0.1:8899

use ck_core::sync::{Campaign, SyncPayload, SyncRequest, SyncResponse};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let base = std::env::args().nth(1).unwrap_or_else(|| "http://127.0.0.1:8899".into());
    let endpoint = format!("{}/sync", base.trim_end_matches('/'));
    let client = reqwest::Client::new();

    // Device A pushes one campaign (no cursor = full sync).
    let push = SyncPayload {
        campaigns: vec![Campaign {
            campaign_id: "smoke-c1".into(),
            name: "Smoke Campaign".into(),
            next_session_number: 2,
            updated_at: "2026-05-27T12:00:00Z".into(),
            ..Default::default()
        }],
        ..Default::default()
    };
    let a: SyncResponse = client
        .post(&endpoint)
        .json(&SyncRequest { client_id: "deviceA".into(), since: None, push })
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    println!("A pushed -> synced_at={} pulled {} campaigns", a.synced_at, a.pull.campaigns.len());
    assert!(a.pull.campaigns.is_empty(), "A must not pull back its own push");

    // Device B (fresh) pulls everything.
    let b: SyncResponse = client
        .post(&endpoint)
        .json(&SyncRequest { client_id: "deviceB".into(), since: None, push: SyncPayload::default() })
        .send()
        .await?
        .error_for_status()?
        .json()
        .await?;
    let names: Vec<&str> = b.pull.campaigns.iter().map(|c| c.name.as_str()).collect();
    println!("B pulled campaigns: {names:?} (synced_at={})", b.synced_at);
    assert!(
        b.pull.campaigns.iter().any(|c| c.campaign_id == "smoke-c1" && c.next_session_number == 2),
        "B must pull device A's campaign intact"
    );

    println!("OK — Rust DTOs round-trip through the sync server.");
    Ok(())
}
