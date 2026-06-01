//! Summary prompt templates: the user-managed library of system prompts shown in
//! the Summarize screen's template picker and managed in Settings. Two builtins
//! (EN/DE) are seeded on first run; the user can add, edit, delete (including the
//! builtins) and restore the builtins. Local-only — not synced, like provider
//! keys and config.

use rusqlite::{params, Connection, OptionalExtension};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::models::{PromptTemplate, PromptTemplateCreate, PromptTemplateUpdate};
use crate::prompts::BUILTIN_TEMPLATES;
use crate::store::now;

const COLS: &str = "id, label, text, builtin, sort_order";

fn row_to_template(r: &rusqlite::Row) -> rusqlite::Result<PromptTemplate> {
    Ok(PromptTemplate {
        id: r.get("id")?,
        label: r.get("label")?,
        text: r.get("text")?,
        builtin: r.get::<_, i64>("builtin")? != 0,
        sort_order: r.get("sort_order")?,
    })
}

const SEED_FLAG: &str = "prompt_templates_seeded";

fn mark_seeded(conn: &Connection) -> AppResult<()> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES (?1, '1') \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![SEED_FLAG],
    )?;
    Ok(())
}

/// Insert any missing builtin rows by their fixed id. Used by first-run seeding
/// and "restore defaults". `INSERT OR IGNORE` means an existing (possibly edited)
/// builtin is never clobbered — restore only brings back ones the user deleted.
fn insert_builtins(conn: &Connection) -> AppResult<()> {
    for (i, (id, label, text)) in BUILTIN_TEMPLATES.iter().enumerate() {
        conn.execute(
            "INSERT OR IGNORE INTO prompt_templates \
             (id, label, text, builtin, sort_order, updated_at) \
             VALUES (?1, ?2, ?3, 1, ?4, ?5)",
            params![id, label, text, i as i64, now()],
        )?;
    }
    Ok(())
}

/// Seed the builtins once. The `prompt_templates_seeded` config flag makes this
/// idempotent *and* lets a user permanently delete a builtin — without the flag
/// we'd re-create it on every boot.
fn seed_once(conn: &Connection) -> AppResult<()> {
    let seeded: Option<String> = conn
        .query_row(
            "SELECT value FROM config WHERE key = ?1",
            params![SEED_FLAG],
            |r| r.get(0),
        )
        .optional()?;
    if seeded.as_deref() == Some("1") {
        return Ok(());
    }
    insert_builtins(conn)?;
    mark_seeded(conn)
}

pub fn list(conn: &Connection) -> AppResult<Vec<PromptTemplate>> {
    seed_once(conn)?;
    let mut stmt =
        conn.prepare(&format!("SELECT {COLS} FROM prompt_templates ORDER BY sort_order, id"))?;
    let rows = stmt.query_map([], row_to_template)?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get(conn: &Connection, id: &str) -> AppResult<Option<PromptTemplate>> {
    let r = conn
        .query_row(
            &format!("SELECT {COLS} FROM prompt_templates WHERE id = ?1"),
            params![id],
            row_to_template,
        )
        .optional()?;
    Ok(r)
}

pub fn create(conn: &Connection, req: &PromptTemplateCreate) -> AppResult<PromptTemplate> {
    seed_once(conn)?;
    let label = req.label.trim();
    if label.is_empty() {
        return Err(AppError::BadRequest("label is required".into()));
    }
    if req.text.trim().is_empty() {
        return Err(AppError::BadRequest("text is required".into()));
    }
    let next: i64 = conn.query_row(
        "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM prompt_templates",
        [],
        |r| r.get(0),
    )?;
    let id = Uuid::new_v4().to_string();
    conn.execute(
        "INSERT INTO prompt_templates (id, label, text, builtin, sort_order, updated_at) \
         VALUES (?1, ?2, ?3, 0, ?4, ?5)",
        params![id, label, req.text, next, now()],
    )?;
    get(conn, &id)?
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("template vanished after insert")))
}

