# Chronicle Keeper — Sync Protocol Specification

> Version: 1 · Status: implemented (Sprint 2) · Last updated: 2026-06-01

This document describes the HTTP protocol between the Chronicle Keeper desktop app
and any compatible sync server. The official hosted server is proprietary; this spec
is published so privacy-conscious users can build and self-host a compatible server.

---

## Transport & auth

- **HTTPS** required in production; HTTP acceptable for local dev/self-hosting.
- **Auth**: `Authorization: Bearer <token>` on every request except `/health`.
- Token is a shared secret configured in the app's sync settings and the server's
  `CK_SYNC_TOKEN` environment variable. **No per-user accounts in v1** — one token = one
  data scope. Per-user auth (Stripe-customer-scoped tokens) is a later upgrade; the hosted
  tier stays single-tenant-per-token until then.
- All request/response bodies are `application/json`.

---

## Endpoints

### `GET /health`

Public (no auth). Liveness check.

```
200 OK
{ "status": "ok" }
```

---

### `POST /sync`

Single endpoint for all sync operations. One round-trip per sync cycle.

**Request**

```jsonc
{
  "client_id": "stable-device-uuid",   // generated once per device, stored locally
  "since": "42",                        // opaque cursor from the last response's synced_at; null/omitted = full sync
  "push": {
    "campaigns": [ <Campaign> ],
    "sessions":  [ <Session>  ],
    "artifacts": [ <Artifact> ],
    "codex_entries": [ <CodexEntry> ],
    "deleted_artifact_ids": [ "artifact-uuid", ... ]
  }
}
```

**Response**

```jsonc
{
  "synced_at": "57",                    // opaque server cursor after this merge; echo back as `since` next time
  "pull": {
    "campaigns": [ <Campaign> ],   // server-side changes since `since` not already in push
    "sessions":  [ <Session>  ],
    "artifacts": [ <Artifact> ],
    "codex_entries": [ <CodexEntry> ],
    "deleted_artifact_ids": [ "artifact-uuid", ... ]
  }
}
```

---

## Record schemas

### Campaign

```jsonc
{
  "campaign_id":          "string",         // client-generated, stable
  "name":                 "string",
  "next_session_number":  1,
  "system":               "string",         // e.g. "D&D 5e"
  "gm":                   "string",
  "setting":              "string",
  "default_language":     "en",
  "players": [
    { "player_name": "string", "character_name": "string" }
  ],
  "extra_info":  "string",
  "codex":              "string",           // legacy single freeform glossary (kept for back-compat)
  "codex_notes":        "string",           // JSON-encoded array [{title, body}] of freeform glossary notes
  "recap":              "string",           // LLM-generated "story so far" narrative (markdown)
  "recap_updated_at":   "2026-05-29T00:00:00Z | \"\"",  // when `recap` was last generated ("" = never)
  "updated_at":  "2026-05-27T10:00:00Z",   // client's last-edit time — informational only (see Conflict resolution)
  "deleted":     false                      // true = soft-delete, propagate to other devices
}
```

> Structured codex entries (NPCs/places/factions/items/lore) sync as their own
> top-level `codex_entries` array — see the CodexEntry shape below.

### Session

```jsonc
{
  "session_id":      "string",            // client-generated UUID
  "campaign_id":     "string | null",
  "session_number":  1,
  "title":           "string",
  "date":            "2026-05-27",
  "metadata": {
    "characters": ["string"],
    "locations":  ["string"],
    "events":     ["string"],
    "items":      ["string"],
    "tags":       ["string"]
  },
  "notes":       "string",
  "speakers": [
    {
      "track_id":       "string",
      "player_name":    "string",
      "character_name": "string",
      "pronouns":       "string"
    }
  ],
  "updated_at": "2026-05-27T10:00:00Z",
  "deleted":    false
}
```

### Artifact

Artifacts (transcripts and summaries) are **immutable** — push once, never update.
Delete via `deleted_artifact_ids`. Content is always the full text of the artifact.

