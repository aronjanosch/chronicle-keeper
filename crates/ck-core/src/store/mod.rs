pub mod artifacts;
pub mod campaigns;
pub mod codex;
pub mod sessions;
pub mod tags;

use chrono::Utc;

/// Current UTC timestamp as RFC 3339 — used for sync `updated_at` stamps.
pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}
