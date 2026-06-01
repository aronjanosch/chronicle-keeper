//! Codex entries (Phase 2): structured per-campaign glossary of known names &
//! lore (NPCs, places, factions, items, lore notes). Built by hand or
//! auto-extracted from the summarizer's session metadata. Injected verbatim into
//! every summary prompt alongside the freeform `campaigns.codex` paste box.
//!
//! Soft-delete + dirty + updated_at mirror the campaigns/sessions tables: a
//! deleted entry is kept as `deleted = 1, dirty = 1` so the deletion propagates
//! through sync.

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{CodexEntry, CodexEntryCreate, CodexEntryUpdate};
use crate::store::now;

pub const KINDS: &[&str] = &["pc", "npc", "place", "faction", "item", "lore"];

fn validate_kind(kind: &str) -> AppResult<()> {
    if KINDS.contains(&kind) {
        Ok(())
    } else {
        Err(AppError::BadRequest(format!(
            "Unknown codex kind: {kind}. Expected one of: {}",
            KINDS.join(", ")
        )))
    }
}

fn row_to_entry(row: &rusqlite::Row) -> rusqlite::Result<CodexEntry> {
    Ok(CodexEntry {
        entry_id: row.get("entry_id")?,
        campaign_id: row.get("campaign_id")?,
        name: row.get("name")?,
        kind: row.get("kind")?,
        body: row.get::<_, Option<String>>("body")?.unwrap_or_default(),
        detail: row.get::<_, Option<String>>("detail")?.unwrap_or_default(),
        source: row
            .get::<_, Option<String>>("source")?
            .unwrap_or_else(|| "manual".into()),
        updated_at: row
            .get::<_, Option<String>>("updated_at")?
            .unwrap_or_default(),
    })
}

const COLS: &str = "entry_id, campaign_id, name, kind, body, detail, source, updated_at";

/// All active (not-deleted) entries for a campaign, ordered for stable display.
pub fn list_entries(conn: &Connection, campaign_id: &str) -> AppResult<Vec<CodexEntry>> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLS} FROM codex_entries \
         WHERE campaign_id = ?1 AND deleted = 0 \
         ORDER BY kind, lower(name)",
    ))?;
    let rows = stmt.query_map(params![campaign_id], row_to_entry)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get_entry(conn: &Connection, entry_id: &str) -> AppResult<Option<CodexEntry>> {
    let r = conn
        .query_row(
            &format!("SELECT {COLS} FROM codex_entries WHERE entry_id = ?1"),
            params![entry_id],
            row_to_entry,
        )
        .optional()?;
    Ok(r)
}

/// Fold every quote-like character to one canonical `'` so quotation marks never
/// split a dedup: ASR/LLM emit `Mac 'the Scrap Jack'` while the hand-written
/// entry has `Mac "the Scrap Jack"` — same name. Single, double, curly variants
/// and backtick all collapse to `'`. (No lowercasing here; the SQL does that.)
fn fold_quotes(s: &str) -> String {
    s.chars()
        .map(|c| match c {
            '"' | '`' | '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}' | '\u{2032}'
            | '\u{2033}' => '\'',
            other => other,
        })
        .collect()
}

/// SQL expression folding the same quote variants on a column, mirroring
/// [`fold_quotes`] so both sides of the dedup comparison match. Wrapped in
/// `lower()` by the caller. (`''''` is an escaped single-quote literal.)
const NAME_FOLD_SQL: &str = "replace(replace(replace(replace(replace(replace(replace(replace(\
     name, '\"', ''''), char(96), ''''), char(8216), ''''), char(8217), ''''), \
     char(8220), ''''), char(8221), ''''), char(8242), ''''), char(8243), '''')";

/// Look up an active entry by (campaign, name, kind) using the dedup key.
/// Match is case-insensitive and quote-variant-insensitive.
fn find_by_natural_key(
    conn: &Connection,
    campaign_id: &str,
    name: &str,
    kind: &str,
) -> AppResult<Option<CodexEntry>> {
    let r = conn
        .query_row(
            &format!(
                "SELECT {COLS} FROM codex_entries \
                 WHERE campaign_id = ?1 AND lower({NAME_FOLD_SQL}) = lower(?2) AND kind = ?3 AND deleted = 0",
            ),
            params![campaign_id, fold_quotes(name), kind],
            row_to_entry,
        )
        .optional()?;
    Ok(r)
}

