//! First-launch onboarding seed.
//!
//! On a brand-new database the library is empty, which gives a new user nothing
//! to learn from. We seed a single example campaign — an original homebrew
//! setting (no published-IP names), a small codex, and one session that already
//! carries a sample transcript *and* a sample AI summary — so the whole pipeline
//! is visible before the user records anything.
//!
//! It seeds exactly once, tracked by the `example_seeded` config flag. The flag
//! is deliberately *not* part of [`crate::config::default_config`] so reading
//! config never sets it. Once seeded the flag stays set forever, so deleting the
//! example never makes it reappear — the user owns it from then on.

use rusqlite::Connection;
use serde_json::json;

use crate::config;
use crate::error::AppResult;
use crate::models::{CampaignUpdateRequest, CodexEntryCreate};
use crate::store::{artifacts, campaigns, codex, sessions};

const FLAG_KEY: &str = "example_seeded";
const CAMPAIGN_ID: &str = "example-ashfall";

/// Seed the example campaign if this database has never been seeded. Idempotent
/// and safe to call on every launch — the flag short-circuits after the first
/// run. Callers (the desktop shell) should log and swallow any error so a seed
/// failure never blocks startup.
pub fn seed_example_if_first(conn: &Connection) -> AppResult<()> {
    if config::get_value(conn, FLAG_KEY)?.is_some() {
        return Ok(());
    }

    let tx = conn.unchecked_transaction()?;
    seed_inner(&tx)?;
    config::set_value(&tx, FLAG_KEY, "true")?;
    tx.commit()?;
    Ok(())
}

fn seed_inner(conn: &Connection) -> AppResult<()> {
    campaigns::create_campaign(conn, CAMPAIGN_ID, "The Ashfall Compact", 1)?;

    // Setting + party. Passing `players` makes update_campaign auto-create the
    // `pc` codex entries via codex::sync_pc_entries, so we don't add them here.
    let update = CampaignUpdateRequest {
        system: Some("D&D 5e".into()),
        gm: Some("The Keeper".into()),
        gm_pronouns: Some("they/them".into()),
        setting: Some("Embermarch — a frontier of ash plains and dead volcanoes, where the old Compact that bound the fire below is failing.".into()),
        extra_info: Some(
            "This is a sample campaign that ships with Chronicle Keeper so you can see a finished session before recording your own. Delete it whenever you like — it won't come back.".into(),
        ),
        players: Some(json!([
            { "player_name": "Sam", "character_name": "Brannik Stonebellow", "pronouns": "he/him" },
            { "player_name": "Priya", "character_name": "Vesh", "pronouns": "she/her" },
            { "player_name": "Leo", "character_name": "Sister Calla", "pronouns": "she/her" },
        ])),
        ..Default::default()
    };
    campaigns::update_campaign(conn, CAMPAIGN_ID, &update)?;

    // A small codex — the summarizer's memory. These are the named entities the
    // LLM would otherwise mangle from audio; here they're filled in for the demo.
    let entries = [
        ("Mayor Teller Oren", "npc", "Anxious mayor of Cinderhold who hired the party.",
         "The elected head of Cinderhold, a soft-handed merchant out of his depth. He hired the Compact to find out why the warding stones along the rim have gone cold, and he is hiding how little coin the town actually has left."),
        ("Cinderhold", "place", "Walled frontier town built into a dead caldera.",
         "The last settled town before the ash plains. Its homes are cut into the inner wall of an extinct volcano; warding stones ring the rim to keep the deep fire asleep. Trade has dried up since the eastern road closed."),
        ("The Ember Wardens", "faction", "Old order sworn to keep the fire below bound.",
         "A dwindling order that maintains the warding stones and remembers the original Compact. Most townsfolk think they're harmless relics. They are not — and they know the stones are failing."),
        ("The Ashfall Compact", "lore", "The ancient pact that bound the fire beneath Embermarch.",
         "A centuries-old binding between the first settlers and something deep underground. Its terms are half-forgotten and its anchors — the warding stones — are now going dark one by one."),
        ("Brannik's Tuning Hammer", "item", "Dwarven hammer that rings true near live ward-stone.",
         "A family heirloom. When struck near an active warding stone it hums a clear note; near a dead one it falls silent. The party used it to map which stones have failed."),
    ];
    for (name, kind, body, detail) in entries {
        codex::create_entry(
            conn,
            CAMPAIGN_ID,
            &CodexEntryCreate {
                name: name.into(),
                kind: kind.into(),
                body: body.into(),
                detail: detail.into(),
            },
        )?;
    }

    // One session, already recorded, transcribed and summarized so the user sees
    // the whole pipeline as "complete" — tracks + speakers + both artifacts.
    let session = sessions::create_campaign_session(
        conn,
        CAMPAIGN_ID,
        Some(1),
        Some("Session 1: The Cold Stones"),
        Some("2026-05-20"),
    )?;
    // Craig gives one track per speaker. file_path is left empty on purpose — the
    // demo is pre-transcribed, so nothing ever reads the (non-existent) audio.
    let tracks = json!([
        { "id": "1-Sam", "filename": "1-Sam.flac", "file_path": "", "duration": null },
        { "id": "2-Priya", "filename": "2-Priya.flac", "file_path": "", "duration": null },
        { "id": "3-Leo", "filename": "3-Leo.flac", "file_path": "", "duration": null },
        { "id": "4-Keeper", "filename": "4-Keeper.flac", "file_path": "", "duration": null },
    ]);
    sessions::set_tracks(conn, &session.session_id, &tracks)?;
    let speakers = json!([
        { "track_id": "1-Sam", "player_name": "Sam", "character_name": "Brannik Stonebellow", "pronouns": "he/him" },
        { "track_id": "2-Priya", "player_name": "Priya", "character_name": "Vesh", "pronouns": "she/her" },
        { "track_id": "3-Leo", "player_name": "Leo", "character_name": "Sister Calla", "pronouns": "she/her" },
        { "track_id": "4-Keeper", "player_name": "The Keeper", "character_name": "", "pronouns": "they/them" },
    ]);
    sessions::set_speakers(conn, &session.session_id, &speakers)?;
    artifacts::insert_artifact(
        conn,
        &session.session_id,
        "transcript",
        "example",
        "sample",
        SAMPLE_TRANSCRIPT,
    )?;
    artifacts::insert_artifact(
        conn,
        &session.session_id,
        "summary",
        "example",
        "sample",
        SAMPLE_SUMMARY,
    )?;
    Ok(())
}

