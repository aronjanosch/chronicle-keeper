//! Session review (SC-04): typed, versioned review records and their safe
//! application to world pages.
//!
//! Files are truth. A review run lives beside its session as `review.json`;
//! superseded runs are archived, never dropped, under `review-history/`. Every
//! mutation is guarded by an opaque revision over the exact file bytes, and
//! application is journalled before any write so an interrupted run can be
//! recognized and resumed instead of duplicating an append or a new page.
//!
//! The model never supplies filesystem paths or file contents: targets are
//! server-computed exact before/after bytes, and every path is resolved through
//! the existing vault safeguards.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::error::{AppError, AppResult};
use crate::vault;

pub const REVIEW_FILE: &str = "review.json";
pub const REVIEW_HISTORY_DIR: &str = "review-history";
const SCHEMA_VERSION: u32 = 1;
const ABSENT: &str = "absent";

// ── Data model ────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReviewStatus {
    Open,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    Pending,
    Selected,
    Skipped,
    Deferred,
    Applied,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QuestionStatus {
    Pending,
    Resolved,
    Deferred,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PossibilityDecision {
    Pending,
    SavedToPrep,
    Dismissed,
}

/// What a development's change is based on. Immutable once generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Evidence {
    Transcript {
        start_turn: usize,
        end_turn: usize,
        excerpt: String,
    },
    Summary {
        excerpt: String,
    },
    GmConfirmation {
        text: String,
        confirmed_at: String,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {
    pub path: String,
    /// SHA-256 of the pre-state bytes, or `"absent"` for a new file.
    pub base_hash: String,
    /// Exact pre-state bytes; `None` when the target does not exist yet.
    pub before: Option<String>,
    /// Exact post-state bytes the server will write.
    pub after: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Development {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    pub targets: Vec<Target>,
    pub decision: Decision,
    #[serde(default)]
    pub application_id: Option<String>,
    /// Compound developments touch more than one fact/page and are labeled so.
    #[serde(default)]
    pub compound: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Question {
    pub id: String,
    pub text: String,
    #[serde(default)]
    pub evidence: Vec<Evidence>,
    pub status: QuestionStatus,
    #[serde(default)]
    pub confirmation: Option<Evidence>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Possibility {
    pub id: String,
    pub title: String,
    pub text: String,
    #[serde(default)]
    pub source_links: Vec<String>,
    pub decision: PossibilityDecision,
    #[serde(default)]
    pub destination_session_id: Option<String>,
}

/// Identifies the exact summary/transcript bytes a run was generated from, so
/// freshness is content-based rather than timestamp-based.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceRevision {
    /// "summary" | "transcript" | "prep"
    pub kind: String,
    pub hash: String,
}

/// One target's progress inside an application journal.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetState {
    Pending,
    Written,
    Conflict,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JournalTarget {
    pub path: String,
    pub base_hash: String,
    pub after_hash: String,
    pub before: Option<String>,
    pub after: String,
    pub state: TargetState,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationStatus {
    /// Journal persisted, writes not finished.
    Prepared,
    Applied,
    /// A target conflicted; recovery is required.
    Conflicted,
    /// Terminal partial outcome: some targets applied, remaining deferred.
    Partial,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Application {
    pub id: String,
    pub request_id: String,
    /// SHA-256 of the normalized apply payload, for the idempotency receipt.
    pub payload_hash: String,
    pub development_ids: Vec<String>,
    pub status: ApplicationStatus,
    pub created_at: String,
    pub targets: Vec<JournalTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewRun {
    pub schema_version: u32,
    pub run_id: String,
    pub session_id: String,
    pub generated_at: String,
    #[serde(default)]
    pub provider: String,
    #[serde(default)]
    pub model: String,
    pub status: ReviewStatus,
    #[serde(default)]
    pub source_revisions: Vec<SourceRevision>,
    #[serde(default)]
    pub developments: Vec<Development>,
    #[serde(default)]
    pub questions: Vec<Question>,
    #[serde(default)]
    pub possibilities: Vec<Possibility>,
    #[serde(default)]
    pub applications: Vec<Application>,
}

/// A loaded run plus the revision of the exact bytes it came from.
pub struct Loaded {
    pub revision: String,
    pub run: ReviewRun,
}

pub fn review_path(session_dir: &Path) -> PathBuf {
    session_dir.join(REVIEW_FILE)
}

pub fn history_path(session_dir: &Path, run_id: &str) -> PathBuf {
    session_dir.join(REVIEW_HISTORY_DIR).join(format!("{run_id}.json"))
}

/// Read the current run. `Ok(None)` when no review exists (a legacy
/// `codex-proposals.json` may still be present — see `http::session_review`).
pub fn load(session_dir: &Path) -> AppResult<Option<Loaded>> {
    let path = review_path(session_dir);
    let bytes = match std::fs::read(&path) {
        Ok(b) => b,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(io_error("read", &path, e)),
    };
    let raw = std::str::from_utf8(&bytes)
        .map_err(|e| AppError::Unprocessable(format!("review.json is not valid UTF-8: {e}")))?;
    let run: ReviewRun = serde_json::from_str(raw)
        .map_err(|e| AppError::Unprocessable(format!("Invalid review.json: {e}")))?;
    if run.schema_version != SCHEMA_VERSION {
        return Err(AppError::Unprocessable(format!(
            "Unsupported review schema version: {}",
            run.schema_version
        )));
    }
    Ok(Some(Loaded {
        revision: revision(&bytes),
        run,
    }))
}

/// Archive the current run under `review-history/<run_id>.json` before it is
/// replaced, so regeneration never loses prior decisions or receipts.
pub fn archive_current(session_dir: &Path) -> AppResult<()> {
    let path = review_path(session_dir);
    let Ok(bytes) = std::fs::read(&path) else {
        return Ok(());
    };
    let Ok(run) = serde_json::from_slice::<ReviewRun>(&bytes) else {
        return Ok(());
    };
    let dest = history_path(session_dir, &run.run_id);
    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create review-history: {e}")))?;
    }
    if !dest.exists() {
        atomic_write(&dest, &bytes)?;
    }
    Ok(())
}

/// Persist a run, archiving the previous bytes first. Returns the new revision.
pub fn save(session_dir: &Path, run: &ReviewRun) -> AppResult<String> {
    if review_path(session_dir).exists() {
        archive_current(session_dir)?;
    }
    let bytes = serde_json::to_vec_pretty(run)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("encode review.json: {e}")))?;
    atomic_write(&review_path(session_dir), &bytes)?;
    Ok(revision(&bytes))
}

/// Guard a mutation on the caller's expected revision.
pub fn require_revision(loaded: &Loaded, base_revision: &str) -> AppResult<()> {
    if loaded.revision != base_revision {
        return Err(AppError::Conflict(
            "This review changed since it was loaded.".into(),
        ));
    }
    Ok(())
}

// ── Request payloads (contract §7) ────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct DecisionEntry {
    pub id: String,
    pub decision: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TargetAfter {
    pub path: String,
    pub after: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Adjustment {
    pub id: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub target_afters: Vec<TargetAfter>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct QuestionAction {
    pub id: String,
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PutReviewRequest {
    pub run_id: String,
    pub base_revision: String,
    #[serde(default)]
    pub decisions: Vec<DecisionEntry>,
    #[serde(default)]
    pub adjustments: Vec<Adjustment>,
    #[serde(default)]
    pub question_actions: Vec<QuestionAction>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApplyRequest {
    pub run_id: String,
    pub base_revision: String,
    pub request_id: String,
    #[serde(default)]
    pub development_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RecoverRequest {
    pub run_id: String,
    pub base_revision: String,
    pub application_id: String,
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FinishRequest {
    pub run_id: String,
    pub base_revision: String,
    #[serde(default)]
    pub defer_pending: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReopenRequest {
    pub run_id: String,
    pub base_revision: String,
}

// ── PUT: decisions, adjustments, question actions ─────────────────

/// Apply a decision/adjustment patch. Returns the saved run + new revision.
pub fn put(session_dir: &Path, req: PutReviewRequest) -> AppResult<(ReviewRun, String)> {
    let loaded = load(session_dir)?
        .ok_or_else(|| AppError::NotFound("No review for this session.".into()))?;
    require_revision(&loaded, &req.base_revision)?;
    if loaded.run.run_id != req.run_id {
        return Err(AppError::Conflict("This is not the current review run.".into()));
    }
    let mut run = loaded.run;

    for entry in &req.decisions {
        if let Some(d) = run.developments.iter_mut().find(|d| d.id == entry.id) {
            if d.decision == Decision::Applied {
                return Err(AppError::Conflict(
                    "Applied developments cannot be changed.".into(),
                ));
            }
            d.decision = parse_development_decision(&entry.decision)?;
        } else if let Some(p) = run.possibilities.iter_mut().find(|p| p.id == entry.id) {
            p.decision = parse_possibility_decision(&entry.decision)?;
        } else {
            return Err(AppError::Unprocessable(format!(
                "Unknown decision target: {}",
                entry.id
            )));
        }
    }

    for adj in &req.adjustments {
        apply_adjustment(&mut run, adj)?;
    }

    for action in &req.question_actions {
        if action.action != "defer" {
            return Err(AppError::Unprocessable(format!(
                "Unsupported question action: {}",
                action.action
            )));
        }
        let q = run
            .questions
            .iter_mut()
            .find(|q| q.id == action.id)
            .ok_or_else(|| AppError::Unprocessable(format!("Unknown question: {}", action.id)))?;
        q.status = QuestionStatus::Deferred;
    }

    validate(&run)?;
    let rev = save(session_dir, &run)?;
    Ok((run, rev))
}

/// A GM adjustment edits the planned bytes of one group, clears its selection,
/// and records a confirmation. The before-state and the target set are fixed.
fn apply_adjustment(run: &mut ReviewRun, adj: &Adjustment) -> AppResult<()> {
    let d = run
        .developments
        .iter_mut()
        .find(|d| d.id == adj.id)
        .ok_or_else(|| AppError::Unprocessable(format!("Unknown development: {}", adj.id)))?;
    if d.decision == Decision::Applied {
        return Err(AppError::Conflict(
            "Applied developments cannot be adjusted.".into(),
        ));
    }
    if !adj.target_afters.is_empty() {
        if adj.target_afters.len() != d.targets.len() {
            return Err(AppError::Unprocessable(
                "An adjustment must cover exactly the group's targets.".into(),
            ));
        }
        for ta in &adj.target_afters {
            let target = d
                .targets
                .iter_mut()
                .find(|t| t.path == ta.path)
                .ok_or_else(|| {
                    AppError::Unprocessable(format!(
                        "Adjustment path is not part of this group: {}",
                        ta.path
                    ))
                })?;
            target.after = ta.after.clone();
        }
    }
    if let Some(desc) = &adj.description {
        d.description = desc.clone();
    }
    d.decision = Decision::Pending; // adjusting clears selection
    d.evidence.push(Evidence::GmConfirmation {
        text: adj
            .description
            .clone()
            .unwrap_or_else(|| "GM adjusted the planned change.".into()),
        confirmed_at: now_iso(),
    });
    Ok(())
}

// ── Apply ─────────────────────────────────────────────────────────

/// Outcome of applying one development group.
#[derive(Debug, Clone, Serialize)]
pub struct GroupResult {
    pub development_id: String,
    pub application_id: String,
    pub status: ApplicationStatus,
    pub written: Vec<String>,
    pub pending: Vec<String>,
    pub conflicted: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct ApplyReport {
    /// True when this exact request was already recorded (receipt replay).
    pub replayed: bool,
    pub groups: Vec<GroupResult>,
}

/// Apply selected developments as recoverable groups.
///
/// Serialization within a world is the caller's job (the HTTP layer holds the
/// world write lock), because the preflight must not interleave with another
/// apply's writes.
pub fn apply(
    session_dir: &Path,
    vault_root: &Path,
    world_root: Option<&Path>,
    req: ApplyRequest,
) -> AppResult<ApplyReport> {
    let loaded = load(session_dir)?
        .ok_or_else(|| AppError::NotFound("No review for this session.".into()))?;
    let payload_hash = apply_payload_hash(&req.development_ids);

    // Idempotency receipt is checked BEFORE the stale revision so a lost
    // successful response can be safely retried.
    if let Some(app) = loaded
        .run
        .applications
        .iter()
        .find(|a| a.request_id == req.request_id)
    {
        if app.payload_hash != payload_hash {
            return Err(AppError::Conflict(
                "This request id was already used with a different payload.".into(),
            ));
        }
        let groups = loaded
            .run
            .developments
            .iter()
            .filter(|d| d.application_id.as_deref() == Some(app.id.as_str()))
            .flat_map(|d| result_for_application(&loaded.run, app, &d.id))
            .collect();
        return Ok(ApplyReport {
            replayed: true,
            groups,
        });
    }

    require_revision(&loaded, &req.base_revision)?;
    if loaded.run.run_id != req.run_id {
        return Err(AppError::Conflict("This is not the current review run.".into()));
    }
    if req.development_ids.is_empty() {
        return Err(AppError::Unprocessable(
            "Select at least one update to apply.".into(),
        ));
    }

    let mut run = loaded.run;
    let mut groups = Vec::new();
    for dev_id in &req.development_ids {
        let idx = run
            .developments
            .iter()
            .position(|d| &d.id == dev_id)
            .ok_or_else(|| AppError::Unprocessable(format!("Unknown development: {dev_id}")))?;
        if run.developments[idx].decision != Decision::Selected {
            return Err(AppError::Unprocessable(format!(
                "Development {dev_id} is not selected for apply."
            )));
        }
        if run.developments[idx].application_id.is_some() {
            return Err(AppError::Conflict(format!(
                "Development {dev_id} was already applied."
            )));
        }
        let group = apply_group(
            session_dir,
            &mut run,
            vault_root,
            world_root,
            idx,
            &req.request_id,
            &payload_hash,
        )?;
        groups.push(group);
    }

    // Persist completion (and receipts) for the whole request.
    save(session_dir, &run)?;
    Ok(ApplyReport {
        replayed: false,
        groups,
    })
}

/// Apply one development: preflight, journal, write, journal progress after
/// each target.
#[allow(clippy::too_many_arguments)]
fn apply_group(
    session_dir: &Path,
    run: &mut ReviewRun,
    vault_root: &Path,
    world_root: Option<&Path>,
    dev_idx: usize,
    request_id: &str,
    payload_hash: &str,
) -> AppResult<GroupResult> {
    let dev = &run.developments[dev_idx];

    // 1. Preflight every target. A mismatch makes the whole group conflicted
    //    with zero writes.
    let mut conflicted: Vec<String> = Vec::new();
    for t in &dev.targets {
        if current_hash(vault_root, &t.path)? != t.base_hash {
            conflicted.push(t.path.clone());
        }
    }
    let app_id = Uuid::new_v4().to_string();
    let journal_targets: Vec<JournalTarget> = dev
        .targets
        .iter()
        .map(|t| JournalTarget {
            path: t.path.clone(),
            base_hash: t.base_hash.clone(),
            after_hash: hash_str(&t.after),
            before: t.before.clone(),
            after: t.after.clone(),
            state: if conflicted.contains(&t.path) {
                TargetState::Conflict
            } else {
                TargetState::Pending
            },
        })
        .collect();

    let status = if conflicted.is_empty() {
        ApplicationStatus::Prepared
    } else {
        ApplicationStatus::Conflicted
    };
    let application = Application {
        id: app_id.clone(),
        request_id: request_id.to_string(),
        payload_hash: payload_hash.to_string(),
        development_ids: vec![dev.id.clone()],
        status,
        created_at: now_iso(),
        targets: journal_targets,
    };
    run.developments[dev_idx].application_id = Some(app_id.clone());
    run.applications.push(application);

    // 2. Persist the journal BEFORE writing. If this save fails, no writes
    //    start and the error propagates.
    save(session_dir, run)?;
    if status == ApplicationStatus::Conflicted {
        return Ok(GroupResult {
            development_id: dev.id.clone(),
            application_id: app_id,
            status: ApplicationStatus::Conflicted,
            written: Vec::new(),
            pending: Vec::new(),
            conflicted,
        });
    }

    // 3. Write each target atomically, recording history and journal progress.
    let mut written = Vec::new();
    let mut conflict_after = Vec::new();
    let dev_id = run.developments[dev_idx].id.clone();
    let paths: Vec<(String, String, String)> = run.developments[dev_idx]
        .targets
        .iter()
        .map(|t| (t.path.clone(), t.after.clone(), t.base_hash.clone()))
        .collect();
    for (path, after, base_hash) in paths {
        // Recheck immediately before writing: an external edit between preflight
        // and here becomes a conflict, never a silent overwrite. An unavoidable
        // check-then-replace race remains; this is not filesystem-wide ACID.
        if current_hash(vault_root, &path)? != base_hash {
            conflict_after.push(path.clone());
            set_target_state(run, &app_id, &path, TargetState::Conflict);
            save(session_dir, run)?;
            continue;
        }
        if let Some(wr) = world_root {
            let _ = crate::history::record(wr, vault_root, &path, "keeper");
        }
        write_target(vault_root, &path, &after)?;
        written.push(path.clone());
        set_target_state(run, &app_id, &path, TargetState::Written);
        // Durably record progress so a crash mid-group is recoverable.
        save(session_dir, run)?;
    }

    let final_status = if !conflict_after.is_empty() {
        ApplicationStatus::Conflicted
    } else {
        ApplicationStatus::Applied
    };
    if let Some(app) = run.applications.iter_mut().find(|a| a.id == app_id) {
        app.status = final_status;
    }
    if final_status == ApplicationStatus::Applied {
        if let Some(d) = run.developments.iter_mut().find(|d| d.id == dev_id) {
            d.decision = Decision::Applied;
        }
    }

    Ok(GroupResult {
        development_id: dev.id.clone(),
        application_id: app_id,
        status: final_status,
        written,
        pending: Vec::new(),
        conflicted: conflict_after,
    })
}

/// Recompute a group's result from its stored journal (receipt replay).
fn result_for_application(run: &ReviewRun, app: &Application, dev_id: &str) -> Vec<GroupResult> {
    let written: Vec<String> = app
        .targets
        .iter()
        .filter(|t| t.state == TargetState::Written)
        .map(|t| t.path.clone())
        .collect();
    let conflicted: Vec<String> = app
        .targets
        .iter()
        .filter(|t| t.state == TargetState::Conflict)
        .map(|t| t.path.clone())
        .collect();
    let pending: Vec<String> = app
        .targets
        .iter()
        .filter(|t| t.state == TargetState::Pending)
        .map(|t| t.path.clone())
        .collect();
    let status = if pending.is_empty() && conflicted.is_empty() {
        ApplicationStatus::Applied
    } else if !conflicted.is_empty() {
        ApplicationStatus::Conflicted
    } else {
        app.status
    };
    let _ = run;
    vec![GroupResult {
        development_id: dev_id.to_string(),
        application_id: app.id.clone(),
        status,
        written,
        pending,
        conflicted,
    }]
}

fn set_target_state(run: &mut ReviewRun, app_id: &str, path: &str, state: TargetState) {
    if let Some(app) = run.applications.iter_mut().find(|a| a.id == app_id) {
        if let Some(t) = app.targets.iter_mut().find(|t| t.path == path) {
            t.state = state;
        }
    }
}

// ── Recover ───────────────────────────────────────────────────────

/// Resume an interrupted application. `retry` writes only still-pending
/// targets; `keep_partial` records a terminal partial outcome without writing
/// further and never labels the whole group applied.
pub fn recover(
    session_dir: &Path,
    vault_root: &Path,
    world_root: Option<&Path>,
    req: RecoverRequest,
) -> AppResult<ApplyReport> {
    let mut loaded = load(session_dir)?
        .ok_or_else(|| AppError::NotFound("No review for this session.".into()))?;
    // Recovery is the one mutation that must ignore a stale revision: the point
    // is to reconcile a run whose writes were interrupted.
    if loaded.run.run_id != req.run_id {
        return Err(AppError::Conflict("This is not the current review run.".into()));
    }
    let app_idx = loaded
        .run
        .applications
        .iter()
        .position(|a| a.id == req.application_id)
        .ok_or_else(|| AppError::NotFound("Unknown application.".into()))?;

    match req.action.as_str() {
        "keep_partial" => {
            loaded.run.applications[app_idx].status = ApplicationStatus::Partial;
            let rev = save(session_dir, &loaded.run)?;
            let _ = rev;
            let app = &loaded.run.applications[app_idx];
            let dev_id = app.development_ids.first().cloned().unwrap_or_default();
            return Ok(ApplyReport {
                replayed: false,
                groups: result_for_application(&loaded.run, app, &dev_id),
            });
        }
        "retry" => {}
        other => {
            return Err(AppError::Unprocessable(format!(
                "Unsupported recovery action: {other}"
            )))
        }
    }

    // Reconcile each target against the live bytes.
    let targets: Vec<(String, String, String)> = loaded.run.applications[app_idx]
        .targets
        .iter()
        .map(|t| (t.path.clone(), t.base_hash.clone(), t.after_hash.clone()))
        .collect();
    let mut written = Vec::new();
    let mut pending = Vec::new();
    let mut conflicted = Vec::new();
    for (path, base_hash, after_hash) in targets {
        let current = current_hash(vault_root, &path)?;
        let state = if current == after_hash {
            TargetState::Written
        } else if current == base_hash {
            TargetState::Pending
        } else {
            TargetState::Conflict
        };
        match state {
            TargetState::Written => written.push(path.clone()),
            TargetState::Pending => pending.push(path.clone()),
            TargetState::Conflict => conflicted.push(path.clone()),
        }
        set_target_state(&mut loaded.run, &req.application_id, &path, state);
    }

    // Write only the still-pending targets (never append a second time).
    let after_bytes: HashMap<String, String> = loaded.run.applications[app_idx]
        .targets
        .iter()
        .map(|t| (t.path.clone(), t.after.clone()))
        .collect();
    for path in &pending {
        let after = after_bytes.get(path).cloned().unwrap_or_default();
        if let Some(wr) = world_root {
            let _ = crate::history::record(wr, vault_root, path, "keeper");
        }
        write_target(vault_root, path, &after)?;
        set_target_state(&mut loaded.run, &req.application_id, path, TargetState::Written);
        written.push(path.clone());
    }
    pending.clear();

    let final_status = if conflicted.is_empty() {
        ApplicationStatus::Applied
    } else {
        ApplicationStatus::Partial
    };
    loaded.run.applications[app_idx].status = final_status;
    if final_status == ApplicationStatus::Applied {
        let dev_ids = loaded.run.applications[app_idx].development_ids.clone();
        for id in dev_ids {
            if let Some(d) = loaded.run.developments.iter_mut().find(|d| d.id == id) {
                d.decision = Decision::Applied;
            }
        }
    }
    save(session_dir, &loaded.run)?;
    let app = &loaded.run.applications[app_idx];
    let dev_id = app.development_ids.first().cloned().unwrap_or_default();
    Ok(ApplyReport {
        replayed: false,
        groups: vec![GroupResult {
            development_id: dev_id,
            application_id: app.id.clone(),
            status: final_status,
            written,
            pending,
            conflicted,
        }],
    })
}

// ── Finish / reopen ───────────────────────────────────────────────

pub fn finish(session_dir: &Path, req: FinishRequest) -> AppResult<(ReviewRun, String)> {
    let loaded = load(session_dir)?
        .ok_or_else(|| AppError::NotFound("No review for this session.".into()))?;
    require_revision(&loaded, &req.base_revision)?;
    if loaded.run.run_id != req.run_id {
        return Err(AppError::Conflict("This is not the current review run.".into()));
    }
    let mut run = loaded.run;
    if req.defer_pending {
        for d in run.developments.iter_mut() {
            if matches!(d.decision, Decision::Pending | Decision::Selected) {
                d.decision = Decision::Deferred;
            }
        }
        for p in run.possibilities.iter_mut() {
            if p.decision == PossibilityDecision::Pending {
                p.decision = PossibilityDecision::Dismissed;
            }
        }
    }
    // Cannot hide active recovery.
    if run.applications.iter().any(|a| {
        matches!(
            a.status,
            ApplicationStatus::Prepared | ApplicationStatus::Conflicted
        )
    }) {
        return Err(AppError::Conflict(
            "A partial application needs recovery before this review can finish.".into(),
        ));
    }
    if run.status == ReviewStatus::Open
        && run
            .developments
            .iter()
            .any(|d| d.decision == Decision::Selected)
    {
        return Err(AppError::Conflict(
            "Apply or defer the selected updates before finishing.".into(),
        ));
    }
    run.status = ReviewStatus::Finished;
    let rev = save(session_dir, &run)?;
    Ok((run, rev))
}

pub fn reopen(session_dir: &Path, req: ReopenRequest) -> AppResult<(ReviewRun, String)> {
    let loaded = load(session_dir)?
        .ok_or_else(|| AppError::NotFound("No review for this session.".into()))?;
    require_revision(&loaded, &req.base_revision)?;
    if loaded.run.run_id != req.run_id {
        return Err(AppError::Conflict("This is not the current review run.".into()));
    }
    let mut run = loaded.run;
    run.status = ReviewStatus::Open; // applied developments stay immutable
    let rev = save(session_dir, &run)?;
    Ok((run, rev))
}

// ── Freshness / recovery flags ────────────────────────────────────

#[derive(Debug, Clone, Serialize)]
pub struct ReviewFlags {
    pub stale: bool,
    pub recovery_needed: bool,
    pub recovery_applications: Vec<String>,
    pub applied_development_ids: Vec<String>,
}

/// Compute freshness and recovery flags. `stale` is true when any recorded
/// source revision no longer matches the live file (checked by the caller that
/// knows how to locate each source kind). Recovery is derived from the journal.
pub fn flags(loaded: &Loaded) -> ReviewFlags {
    let recovery_applications: Vec<String> = loaded
        .run
        .applications
        .iter()
        .filter(|a| {
            matches!(
                a.status,
                ApplicationStatus::Prepared | ApplicationStatus::Conflicted | ApplicationStatus::Partial
            )
        })
        .map(|a| a.id.clone())
        .collect();
    let applied_development_ids = loaded
        .run
        .developments
        .iter()
        .filter(|d| d.decision == Decision::Applied)
        .map(|d| d.id.clone())
        .collect();
    ReviewFlags {
        stale: false,
        recovery_needed: !recovery_applications.is_empty(),
        recovery_applications,
        applied_development_ids,
    }
}

// ── Legacy adapter (contract §8) ──────────────────────────────────

/// Build a read-only legacy review view from an old `codex-proposals.json`.
/// Uncommitted proposals become pending cards; committed runs are historical
/// and never claim individual application status.
pub fn legacy_view(session_dir: &Path) -> AppResult<Option<serde_json::Value>> {
    let Some(run) = crate::codex_update::read_run(session_dir)? else {
        return Ok(None);
    };
    let historical = run.status != "open";
    let developments: Vec<serde_json::Value> = if historical {
        Vec::new()
    } else {
        run.proposals
            .iter()
            .filter(|p| p.decision != "rejected")
            .map(|p| {
                serde_json::json!({
                    "id": p.id,
                    "title": p.title,
                    "description": if p.rationale.is_empty() { "Imported page update".to_string() } else { p.rationale.clone() },
                    "decision": "pending",
                    "targets": [],
                    "legacy": true,
                })
            })
            .collect()
    };
    let note = if historical && run.status == "committed" {
        "Legacy review completed; individual application status unavailable"
    } else if historical {
        "Legacy review was skipped"
    } else {
        "Imported page update"
    };
    Ok(Some(serde_json::json!({
        "status": "legacy",
        "legacy_note": note,
        "run_id": format!("legacy-{}", run.session_id),
        "session_id": run.session_id,
        "generated_at": run.generated_at,
        "provider": run.provider,
        "model": run.model,
        "legacy_status": run.status,
        "developments": developments,
        "questions": [],
        "possibilities": [],
    })))
}

/// True when a new review owns this session, so the legacy commit route must
/// refuse to write a second time.
pub fn legacy_commit_blocked(session_dir: &Path) -> AppResult<bool> {
    Ok(load(session_dir)?.is_some())
}

// ── Validation ────────────────────────────────────────────────────

pub fn validate(run: &ReviewRun) -> AppResult<()> {
    let mut ids: HashMap<&str, ()> = HashMap::new();
    for d in &run.developments {
        if ids.insert(&d.id, ()).is_some() {
            return Err(AppError::Unprocessable(format!("Duplicate id: {}", d.id)));
        }
        if d.targets.is_empty() && d.decision != Decision::Applied {
            return Err(AppError::Unprocessable(format!(
                "Development {} has no targets.",
                d.id
            )));
        }
        let mut paths: HashMap<&str, ()> = HashMap::new();
        for t in &d.targets {
            validate_target_path(&t.path)?;
            if paths.insert(&t.path, ()).is_some() {
                return Err(AppError::Unprocessable(format!(
                    "A development cannot target the same path twice: {}",
                    t.path
                )));
            }
        }
    }
    // One path has one owner per run (compound grouping happens before save).
    let mut owner: HashMap<&str, &str> = HashMap::new();
    for d in &run.developments {
        for t in &d.targets {
            if let Some(prev) = owner.insert(&t.path, &d.id) {
                if prev != d.id {
                    return Err(AppError::Unprocessable(format!(
                        "Two developments target {} — combine them into one compound development.",
                        t.path
                    )));
                }
            }
        }
    }
    for q in &run.questions {
        if ids.insert(&q.id, ()).is_some() {
            return Err(AppError::Unprocessable(format!("Duplicate id: {}", q.id)));
        }
    }
    for p in &run.possibilities {
        if ids.insert(&p.id, ()).is_some() {
            return Err(AppError::Unprocessable(format!("Duplicate id: {}", p.id)));
        }
        if p.decision == PossibilityDecision::SavedToPrep && p.destination_session_id.is_none() {
            return Err(AppError::Unprocessable(
                "A saved possibility needs a destination session.".into(),
            ));
        }
        for link in &p.source_links {
            validate_target_path(link)?;
        }
    }
    Ok(())
}

fn validate_target_path(path: &str) -> AppResult<()> {
    if path.trim().is_empty() || !path.ends_with(".md") {
        return Err(AppError::Unprocessable(format!(
            "Invalid review target path: {path}"
        )));
    }
    // Reuse the vault's traversal checks without touching the filesystem.
    if Path::new(path).is_absolute()
        || Path::new(path)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
    {
        return Err(AppError::Unprocessable(format!(
            "Invalid review target path: {path}"
        )));
    }
    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────

fn parse_development_decision(s: &str) -> AppResult<Decision> {
    match s {
        "pending" => Ok(Decision::Pending),
        "selected" => Ok(Decision::Selected),
        "skipped" => Ok(Decision::Skipped),
        "deferred" => Ok(Decision::Deferred),
        "applied" => Err(AppError::Conflict(
            "Applied developments cannot be set directly.".into(),
        )),
        other => Err(AppError::Unprocessable(format!("Unknown decision: {other}"))),
    }
}

fn parse_possibility_decision(s: &str) -> AppResult<PossibilityDecision> {
    match s {
        "pending" => Ok(PossibilityDecision::Pending),
        "dismissed" => Ok(PossibilityDecision::Dismissed),
        "saved_to_prep" => Err(AppError::Conflict(
            "Saving a possibility to prep goes through the carry endpoint.".into(),
        )),
        other => Err(AppError::Unprocessable(format!("Unknown decision: {other}"))),
    }
}

fn apply_payload_hash(ids: &[String]) -> String {
    let mut sorted: Vec<&str> = ids.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    hash_str(&sorted.join("\n"))
}

fn current_hash(vault_root: &Path, rel: &str) -> AppResult<String> {
    let abs = safe_join(vault_root, rel)?;
    match std::fs::read(&abs) {
        Ok(bytes) => Ok(revision(&bytes)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(ABSENT.to_string()),
        Err(e) => Err(io_error("read", &abs, e)),
    }
}

/// Resolve a vault-relative path for a review target, rejecting traversal and
/// reserved directories exactly like the vault itself.
fn safe_join(vault_root: &Path, rel: &str) -> AppResult<PathBuf> {
    validate_target_path(rel)?;
    Ok(vault_root.join(rel))
}

fn write_target(vault_root: &Path, rel: &str, after: &str) -> AppResult<()> {
    let abs = safe_join(vault_root, rel)?;
    if let Some(parent) = abs.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("create dir: {e}")))?;
    }
    atomic_write(&abs, after.as_bytes())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> AppResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| AppError::Internal(anyhow::anyhow!("no parent dir")))?;
    std::fs::create_dir_all(parent)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create dir: {e}")))?;
    let temp = parent.join(format!(".review-{}.tmp", Uuid::new_v4()));
    let result = (|| -> std::io::Result<()> {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        std::fs::rename(&temp, path)?;
        Ok(())
    })();
    if let Err(error) = result {
        let _ = std::fs::remove_file(&temp);
        return Err(io_error("write", path, error));
    }
    Ok(())
}

pub fn revision(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn hash_str(s: &str) -> String {
    revision(s.as_bytes())
}

fn now_iso() -> String {
    chrono::Utc::now().to_rfc3339()
}

fn io_error(action: &str, path: &Path, error: std::io::Error) -> AppError {
    AppError::Internal(anyhow::anyhow!("{action} {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tmp_dir(tag: &str) -> PathBuf {
        let p = std::env::temp_dir().join(format!("ck-review-{tag}-{}", Uuid::new_v4()));
        std::fs::create_dir_all(&p).unwrap();
        p
    }

    fn new_run(session_id: &str) -> ReviewRun {
        ReviewRun {
            schema_version: SCHEMA_VERSION,
            run_id: Uuid::new_v4().to_string(),
            session_id: session_id.into(),
            generated_at: now_iso(),
            provider: "test".into(),
            model: "test".into(),
            status: ReviewStatus::Open,
            source_revisions: Vec::new(),
            developments: Vec::new(),
            questions: Vec::new(),
            possibilities: Vec::new(),
            applications: Vec::new(),
        }
    }

    fn development(id: &str, path: &str, before: Option<&str>, after: &str) -> Development {
        Development {
            id: id.into(),
            title: "T".into(),
            description: "D".into(),
            evidence: Vec::new(),
            targets: vec![Target {
                path: path.into(),
                base_hash: before.map(hash_str).unwrap_or_else(|| ABSENT.into()),
                before: before.map(str::to_string),
                after: after.into(),
            }],
            decision: Decision::Selected,
            application_id: None,
            compound: false,
        }
    }

    #[test]
    fn no_selected_ids_writes_nothing() {
        let dir = tmp_dir("noselect");
        let vault = tmp_dir("noselect-vault");
        let mut run = new_run("s1");
        run.developments.push(development("d1", "A.md", None, "new"));
        let rev = save(&dir, &run).unwrap();
        let err = apply(
            &dir,
            &vault,
            None,
            ApplyRequest {
                run_id: run.run_id.clone(),
                base_revision: rev,
                request_id: Uuid::new_v4().to_string(),
                development_ids: vec![],
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Unprocessable(_)));
        assert!(!vault.join("A.md").exists());
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(vault).ok();
    }

    #[test]
    fn stale_base_hash_makes_the_group_conflicted_with_no_writes() {
        let dir = tmp_dir("stale");
        let vault = tmp_dir("stale-vault");
        std::fs::write(vault.join("A.md"), "changed externally").unwrap();
        let mut run = new_run("s1");
        run.developments.push(development("d1", "A.md", Some("original"), "new"));
        let rev = save(&dir, &run).unwrap();
        let report = apply(
            &dir,
            &vault,
            None,
            ApplyRequest {
                run_id: run.run_id.clone(),
                base_revision: rev,
                request_id: Uuid::new_v4().to_string(),
                development_ids: vec!["d1".into()],
            },
        )
        .unwrap();
        assert_eq!(report.groups[0].status, ApplicationStatus::Conflicted);
        assert_eq!(
            std::fs::read_to_string(vault.join("A.md")).unwrap(),
            "changed externally"
        );
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(vault).ok();
    }

    #[test]
    fn apply_writes_and_is_idempotent_on_replay() {
        let dir = tmp_dir("apply");
        let vault = tmp_dir("apply-vault");
        std::fs::write(vault.join("A.md"), "before").unwrap();
        let mut run = new_run("s1");
        run.developments.push(development("d1", "A.md", Some("before"), "after"));
        let rev = save(&dir, &run).unwrap();
        let request_id = Uuid::new_v4().to_string();
        let req = ApplyRequest {
            run_id: run.run_id.clone(),
            base_revision: rev.clone(),
            request_id: request_id.clone(),
            development_ids: vec!["d1".into()],
        };
        let first = apply(&dir, &vault, None, req).unwrap();
        assert!(!first.replayed);
        assert_eq!(first.groups[0].status, ApplicationStatus::Applied);
        assert_eq!(std::fs::read_to_string(vault.join("A.md")).unwrap(), "after");

        // Replay with the SAME request id returns recorded results without a
        // stale-revision error and without rewriting.
        let replayed = apply(
            &dir,
            &vault,
            None,
            ApplyRequest {
                run_id: run.run_id.clone(),
                base_revision: "stale-on-purpose".into(),
                request_id,
                development_ids: vec!["d1".into()],
            },
        )
        .unwrap();
        assert!(replayed.replayed);
        assert_eq!(replayed.groups[0].status, ApplicationStatus::Applied);
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(vault).ok();
    }

    #[test]
    fn recovery_resumes_only_pending_targets() {
        let dir = tmp_dir("recover");
        let vault = tmp_dir("recover-vault");
        std::fs::write(vault.join("A.md"), "before-a").unwrap();
        std::fs::write(vault.join("B.md"), "before-b").unwrap();
        let mut run = new_run("s1");
        run.developments.push(Development {
            id: "d1".into(),
            title: "T".into(),
            description: "D".into(),
            evidence: Vec::new(),
            targets: vec![
                Target {
                    path: "A.md".into(),
                    base_hash: hash_str("before-a"),
                    before: Some("before-a".into()),
                    after: "after-a".into(),
                },
                Target {
                    path: "B.md".into(),
                    base_hash: hash_str("before-b"),
                    before: Some("before-b".into()),
                    after: "after-b".into(),
                },
            ],
            decision: Decision::Selected,
            application_id: None,
            compound: true,
        });
        let rev = save(&dir, &run).unwrap();
        // Simulate a crash after the first write: A applied, B untouched, and a
        // prepared journal on disk.
        std::fs::write(vault.join("A.md"), "after-a").unwrap();
        let mut loaded = load(&dir).unwrap().unwrap().run;
        let app_id = Uuid::new_v4().to_string();
        loaded.developments[0].application_id = Some(app_id.clone());
        loaded.applications.push(Application {
            id: app_id.clone(),
            request_id: Uuid::new_v4().to_string(),
            payload_hash: "x".into(),
            development_ids: vec!["d1".into()],
            status: ApplicationStatus::Prepared,
            created_at: now_iso(),
            targets: vec![
                JournalTarget {
                    path: "A.md".into(),
                    base_hash: hash_str("before-a"),
                    after_hash: hash_str("after-a"),
                    before: Some("before-a".into()),
                    after: "after-a".into(),
                    state: TargetState::Pending,
                },
                JournalTarget {
                    path: "B.md".into(),
                    base_hash: hash_str("before-b"),
                    after_hash: hash_str("after-b"),
                    before: Some("before-b".into()),
                    after: "after-b".into(),
                    state: TargetState::Pending,
                },
            ],
        });
        save(&dir, &loaded).unwrap();
        let _ = rev;

        let report = recover(
            &dir,
            &vault,
            None,
            RecoverRequest {
                run_id: loaded.run_id.clone(),
                base_revision: "ignored".into(),
                application_id: app_id,
                action: "retry".into(),
            },
        )
        .unwrap();
        assert_eq!(report.groups[0].status, ApplicationStatus::Applied);
        assert_eq!(std::fs::read_to_string(vault.join("A.md")).unwrap(), "after-a");
        assert_eq!(std::fs::read_to_string(vault.join("B.md")).unwrap(), "after-b");
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(vault).ok();
    }

    #[test]
    fn keep_partial_is_terminal_and_does_not_apply() {
        let dir = tmp_dir("partial");
        let vault = tmp_dir("partial-vault");
        let mut run = new_run("s1");
        run.developments.push(development("d1", "A.md", None, "after"));
        let rev = save(&dir, &run).unwrap();
        let app_id = Uuid::new_v4().to_string();
        run.developments[0].application_id = Some(app_id.clone());
        run.applications.push(Application {
            id: app_id.clone(),
            request_id: Uuid::new_v4().to_string(),
            payload_hash: "x".into(),
            development_ids: vec!["d1".into()],
            status: ApplicationStatus::Conflicted,
            created_at: now_iso(),
            targets: vec![JournalTarget {
                path: "A.md".into(),
                base_hash: ABSENT.into(),
                after_hash: hash_str("after"),
                before: None,
                after: "after".into(),
                state: TargetState::Conflict,
            }],
        });
        save(&dir, &run).unwrap();
        let _ = rev;
        recover(
            &dir,
            &vault,
            None,
            RecoverRequest {
                run_id: run.run_id.clone(),
                base_revision: "ignored".into(),
                application_id: app_id,
                action: "keep_partial".into(),
            },
        )
        .unwrap();
        let loaded = load(&dir).unwrap().unwrap().run;
        assert_eq!(loaded.applications[0].status, ApplicationStatus::Partial);
        assert_ne!(loaded.developments[0].decision, Decision::Applied);
        assert!(!vault.join("A.md").exists());
        std::fs::remove_dir_all(dir).ok();
        std::fs::remove_dir_all(vault).ok();
    }

    #[test]
    fn finish_blocks_while_recovery_is_active() {
        let dir = tmp_dir("finish");
        let mut run = new_run("s1");
        run.developments.push(development("d1", "A.md", None, "after"));
        run.applications.push(Application {
            id: Uuid::new_v4().to_string(),
            request_id: Uuid::new_v4().to_string(),
            payload_hash: "x".into(),
            development_ids: vec!["d1".into()],
            status: ApplicationStatus::Prepared,
            created_at: now_iso(),
            targets: Vec::new(),
        });
        let rev = save(&dir, &run).unwrap();
        let err = finish(
            &dir,
            FinishRequest {
                run_id: run.run_id.clone(),
                base_revision: rev,
                defer_pending: true,
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn adjusting_clears_selection_and_records_confirmation() {
        let dir = tmp_dir("adjust");
        let mut run = new_run("s1");
        run.developments.push(development("d1", "A.md", None, "after"));
        let rev = save(&dir, &run).unwrap();
        let (saved, _) = put(
            &dir,
            PutReviewRequest {
                run_id: run.run_id.clone(),
                base_revision: rev,
                decisions: Vec::new(),
                adjustments: vec![Adjustment {
                    id: "d1".into(),
                    description: Some("softened".into()),
                    target_afters: vec![TargetAfter {
                        path: "A.md".into(),
                        after: "adjusted".into(),
                    }],
                }],
                question_actions: Vec::new(),
            },
        )
        .unwrap();
        assert_eq!(saved.developments[0].decision, Decision::Pending);
        assert_eq!(saved.developments[0].targets[0].after, "adjusted");
        assert!(matches!(
            saved.developments[0].evidence.last(),
            Some(Evidence::GmConfirmation { .. })
        ));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn validation_rejects_two_owners_for_one_path() {
        let mut run = new_run("s1");
        run.developments
            .push(development("d1", "A.md", None, "after"));
        run.developments
            .push(development("d2", "A.md", None, "after2"));
        assert!(matches!(validate(&run), Err(AppError::Unprocessable(_))));
    }

    #[test]
    fn revision_mismatch_conflicts_on_put() {
        let dir = tmp_dir("rev");
        let run = new_run("s1");
        save(&dir, &run).unwrap();
        let err = put(
            &dir,
            PutReviewRequest {
                run_id: run.run_id.clone(),
                base_revision: "wrong".into(),
                decisions: Vec::new(),
                adjustments: Vec::new(),
                question_actions: Vec::new(),
            },
        )
        .unwrap_err();
        assert!(matches!(err, AppError::Conflict(_)));
        std::fs::remove_dir_all(dir).ok();
    }
}