```jsonc
{
  "artifact_id": "client-generated-uuid",  // stable, content-addressed recommended
  "session_id":  "string",
  "kind":        "transcript | summary",
  "provider":    "string",                 // e.g. "sherpa", "ollama/llama3.2"
  "model":       "string",
  "content":     "full text content",
  "created_at":  "2026-05-27T10:00:00Z"
}
```

### CodexEntry

A structured per-campaign glossary entry (NPC, place, faction, item, lore, or
player character). Carried in the top-level `codex_entries` array of both push
and pull. Mutable — last push received wins, like campaigns/sessions.

```jsonc
{
  "entry_id":    "client-generated-uuid",   // stable
  "campaign_id": "string",
  "name":        "string",
  "kind":        "pc | npc | place | faction | item | lore",
  "body":        "string",                  // one-line description, fed into summary prompts
  "detail":      "string",                  // longer write-up, shown in the inspector (not fed to summaries)
  "source":      "manual | auto",           // auto = extracted from a summary
  "updated_at":  "2026-05-27T10:00:00Z",
  "deleted":     false
}
```

---

## Conflict resolution

**The server's clock is the only clock that matters.** Every record the server accepts is
stamped with a server-side, monotonically increasing `server_seq` (and a `server_updated_at`
timestamp from the server's own clock). Client `updated_at` is **not** used to decide
conflicts — it is informational ("when the user last edited this"). This makes resolution
immune to client clock skew, which a multi-device offline app cannot otherwise guarantee.

| Record type | Rule |
|---|---|
| Campaign | **Last push received wins.** The server overwrites its stored row with the incoming push and bumps `server_seq`. No timestamp comparison. |
| Session | Same — last push received wins. |
| CodexEntry | Same — last push received wins (keyed on `entry_id`). |
| Artifact | Immutable. If `artifact_id` already exists on the server, the push is silently ignored. |
| Deletions | Server marks the record deleted and bumps `server_seq`. Propagated on pull (soft-deletes inline; `deleted_artifact_ids` for artifacts). |

**Known tradeoff (accepted for v1):** "last push received wins" means a device that edited
a record while offline can overwrite a *newer* edit made on another device that synced first,
once the offline device reconnects. A true last-writer-wins across devices would require
trusting client clocks (skew-prone) or vector clocks (complex). v1 favours simplicity; the
windows for this are small for a single-user, few-devices workload.

Soft-deleted campaigns/sessions (`"deleted": true`) are included in normal push/pull.
Receiving clients remove the record from their local DB and stop displaying it.

---

## Client-side sync behaviour

| Trigger | Action |
|---|---|
| App startup | Full sync if no `since`; incremental otherwise |
| App shutdown | Flush dirty records → sync |
| Every 5 min (configurable) | Incremental sync if sync URL is configured |
| After write | Set the record's local `dirty` flag; sync deferred to next interval |

**Dirty tracking (clock-free):** the local SQLite DB has a `dirty` flag (and `updated_at`,
for display) on campaigns and sessions. Every local write sets `dirty = 1`. A sync pushes
all rows where `dirty = 1`; on a successful response the client clears `dirty = 0` for the
pushed rows. The client never compares its own clock against the server's.

**Pull cursor:** the client stores `since` from the previous response's `synced_at` — an
**opaque server-defined token** (the reference server uses the monotonic `server_seq` as a
decimal string) — and echoes it back next sync. The server returns every record whose
`server_seq` advanced past the cursor, excluding rows just pushed in the same request.
`since` is null/omitted on first sync (full pull).

---

## Self-hosting

If you want to run your own compatible sync server:

1. Implement the two endpoints above (`GET /health`, `POST /sync`).
2. Store campaigns, sessions, and artifacts in any database.
3. Implement conflict resolution as specified (server stamps `server_seq`; last push received wins).
4. Set `CK_SYNC_TOKEN` to a long random secret.
5. Configure the same token in the app's sync settings, along with your server URL.

The official server implementation is proprietary; the protocol is not.

---

## Versioning

The `POST /sync` path may include a version prefix in future (`/v2/sync`) if breaking
changes are needed. The current version is implicitly **v1** — no prefix.
