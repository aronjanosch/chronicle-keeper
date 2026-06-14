pub mod artifacts;
pub mod campaigns;
pub mod index;
pub mod migration;
pub mod prompts;
pub mod sessions;
pub mod tags;

use chrono::Utc;

/// Current UTC timestamp as RFC 3339.
pub(crate) fn now() -> String {
    Utc::now().to_rfc3339()
}