/// Insert a manual entry. Errors if (campaign, name, kind) already exists.
pub fn create_entry(
    conn: &Connection,
    campaign_id: &str,
    req: &CodexEntryCreate,
) -> AppResult<CodexEntry> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    validate_kind(&req.kind)?;
    if find_by_natural_key(conn, campaign_id, name, &req.kind)?.is_some() {
        return Err(AppError::BadRequest(format!(
            "Codex entry already exists: {name} ({kind})",
            name = name,
            kind = req.kind
        )));
    }
    let entry_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO codex_entries \
         (entry_id, campaign_id, name, kind, body, detail, source, updated_at, deleted, dirty) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 'manual', ?7, 0, 1)",
        params![
            entry_id,
            campaign_id,
            name,
            req.kind,
            req.body.trim(),
            req.detail.trim(),
            now()
        ],
    )?;
    get_entry(conn, &entry_id)?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("entry vanished after insert")))
}

/// Public existence check by natural key — used by import review to flag entries
/// that already live in the codex (so the user knows a save will overwrite).
pub fn exists(conn: &Connection, campaign_id: &str, name: &str, kind: &str) -> AppResult<bool> {
    Ok(find_by_natural_key(conn, campaign_id, name.trim(), kind)?.is_some())
}

/// Manual upsert for import commit: create the entry, or if one already exists
/// for (campaign, name, kind), overwrite its body (the user explicitly chose to
/// replace it with a better write-up). Always `source='manual'`. Returns true
/// iff a new row was created (false = an existing row was replaced).
pub fn upsert_manual(
    conn: &Connection,
    campaign_id: &str,
    req: &CodexEntryCreate,
) -> AppResult<bool> {
    let name = req.name.trim();
    if name.is_empty() {
        return Err(AppError::BadRequest("name is required".into()));
    }
    validate_kind(&req.kind)?;
    if let Some(existing) = find_by_natural_key(conn, campaign_id, name, &req.kind)? {
        conn.execute(
            "UPDATE codex_entries SET body = ?1, detail = ?2, source = 'manual', updated_at = ?3, dirty = 1 \
             WHERE entry_id = ?4",
            params![req.body.trim(), req.detail.trim(), now(), existing.entry_id],
        )?;
        Ok(false)
    } else {
        create_entry(conn, campaign_id, req)?;
        Ok(true)
    }
}

/// Auto-extract upsert: insert a row with `source='auto'` only if no active
/// entry exists for the same (campaign, name, kind). Never overwrites an
/// existing row — so a user-corrected spelling or hand-written body sticks.
/// Returns true iff a new row was inserted.
pub fn upsert_auto(
    conn: &Connection,
    campaign_id: &str,
    name: &str,
    kind: &str,
) -> AppResult<bool> {
    let name = name.trim();
    if name.is_empty() || !KINDS.contains(&kind) {
        return Ok(false);
    }
    if find_by_natural_key(conn, campaign_id, name, kind)?.is_some() {
        return Ok(false);
    }
    // PCs are characters too: the summarizer extracts them under `characters` and
    // they arrive here as `npc`. If a `pc` with this name already exists, that's
    // the same person — skip, don't create a duplicate npc row.
    if kind == "npc" && find_by_natural_key(conn, campaign_id, name, "pc")?.is_some() {
        return Ok(false);
    }
    let entry_id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO codex_entries \
         (entry_id, campaign_id, name, kind, body, detail, source, updated_at, deleted, dirty) \
         VALUES (?1, ?2, ?3, ?4, '', '', 'auto', ?5, 0, 1)",
        params![entry_id, campaign_id, name, kind, now()],
    )?;
    Ok(true)
}

