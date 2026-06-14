//! World timeline (Phase 11 + 11.5): pages with `date:` frontmatter, sorted
//! on the world's calendar. Dates are numeric `year[-month[-day]]` with an
//! optional era suffix from `[calendar] eras` (`1374-08-12 DR`); month names
//! from `[calendar] months` are display-only. 11.5 additions: negative years
//! (`-500`), era-only dates (`DR`), a `~`/`c.` circa prefix (display-only),
//! an `order:`/`seq:` integer fallback for calendar-less relative beats,
//! `end_date:`/`until:` ranges, and a `gm_only:`/`publish: false` flag.

use serde_json::{json, Value};

use crate::world_config::CalendarConfig;

#[derive(Debug, PartialEq)]
pub struct WorldDate {
    pub era_idx: usize, // position in the configured eras; no suffix → 0
    pub era: Option<String>,
    pub year: Option<i64>, // None = era-only date
    pub month: u32,        // 0 = unset
    pub day: u32,          // 0 = unset
    pub circa: bool,
}

pub fn parse_world_date(raw: &str, eras: &[String]) -> Option<WorldDate> {
    let mut s = raw.trim();
    let mut circa = false;
    for p in ["~", "c.", "ca."] {
        if let Some(rest) = s.strip_prefix(p) {
            circa = true;
            s = rest.trim_start();
            break;
        }
    }
    let mut era = None;
    let mut era_idx = 0usize;
    for (i, e) in eras.iter().enumerate() {
        let lower = s.to_lowercase();
        let el = e.to_lowercase();
        if lower == el {
            return Some(WorldDate {
                era_idx: i,
                era: Some(e.clone()),
                year: None,
                month: 0,
                day: 0,
                circa,
            });
        }
        if let Some(rest) = lower.strip_suffix(&el) {
            if rest.ends_with(' ') {
                s = s[..rest.len()].trim_end();
                era = Some(e.clone());
                era_idx = i;
                break;
            }
        }
    }
    let negative = s.starts_with('-');
    if negative {
        s = &s[1..];
    }
    let mut parts = s.split('-');
    let year: i64 = parts.next()?.trim().parse().ok()?;
    let year = if negative { -year } else { year };
    let month: u32 = match parts.next() {
        Some(p) => p.trim().parse().ok()?,
        None => 0,
    };
    let day: u32 = match parts.next() {
        Some(p) => p.trim().parse().ok()?,
        None => 0,
    };
    if parts.next().is_some() {
        return None;
    }
    Some(WorldDate {
        era_idx,
        era,
        year: Some(year),
        month,
        day,
        circa,
    })
}

pub fn display(d: &WorldDate, months: &[String]) -> String {
    let mut s = match d.year {
        None => String::new(),
        Some(year) => {
            let name = (d.month >= 1 && (d.month as usize) <= months.len())
                .then(|| months[d.month as usize - 1].as_str());
            match (name, d.month, d.day) {
                (Some(n), _, 0) => format!("{} {}", n, year),
                (Some(n), _, day) => format!("{} {} {}", day, n, year),
                (None, 0, _) => year.to_string(),
                (None, m, 0) => format!("{}-{m:02}", year),
                (None, m, day) => format!("{}-{m:02}-{day:02}", year),
            }
        }
    };
    if let Some(e) = &d.era {
        if !s.is_empty() {
            s.push(' ');
        }
        s.push_str(e);
    }
    if d.circa {
        s = format!("c. {s}");
    }
    s
}

// Frontmatter values arrive as parsed YAML → JSON: take a string or the
// first element of a list, integers as numbers or quoted strings.
fn fm_str(fm: &Value, key: &str) -> Option<String> {
    match &fm[key] {
        Value::String(s) => Some(s.clone()),
        Value::Array(a) => a.first()?.as_str().map(str::to_string),
        Value::Number(n) => Some(n.to_string()),
        _ => None,
    }
}

fn fm_int(fm: &Value, keys: &[&str]) -> Option<i64> {
    keys.iter().find_map(|k| match &fm[*k] {
        Value::Number(n) => n.as_i64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    })
}

fn fm_bool(fm: &Value, key: &str) -> Option<bool> {
    match &fm[key] {
        Value::Bool(b) => Some(*b),
        Value::String(s) => match s.trim().to_lowercase().as_str() {
            "true" | "yes" => Some(true),
            "false" | "no" => Some(false),
            _ => None,
        },
        _ => None,
    }
}

