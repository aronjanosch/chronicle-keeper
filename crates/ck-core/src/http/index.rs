//! Index-backed endpoints: link graph (backlinks panel + diagnostics),
//! full-text search, page tags. All read the per-world `.ck/index.db` cache.

use axum::extract::{Path, Query, State};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::error::AppResult;
use crate::state::AppState;
use crate::store::index;

use super::vault::vault_root;

/// Change counter for the vault — moves when files change outside CK
/// (Obsidian, Finder). The frontend polls this and refreshes on change.
pub async fn seq(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    Ok(Json(json!({ "seq": state.vault_seq(&root)? })))
}

pub async fn links(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        Ok(Json(json!({
            "links": index::all_links(conn)?,
            "unresolved": index::unresolved_count(conn)?,
            "orphans": index::orphan_count(conn)?,
        })))
    })?
}

/// Grouped vault diagnostics for the Explorer panel (Phase 3).
pub async fn diagnostics(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        Ok(Json(
            serde_json::to_value(index::diagnostics(conn, &root)?).unwrap(),
        ))
    })?
}

#[derive(Deserialize)]
pub struct SearchQuery {
    #[serde(default)]
    pub q: String,
    pub kind: Option<String>,
    pub tag: Option<String>,
    pub folder: Option<String>,
    pub edited_after: Option<i64>,
    pub edited_before: Option<i64>,
}

pub async fn search(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Query(query): Query<SearchQuery>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    let facets = index::SearchFacets {
        kind: query.kind.filter(|s| !s.is_empty()),
        tag: query.tag.filter(|s| !s.is_empty()),
        folder: query.folder.filter(|s| !s.is_empty()),
        edited_after: query.edited_after,
        edited_before: query.edited_before,
    };
    state.with_index(&root, |conn| {
        Ok(Json(
            json!({ "results": index::search_faceted(conn, &query.q, &facets)? }),
        ))
    })?
}

#[derive(Deserialize)]
pub struct SessionSearchQuery {
    #[serde(default)]
    pub q: String,
    /// "summaries" (default) or "transcripts".
    pub scope: Option<String>,
}

/// Full-text-ish search over session summaries / raw transcripts (Phase 7d).
/// These records are files, not in the page FTS index, so this is a substring
/// scan rather than a ranked query.
pub async fn session_search(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Query(query): Query<SessionSearchQuery>,
) -> AppResult<Json<Value>> {
    let (root, _) = super::vault::world_cfg(&state, &campaign_id)?;
    let hits = if query.scope.as_deref() == Some("transcripts") {
        crate::session_search::search_transcripts(&root, &query.q)
    } else {
        crate::session_search::search_summaries(&root, &query.q)
    };
    Ok(Json(json!({ "results": hits })))
}

/// Typed relations (Phase 9A): every frontmatter `[[link]]` value, keyed by
/// its frontmatter key as the predicate. Graph edges + reverse-relation rail.
pub async fn relations(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        Ok(Json(json!({ "relations": index::all_relations(conn)? })))
    })?
}

#[derive(Deserialize)]
pub struct VaultQuery {
    pub q: String,
}

/// Dataview-lite (Phase 9C): `LIST FROM #npc WHERE location = [[Ashfall]]`.
/// Parse errors come back as `{ error }` so the render layer can show them inline.
pub async fn query(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
    Query(q): Query<VaultQuery>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        Ok(Json(match index::run_query(conn, &q.q)? {
            Ok(hits) => json!({ "hits": hits }),
            Err(e) => json!({ "error": e }),
        }))
    })?
}

/// World timeline (Phase 11 + 11.5): dated pages sorted on the world's
/// calendar, plus the calendar itself so the frontend can group/label.
/// Sessions with a `world_date` in session.toml join the lane as synthetic
/// `session:<id>` entries (11.5G); each event carries its tags and its
/// `participants`/`location` relations for filtering and chips (11.5E/F).
pub async fn timeline(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let (_, cfg) = super::vault::world_cfg(&state, &campaign_id)?;
    let root = vault_root(&state, &campaign_id)?;
    let mut rows = state.with_index(&root, index::all_frontmatter)??;
    rows.extend(
        state
            .with_db(|conn| crate::store::sessions::world_dated_session_rows(conn, &campaign_id))?,
    );
    let mut events = crate::timeline::world_events(rows, &cfg.calendar);
    let (meta, relations) = state.with_index(&root, |conn| {
        let meta = index::page_meta(conn)?;
        let relations = index::all_relations(conn)?;
        AppResult::Ok((meta, relations))
    })??;
    for ev in &mut events {
        let Some(path) = ev["path"].as_str().map(str::to_string) else {
            continue;
        };
        if let Some((_, tags)) = meta.get(&path) {
            ev["tags"] = json!(tags);
        }
        let links: Vec<Value> = relations
            .iter()
            .filter(|r| {
                r.source_path == path
                    && matches!(r.predicate.as_str(), "participants" | "participant" | "location")
            })
            .map(|r| {
                json!({ "predicate": r.predicate, "label": r.link_text, "path": r.target_path })
            })
            .collect();
        if !links.is_empty() {
            ev["links"] = json!(links);
        }
    }
    Ok(Json(json!({
        "events": events,
        "calendar": { "months": cfg.calendar.months, "eras": cfg.calendar.eras },
    })))
}

pub async fn tags(
    State(state): State<AppState>,
    Path(campaign_id): Path<String>,
) -> AppResult<Json<Value>> {
    let root = vault_root(&state, &campaign_id)?;
    state.with_index(&root, |conn| {
        let tags: Vec<Value> = index::tag_counts(conn)?
            .into_iter()
            .map(|(tag, count)| json!({ "tag": tag, "count": count }))
            .collect();
        Ok(Json(json!({ "tags": tags })))
    })?
}