/// Mirror the campaign's party roster into the codex: ensure a `pc` entry exists
/// for each player's character name. Create-only (reuses `upsert_auto`): never
/// clobbers an entry the GM has fleshed out, and never deletes one when a player
/// is dropped from the roster — so PC lore sticks. Characters with no name are
/// skipped (nothing in-world to remember yet).
pub fn sync_pc_entries(
    conn: &Connection,
    campaign_id: &str,
    players: &serde_json::Value,
) -> AppResult<()> {
    let Some(arr) = players.as_array() else {
        return Ok(());
    };
    for p in arr {
        let ch = p
            .get("character_name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .trim();
        if ch.is_empty() {
            continue;
        }
        upsert_auto(conn, campaign_id, ch, "pc")?;
    }
    Ok(())
}

/// Patch fields the caller supplied. A user-edited entry stays editable but its
/// `source` is upgraded to `manual` so future auto-extracts cannot overwrite it.
pub fn update_entry(
    conn: &Connection,
    entry_id: &str,
    req: &CodexEntryUpdate,
) -> AppResult<CodexEntry> {
    let existing = get_entry(conn, entry_id)?
        .ok_or_else(|| AppError::NotFound(format!("Codex entry not found: {entry_id}")))?;
    if let Some(k) = &req.kind {
        validate_kind(k)?;
    }
    let new_name = req.name.as_deref().map(str::trim).map(str::to_string);
    if let Some(n) = &new_name {
        if n.is_empty() {
            return Err(AppError::BadRequest("name cannot be empty".into()));
        }
    }
    // Check the dedup index won't be violated by a rename/retype.
    let candidate_name = new_name.as_deref().unwrap_or(&existing.name);
    let candidate_kind = req.kind.as_deref().unwrap_or(&existing.kind);
    if let Some(clash) =
        find_by_natural_key(conn, &existing.campaign_id, candidate_name, candidate_kind)?
    {
        if clash.entry_id != existing.entry_id {
            return Err(AppError::BadRequest(format!(
                "Another codex entry already uses {candidate_name} ({candidate_kind})"
            )));
        }
    }
    conn.execute(
        "UPDATE codex_entries SET \
            name       = COALESCE(?1, name), \
            kind       = COALESCE(?2, kind), \
            body       = COALESCE(?3, body), \
            detail     = COALESCE(?4, detail), \
            source     = 'manual', \
            updated_at = ?5, \
            dirty      = 1 \
         WHERE entry_id = ?6",
        params![new_name, req.kind, req.body, req.detail, now(), entry_id],
    )?;
    get_entry(conn, entry_id)?
        .ok_or_else(|| AppError::NotFound(format!("Codex entry not found: {entry_id}")))
}

/// Soft-delete: keep the row so the deletion propagates through sync, but hide
/// it from listings and the prompt context.
pub fn delete_entry(conn: &Connection, entry_id: &str) -> AppResult<()> {
    let n = conn.execute(
        "UPDATE codex_entries SET deleted = 1, dirty = 1, updated_at = ?1 WHERE entry_id = ?2",
        params![now(), entry_id],
    )?;
    if n == 0 {
        return Err(AppError::NotFound(format!(
            "Codex entry not found: {entry_id}"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::campaigns;

    fn seed_campaign(conn: &Connection) -> String {
        campaigns::create_campaign(conn, "c1", "Camp", 1).unwrap();
        "c1".into()
    }

    #[test]
    fn create_and_list_entries() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        create_entry(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Aragorn".into(),
                kind: "npc".into(),
                body: "Ranger".into(),
                detail: String::new(),
            },
        )
        .unwrap();
        create_entry(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Rivendell".into(),
                kind: "place".into(),
                body: String::new(),
                detail: String::new(),
            },
        )
        .unwrap();
        let list = list_entries(&conn, &cid).unwrap();
        assert_eq!(list.len(), 2);
        // Ordered by kind then name: "npc" < "place".
        assert_eq!(list[0].name, "Aragorn");
        assert_eq!(list[1].name, "Rivendell");
    }

    #[test]
    fn duplicate_create_rejected() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        create_entry(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Aragorn".into(),
                kind: "npc".into(),
                body: String::new(),
                detail: String::new(),
            },
        )
        .unwrap();
        let err = create_entry(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "aragorn".into(),
                kind: "npc".into(),
                body: String::new(),
                detail: String::new(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }

    #[test]
    fn auto_skips_existing_manual_entry() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        let manual = create_entry(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Aragorn".into(),
                kind: "npc".into(),
                body: "Hand-edited".into(),
                detail: String::new(),
            },
        )
        .unwrap();
        assert!(!upsert_auto(&conn, &cid, "Aragorn", "npc").unwrap());
        let still = get_entry(&conn, &manual.entry_id).unwrap().unwrap();
        assert_eq!(
            still.body, "Hand-edited",
            "manual body must not be overwritten"
        );
        assert_eq!(still.source, "manual");
    }

    #[test]
    fn dedup_folds_quote_variants() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        create_entry(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Mac \"the Scrap Jack\"".into(),
                kind: "npc".into(),
                body: "Junk dealer".into(),
                detail: String::new(),
            },
        )
        .unwrap();
        // Smart/straight quote variants resolve to the same entry — no dup created.
        assert!(exists(&conn, &cid, "Mac \u{2018}the Scrap Jack\u{2019}", "npc").unwrap());
        assert!(!upsert_auto(&conn, &cid, "Mac 'the Scrap Jack'", "npc").unwrap());
        assert_eq!(list_entries(&conn, &cid).unwrap().len(), 1);
    }

    #[test]
    fn auto_npc_skips_existing_pc() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        // PC roster entry (from sync_pc_entries).
        assert!(upsert_auto(&conn, &cid, "Cordy", "pc").unwrap());
        // Summarizer extracts "Cordy" under characters -> arrives as npc: must skip.
        assert!(!upsert_auto(&conn, &cid, "Cordy", "npc").unwrap());
        let list = list_entries(&conn, &cid).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].kind, "pc");
    }

    #[test]
    fn auto_inserts_new_name() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        assert!(upsert_auto(&conn, &cid, "Bree", "place").unwrap());
        let list = list_entries(&conn, &cid).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].source, "auto");
    }

    #[test]
    fn update_promotes_source_to_manual() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        upsert_auto(&conn, &cid, "Bree", "place").unwrap();
        let auto_entry = list_entries(&conn, &cid).unwrap().pop().unwrap();
        let updated = update_entry(
            &conn,
            &auto_entry.entry_id,
            &CodexEntryUpdate {
                body: Some("Town near the Shire".into()),
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(updated.body, "Town near the Shire");
        assert_eq!(updated.source, "manual", "edit promotes source");
    }

    #[test]
    fn upsert_manual_creates_then_replaces_body() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        // First import (e.g. from NPC notes): thin body.
        let created = upsert_manual(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Iron Hand".into(),
                kind: "faction".into(),
                body: "mentioned by Ulric".into(),
                detail: String::new(),
            },
        )
        .unwrap();
        assert!(created);
        assert!(exists(&conn, &cid, "iron hand", "faction").unwrap());
        // Second import (the faction file): better body replaces it, no dup error.
        let created2 = upsert_manual(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Iron Hand".into(),
                kind: "faction".into(),
                body: "Thieves' guild controlling the docks".into(),
                detail: String::new(),
            },
        )
        .unwrap();
        assert!(!created2, "second upsert replaces, does not create");
        let list = list_entries(&conn, &cid).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].body, "Thieves' guild controlling the docks");
    }

    #[test]
    fn soft_delete_hides_from_list_but_allows_recreate() {
        let conn = crate::db::open_in_memory().unwrap();
        let cid = seed_campaign(&conn);
        let e = create_entry(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Sauron".into(),
                kind: "npc".into(),
                body: String::new(),
                detail: String::new(),
            },
        )
        .unwrap();
        delete_entry(&conn, &e.entry_id).unwrap();
        assert!(
            list_entries(&conn, &cid).unwrap().is_empty(),
            "deleted hidden"
        );
        // Re-create with the same natural key succeeds (partial index excludes deleted).
        create_entry(
            &conn,
            &cid,
            &CodexEntryCreate {
                name: "Sauron".into(),
                kind: "npc".into(),
                body: String::new(),
                detail: String::new(),
            },
        )
        .unwrap();
        assert_eq!(list_entries(&conn, &cid).unwrap().len(), 1);
    }
}