/// Sort key: (era_idx, dated, year, month, day, order). Order-only beats
/// (`dated = 0`) form a relative prologue before all dated events.
type SortKey = (usize, u8, i64, u32, u32, i64);

/// Dated/ordered pages → sorted timeline entries. `rows` = (path, title, kind,
/// frontmatter_json) from the index; pages with neither a parseable `date`
/// nor an `order:`/`seq:` integer drop out.
pub fn world_events(
    rows: Vec<crate::store::index::PageFrontmatter>,
    cal: &CalendarConfig,
) -> Vec<Value> {
    let mut dated: Vec<(SortKey, Value)> = rows
        .into_iter()
        .filter_map(|(path, title, kind, fm)| {
            let fm: Value = serde_json::from_str(&fm).ok()?;
            let raw = fm_str(&fm, "date");
            let d = raw.as_deref().and_then(|r| parse_world_date(r, &cal.eras));
            let order = fm_int(&fm, &["order", "seq"]);
            let gm_only =
                fm_bool(&fm, "gm_only") == Some(true) || fm_bool(&fm, "publish") == Some(false);
            let mut entry = json!({
                "path": path,
                "title": title,
                "kind": kind,
                "summary": fm["summary"].as_str().unwrap_or(""),
                "gm_only": gm_only,
            });
            // `image:`/`cover:` → banner asset; accept bare names or [[embed]] syntax.
            if let Some(img) = fm_str(&fm, "image").or_else(|| fm_str(&fm, "cover")) {
                let img = img
                    .trim()
                    .trim_start_matches('!')
                    .trim_start_matches("[[")
                    .trim_end_matches("]]")
                    .trim();
                if !img.is_empty() {
                    entry["image"] = json!(img);
                }
            }
            let key: SortKey = match (&d, order) {
                (Some(d), o) => {
                    entry["date"] = json!(raw);
                    entry["display"] = json!(display(d, &cal.months));
                    entry["era"] = json!(d.era);
                    entry["year"] = json!(d.year);
                    entry["month"] = json!(d.month);
                    entry["day"] = json!(d.day);
                    if let Some(end_raw) = fm_str(&fm, "end_date").or_else(|| fm_str(&fm, "until"))
                    {
                        if let Some(ed) = parse_world_date(&end_raw, &cal.eras) {
                            entry["end"] = json!(end_raw);
                            entry["end_display"] = json!(display(&ed, &cal.months));
                            entry["end_year"] = json!(ed.year);
                            entry["end_month"] = json!(ed.month);
                            entry["end_day"] = json!(ed.day);
                        }
                    }
                    (
                        d.era_idx,
                        1,
                        d.year.unwrap_or(i64::MIN),
                        d.month,
                        d.day,
                        o.unwrap_or(0),
                    )
                }
                (None, Some(o)) => {
                    entry["order"] = json!(o);
                    entry["year"] = Value::Null;
                    (0, 0, o, 0, 0, 0)
                }
                (None, None) => return None,
            };
            Some((key, entry))
        })
        .collect();
    dated.sort_by(|(a, av), (b, bv)| (a, av["title"].as_str()).cmp(&(b, bv["title"].as_str())));
    dated.into_iter().map(|(_, v)| v).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cal(months: &[&str], eras: &[&str]) -> CalendarConfig {
        CalendarConfig {
            months: months.iter().map(|s| s.to_string()).collect(),
            eras: eras.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn parses_and_displays() {
        let c = cal(&["Hammer", "Alturiak"], &["DR", "NR"]);
        let d = parse_world_date("1374-02-12 DR", &c.eras).unwrap();
        assert_eq!((d.era_idx, d.year, d.month, d.day), (0, Some(1374), 2, 12));
        assert_eq!(display(&d, &c.months), "12 Alturiak 1374 DR");
        let d = parse_world_date("1374", &c.eras).unwrap();
        assert_eq!(display(&d, &c.months), "1374");
        let d = parse_world_date("212-05", &[]).unwrap();
        assert_eq!(display(&d, &[]), "212-05");
        assert!(parse_world_date("not a date", &c.eras).is_none());
        assert!(parse_world_date("1374-1-2-3", &c.eras).is_none());
    }

    #[test]
    fn parses_negative_era_only_and_circa() {
        let c = cal(&[], &["DR"]);
        let d = parse_world_date("-500", &c.eras).unwrap();
        assert_eq!(d.year, Some(-500));
        assert_eq!(display(&d, &c.months), "-500");
        let d = parse_world_date("-500-03 DR", &c.eras).unwrap();
        assert_eq!((d.year, d.month), (Some(-500), 3));
        let d = parse_world_date("DR", &c.eras).unwrap();
        assert_eq!((d.year, d.era.as_deref()), (None, Some("DR")));
        assert_eq!(display(&d, &c.months), "DR");
        let d = parse_world_date("~1374 DR", &c.eras).unwrap();
        assert!(d.circa);
        assert_eq!(display(&d, &c.months), "c. 1374 DR");
        let d = parse_world_date("c. -2000", &c.eras).unwrap();
        assert_eq!((d.circa, d.year), (true, Some(-2000)));
    }

    #[test]
    fn events_sort_on_era_then_date() {
        let c = cal(&[], &["DR", "NR"]);
        let rows = vec![
            (
                "a.md".into(),
                "Late".into(),
                Some("event".into()),
                r#"{"date":"5 NR"}"#.into(),
            ),
            (
                "b.md".into(),
                "Early".into(),
                Some("event".into()),
                r#"{"date":"1374-08 DR"}"#.into(),
            ),
            ("c.md".into(), "Undated".into(), None, r#"{}"#.into()),
            (
                "d.md".into(),
                "Mid".into(),
                Some("event".into()),
                r#"{"date":"1374-09-01 DR"}"#.into(),
            ),
        ];
        let ev = world_events(rows, &c);
        let titles: Vec<&str> = ev.iter().map(|e| e["title"].as_str().unwrap()).collect();
        assert_eq!(titles, ["Early", "Mid", "Late"]);
    }

    #[test]
    fn order_fallback_negative_years_and_ranges() {
        let c = cal(&[], &["DR"]);
        let rows = vec![
            (
                "a.md".into(),
                "Dated".into(),
                None,
                r#"{"date":"1374 DR"}"#.into(),
            ),
            (
                "b.md".into(),
                "Beat 2".into(),
                None,
                r#"{"order":2}"#.into(),
            ),
            (
                "c.md".into(),
                "Beat 1".into(),
                None,
                r#"{"seq":"1"}"#.into(),
            ),
            (
                "d.md".into(),
                "Ancient".into(),
                None,
                r#"{"date":"-500 DR"}"#.into(),
            ),
            (
                "e.md".into(),
                "War".into(),
                None,
                r#"{"date":"1300 DR","end_date":"1310 DR"}"#.into(),
            ),
            // the index stores frontmatter scalars as strings; raw bools work too
            (
                "f.md".into(),
                "Secret".into(),
                None,
                r#"{"date":"1380 DR","gm_only":"true"}"#.into(),
            ),
            (
                "g.md".into(),
                "Hidden".into(),
                None,
                r#"{"date":"1381 DR","publish":false}"#.into(),
            ),
            (
                "h.md".into(),
                "Pictured".into(),
                None,
                r#"{"date":"1390-02-05 DR","image":"![[banner.png]]"}"#.into(),
            ),
        ];
        let ev = world_events(rows, &c);
        let titles: Vec<&str> = ev.iter().map(|e| e["title"].as_str().unwrap()).collect();
        assert_eq!(
            titles,
            ["Beat 1", "Beat 2", "Ancient", "War", "Dated", "Secret", "Hidden", "Pictured"]
        );
        assert_eq!(ev[0]["order"], 1);
        assert!(ev[0]["year"].is_null());
        assert_eq!(ev[3]["end_display"], "1310 DR");
        assert_eq!(ev[3]["end_year"], 1310);
        assert_eq!(ev[7]["image"], "banner.png");
        assert_eq!(
            (ev[7]["month"].as_u64(), ev[7]["day"].as_u64()),
            (Some(2), Some(5))
        );
        assert_eq!(ev[5]["gm_only"], true);
        assert_eq!(ev[6]["gm_only"], true);
        assert_eq!(ev[4]["gm_only"], false);
    }
}