pub fn update(
    conn: &Connection,
    id: &str,
    req: &PromptTemplateUpdate,
) -> AppResult<PromptTemplate> {
    get(conn, id)?.ok_or_else(|| AppError::NotFound(format!("Prompt template not found: {id}")))?;
    let new_label = match &req.label {
        Some(l) if l.trim().is_empty() => {
            return Err(AppError::BadRequest("label cannot be empty".into()))
        }
        Some(l) => Some(l.trim().to_string()),
        None => None,
    };
    conn.execute(
        "UPDATE prompt_templates SET \
            label      = COALESCE(?1, label), \
            text       = COALESCE(?2, text), \
            updated_at = ?3 \
         WHERE id = ?4",
        params![new_label, req.text, now(), id],
    )?;
    get(conn, id)?.ok_or_else(|| AppError::NotFound(format!("Prompt template not found: {id}")))
}

/// Hard delete. Builtins are deletable too — the seed flag (set on first run)
/// keeps them gone until the user hits "restore defaults".
pub fn delete(conn: &Connection, id: &str) -> AppResult<()> {
    let n = conn.execute("DELETE FROM prompt_templates WHERE id = ?1", params![id])?;
    if n == 0 {
        return Err(AppError::NotFound(format!(
            "Prompt template not found: {id}"
        )));
    }
    Ok(())
}

/// Re-create any builtin templates the user has deleted, then return the full
/// list. Edited builtins that still exist are left untouched.
pub fn restore_defaults(conn: &Connection) -> AppResult<Vec<PromptTemplate>> {
    insert_builtins(conn)?;
    mark_seeded(conn)?;
    list(conn)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seeds_builtins_on_first_list() {
        let conn = crate::db::open_in_memory().unwrap();
        let list = list(&conn).unwrap();
        assert_eq!(list.len(), 2);
        assert!(list.iter().all(|t| t.builtin));
        assert_eq!(list[0].id, "default-en");
        assert_eq!(list[1].id, "default-de");
    }

    #[test]
    fn deleted_builtin_stays_deleted_across_reseed() {
        let conn = crate::db::open_in_memory().unwrap();
        list(&conn).unwrap(); // seed
        delete(&conn, "default-de").unwrap();
        // A later seed attempt (e.g. next boot) must not bring it back.
        seed_once(&conn).unwrap();
        let list = list(&conn).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].id, "default-en");
    }

    #[test]
    fn restore_defaults_brings_back_deleted_builtin() {
        let conn = crate::db::open_in_memory().unwrap();
        list(&conn).unwrap();
        delete(&conn, "default-de").unwrap();
        let restored = restore_defaults(&conn).unwrap();
        assert!(restored.iter().any(|t| t.id == "default-de"));
    }

    #[test]
    fn restore_defaults_preserves_edited_builtin() {
        let conn = crate::db::open_in_memory().unwrap();
        list(&conn).unwrap();
        update(
            &conn,
            "default-en",
            &PromptTemplateUpdate {
                label: Some("My English".into()),
                text: Some("custom".into()),
            },
        )
        .unwrap();
        restore_defaults(&conn).unwrap();
        let en = get(&conn, "default-en").unwrap().unwrap();
        assert_eq!(en.label, "My English", "edited builtin not clobbered");
        assert_eq!(en.text, "custom");
    }

    #[test]
    fn create_appends_user_template() {
        let conn = crate::db::open_in_memory().unwrap();
        let created = create(
            &conn,
            &PromptTemplateCreate {
                label: "Terse".into(),
                text: "Be brief.".into(),
            },
        )
        .unwrap();
        assert!(!created.builtin);
        let list = list(&conn).unwrap();
        assert_eq!(list.len(), 3);
        assert_eq!(list.last().unwrap().id, created.id, "appended last");
    }

    #[test]
    fn create_rejects_blank() {
        let conn = crate::db::open_in_memory().unwrap();
        let err = create(
            &conn,
            &PromptTemplateCreate {
                label: "  ".into(),
                text: "x".into(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::BadRequest(_)));
    }
}