// Matches the real on-device transcript format (transcript_format.rs):
// a `[Character (Player)]` header line per speaker block, then their lines,
// with a blank line between blocks. The GM voices the NPCs.
const SAMPLE_TRANSCRIPT: &str = r#"[The Keeper]
We open in Cinderhold, in the back room of the Last Lantern. Mayor Oren has a map spread on the table and keeps smoothing it flat even though it's already flat.
"Three of the rim stones went cold this month. Three. My grandfather's whole life, none went cold. I'll pay what I can — please, just go up and look."

[Brannik Stonebellow (Sam)]
I take out the tuning hammer and set it on the table where he can see it.
"If they're dead, this'll tell us which ones. Stone doesn't lie."

[Vesh (Priya)]
While he's talking I'm watching his hands. Is he lying about the coin? I want an Insight check.
That's a nineteen.

[The Keeper]
He's not lying about wanting your help. But he is lying about how much he can pay — the town is nearly broke.

[Sister Calla (Leo)]
I put a hand on the map. "We'll go. But if the Compact is failing, gold is the least of anyone's problems."

[The Keeper]
The three of you climb the rim road at dawn. The first dead stone is cracked clean through, and the ash around it is warm to the touch.
"#;

const SAMPLE_SUMMARY: &str = r#"# Session 1: The Cold Stones

## Recap
The Ashfall Compact gathered in Cinderhold at the request of **Mayor Teller Oren**, who is frightened by three of the rim's warding stones going cold in a single month. The party agreed to investigate the failing wards, and on the climb up the rim road found the first dead stone cracked through — with warm ash gathering around its base.

## Key beats
- **Mayor Oren's plea.** Three rim stones have gone cold this month, unheard of in living memory. He begged the party to investigate.
- **Reading the mayor.** Vesh caught that Oren is sincere about needing help but is hiding how little coin Cinderhold has left — the town is nearly broke.
- **A grim warning.** Sister Calla reframed the job: if the Compact itself is failing, money is the smallest of everyone's worries.
- **The first dead stone.** At dawn the party reached a cracked, cold warding stone; the surrounding ash was warm — a sign the fire below is stirring.

## NPCs
- **Mayor Teller Oren** — anxious, broke, sincere. Wants the wards fixed, can't really pay for it.

## Loot & tools
- **Brannik's tuning hammer** proved its worth: it rings near a live ward-stone and falls silent near a dead one — the party's tool for mapping which stones have failed.

## Threads for next time
- Map the remaining rim stones with the hammer — how many are already cold?
- Why is the ash *warm* around a dead stone?
- Find the **Ember Wardens** — they remember the original Compact and may know why it's unraveling.
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::{artifacts, codex};

    fn fresh_db() -> Connection {
        let conn = crate::db::open_in_memory().unwrap();
        // Keep create_campaign_session's mkdir out of the real app-data dir.
        let tmp = std::env::temp_dir().join("ck-seed-test");
        config::set_value(&conn, "output_root", &tmp.to_string_lossy()).unwrap();
        conn
    }

    #[test]
    fn seeds_once_and_is_idempotent() {
        let conn = fresh_db();
        seed_example_if_first(&conn).unwrap();

        // Campaign + codex (3 PCs auto + 5 explicit = 8) + a session with both artifacts.
        let campaign = campaigns::get_campaign(&conn, CAMPAIGN_ID)
            .unwrap()
            .unwrap();
        assert_eq!(campaign.name, "The Ashfall Compact");
        let entries = codex::list_entries(&conn, CAMPAIGN_ID).unwrap();
        assert_eq!(entries.len(), 8);
        let pcs = entries.iter().filter(|e| e.kind == "pc").count();
        assert_eq!(pcs, 3);

        let sessions = crate::store::sessions::list_campaign_sessions(&conn, CAMPAIGN_ID).unwrap();
        assert_eq!(sessions.len(), 1);
        // Pipeline reads "complete": tracks recorded + both artifacts present.
        assert!(sessions[0].has_tracks);
        let sid = &sessions[0].session_id;
        assert!(artifacts::has_kind(&conn, sid, "transcript").unwrap());
        assert!(artifacts::has_kind(&conn, sid, "summary").unwrap());

        // Second call is a no-op: no duplicate campaign/sessions.
        seed_example_if_first(&conn).unwrap();
        let after = crate::store::sessions::list_campaign_sessions(&conn, CAMPAIGN_ID).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(
            config::get_value(&conn, FLAG_KEY).unwrap().as_deref(),
            Some("true")
        );
    }

    #[test]
    fn does_not_reseed_after_user_deletes() {
        let conn = fresh_db();
        seed_example_if_first(&conn).unwrap();
        campaigns::delete_campaign(&conn, CAMPAIGN_ID).unwrap();
        // Flag persists, so the deleted example never comes back.
        seed_example_if_first(&conn).unwrap();
        assert!(campaigns::get_campaign(&conn, CAMPAIGN_ID)
            .unwrap()
            .is_none());
    }
}
