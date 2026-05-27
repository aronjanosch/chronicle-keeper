use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};

use crate::config::get_config_map;
use crate::error::{AppError, AppResult};
use crate::models::{CampaignDetail, CampaignInfo, CampaignUpdateRequest};
use crate::normalize::normalize_players;

fn default_language(conn: &Connection) -> String {
    get_config_map(conn)
        .ok()
        .and_then(|m| m.get("default_language").cloned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "en".to_string())
}

fn row_to_detail(row: &rusqlite::Row, fallback_lang: &str) -> rusqlite::Result<CampaignDetail> {
    let players_json: String = row.get("players_json")?;
    let players = serde_json::from_str::<Value>(&players_json).unwrap_or_else(|_| json!([]));
    let lang: Option<String> = row.get("default_language")?;
    Ok(CampaignDetail {
        campaign_id: row.get("campaign_id")?,
        name: row.get("name")?,
        next_session_number: row.get("next_session_number")?,
        system: row.get::<_, Option<String>>("system")?.unwrap_or_default(),
        gm: row.get::<_, Option<String>>("gm")?.unwrap_or_default(),
        setting: row.get::<_, Option<String>>("setting")?.unwrap_or_default(),
        default_language: lang.filter(|s| !s.is_empty()).unwrap_or_else(|| fallback_lang.to_string()),
        players: normalize_players(&players),
        extra_info: row.get::<_, Option<String>>("extra_info")?.unwrap_or_default(),
    })
}

pub fn get_campaigns(conn: &Connection) -> AppResult<Vec<CampaignDetail>> {
    let lang = default_language(conn);
    let mut stmt = conn.prepare("SELECT * FROM campaigns ORDER BY name")?;
    let rows = stmt.query_map([], |r| row_to_detail(r, &lang))?;
    let mut out = Vec::new();
    for r in rows {
        out.push(r?);
    }
    Ok(out)
}

pub fn get_campaign(conn: &Connection, campaign_id: &str) -> AppResult<Option<CampaignDetail>> {
    let lang = default_language(conn);
    let c = conn
        .query_row(
            "SELECT * FROM campaigns WHERE campaign_id = ?1",
            params![campaign_id],
            |r| row_to_detail(r, &lang),
        )
        .optional()?;
    Ok(c)
}

pub fn campaign_infos(conn: &Connection) -> AppResult<Vec<CampaignInfo>> {
    Ok(get_campaigns(conn)?
        .into_iter()
        .map(|c| CampaignInfo {
            campaign_id: c.campaign_id,
            name: c.name,
            next_session_number: c.next_session_number,
        })
        .collect())
}

pub fn create_campaign(
    conn: &Connection,
    campaign_id: &str,
    name: &str,
    start_session_number: i64,
) -> AppResult<CampaignDetail> {
    if let Some(existing) = get_campaign(conn, campaign_id)? {
        return Ok(existing);
    }
    let lang = default_language(conn);
    conn.execute(
        "INSERT INTO campaigns \
         (campaign_id, name, next_session_number, system, gm, setting, default_language, players_json, extra_info) \
         VALUES (?1, ?2, ?3, '', '', '', ?4, '[]', '')",
        params![campaign_id, name, start_session_number, lang],
    )?;
    set_current_campaign_id(conn, campaign_id)?;
    get_campaign(conn, campaign_id)?.ok_or_else(|| AppError::Internal(anyhow::anyhow!("campaign vanished")))
}

pub fn update_campaign(
    conn: &Connection,
    campaign_id: &str,
    req: &CampaignUpdateRequest,
) -> AppResult<CampaignDetail> {
    if get_campaign(conn, campaign_id)?.is_none() {
        return Err(AppError::NotFound(format!("Campaign not found: {campaign_id}")));
    }
    let mut sets: Vec<String> = Vec::new();
    let mut vals: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    macro_rules! push_str {
        ($field:literal, $opt:expr) => {
            if let Some(v) = &$opt {
                sets.push(format!("{} = ?", $field));
                vals.push(Box::new(v.clone()));
            }
        };
    }
    push_str!("name", req.name);
    push_str!("system", req.system);
    push_str!("gm", req.gm);
    push_str!("setting", req.setting);
    push_str!("default_language", req.default_language);
    push_str!("extra_info", req.extra_info);
    if let Some(n) = req.next_session_number {
        sets.push("next_session_number = ?".into());
        vals.push(Box::new(n));
    }
    if let Some(players) = &req.players {
        sets.push("players_json = ?".into());
        vals.push(Box::new(normalize_players(players).to_string()));
    }
    if !sets.is_empty() {
        vals.push(Box::new(campaign_id.to_string()));
        let sql = format!("UPDATE campaigns SET {} WHERE campaign_id = ?", sets.join(", "));
        let refs: Vec<&dyn rusqlite::ToSql> = vals.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, refs.as_slice())?;
    }
    get_campaign(conn, campaign_id)?.ok_or_else(|| AppError::NotFound(format!("Campaign not found: {campaign_id}")))
}

pub fn next_session_number(conn: &Connection, campaign_id: Option<&str>) -> AppResult<i64> {
    let target = match campaign_id {
        Some(id) if !id.is_empty() => Some(id.to_string()),
        _ => current_campaign_id(conn)?,
    };
    let Some(target) = target else { return Ok(1) };
    let n: Option<i64> = conn
        .query_row(
            "SELECT next_session_number FROM campaigns WHERE campaign_id = ?1",
            params![target],
            |r| r.get(0),
        )
        .optional()?;
    Ok(n.unwrap_or(1))
}

pub fn current_campaign_id(conn: &Connection) -> AppResult<Option<String>> {
    let v: Option<String> = conn
        .query_row("SELECT value FROM config WHERE key = 'current_campaign_id'", [], |r| r.get(0))
        .optional()?;
    Ok(v.filter(|s| !s.is_empty()))
}

pub fn set_current_campaign_id(conn: &Connection, campaign_id: &str) -> AppResult<()> {
    conn.execute(
        "INSERT INTO config (key, value) VALUES ('current_campaign_id', ?1) \
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![campaign_id],
    )?;
    Ok(())
}
