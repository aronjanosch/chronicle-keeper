//! Import a transcript produced somewhere else (WhisperX, faster-whisper, a
//! studio, a human typist) instead of running ASR here.
//!
//! Not gated on the `transcription` feature: parsing text needs no ASR stack, so
//! the headless build can import too.

use crate::error::{AppError, AppResult};
use crate::models::Segment;

/// Detect the format from the text itself (the filename is only a tie-breaker)
/// and parse it into segments. Speaker labels are kept when the source has them.
pub fn parse(text: &str, filename: Option<&str>) -> AppResult<Vec<Segment>> {
    let trimmed = text.trim_start_matches('\u{feff}').trim_start();
    if trimmed.is_empty() {
        return Err(AppError::BadRequest("The transcript is empty.".into()));
    }
    let ext = filename
        .and_then(|f| f.rsplit_once('.'))
        .map(|(_, e)| e.to_ascii_lowercase())
        .unwrap_or_default();

    // A leading `[` is ambiguous — it opens a JSON array but also a `[Speaker]`
    // header — so only commit to JSON if it actually parses. A leading `{` is
    // unambiguous, and there the parse error is worth surfacing.
    let as_json = match trimmed.chars().next() {
        Some('{') => Some(parse_json(trimmed)?),
        Some('[') => parse_json(trimmed).ok(),
        _ => None,
    };

    let segments = if let Some(rows) = as_json {
        rows
    } else if trimmed.starts_with("WEBVTT") || ext == "vtt" {
        parse_cues(trimmed, true)
    } else if ext == "srt" || looks_like_srt(trimmed) {
        parse_cues(trimmed, false)
    } else {
        parse_plain(trimmed)
    };

    let segments: Vec<Segment> = segments
        .into_iter()
        .filter(|s| !s.text.trim().is_empty())
        .collect();
    if segments.is_empty() {
        return Err(AppError::BadRequest(
            "No transcript lines found — expected SRT, WebVTT, JSON or plain text.".into(),
        ));
    }
    Ok(segments)
}

fn looks_like_srt(text: &str) -> bool {
    text.lines().take(40).any(|l| l.contains("-->"))
}

fn seg(text: String, start: f64, end: f64, speaker: Option<String>) -> Segment {
    Segment {
        text,
        start,
        end,
        speaker,
        source: Some("import".to_string()),
        words: None,
    }
}

/// `HH:MM:SS,mmm` / `HH:MM:SS.mmm` / `MM:SS.mmm` → seconds.
fn parse_timestamp(raw: &str) -> Option<f64> {
    let raw = raw.trim().replace(',', ".");
    let mut secs = 0.0;
    for part in raw.split(':') {
        secs = secs * 60.0 + part.trim().parse::<f64>().ok()?;
    }
    Some(secs)
}

/// SRT and WebVTT differ only in framing around the same `start --> end` cues,
/// so one parser covers both; `vtt` skips the header and metadata blocks.
fn parse_cues(text: &str, vtt: bool) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut times: Option<(f64, f64)> = None;
    let mut body: Vec<String> = Vec::new();

    let flush = |times: &mut Option<(f64, f64)>, body: &mut Vec<String>, out: &mut Vec<Segment>| {
        if let Some((start, end)) = times.take() {
            let joined = body.join(" ").trim().to_string();
            if !joined.is_empty() {
                let (speaker, line) = split_speaker(&joined);
                out.push(seg(line, start, end, speaker));
            }
        }
        body.clear();
    };

    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            flush(&mut times, &mut body, &mut out);
            continue;
        }
        if vtt && (line == "WEBVTT" || line.starts_with("NOTE") || line.starts_with("STYLE")) {
            continue;
        }
        if let Some((from, to)) = line.split_once("-->") {
            // WebVTT puts cue settings after the end time; SRT never does.
            let to = to.split_whitespace().next().unwrap_or(to);
            if let (Some(a), Some(b)) = (parse_timestamp(from), parse_timestamp(to)) {
                flush(&mut times, &mut body, &mut out);
                times = Some((a, b));
                continue;
            }
        }
        // A bare number before a cue is an SRT index (or a VTT cue id); drop it.
        if times.is_none() && line.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        body.push(line.to_string());
    }
    flush(&mut times, &mut body, &mut out);
    out
}

/// Speaker attribution inside a cue: `<v Alice>text` (WebVTT) or the
/// `Alice: text` convention. Only used for cue formats — in plain text a bare
/// `Word: ` is far more likely to be prose than a speaker label.
fn split_speaker(line: &str) -> (Option<String>, String) {
    if let Some(rest) = line.strip_prefix("<v ") {
        if let Some((name, body)) = rest.split_once('>') {
            return (
                Some(name.trim().to_string()),
                body.replace("</v>", "").trim().to_string(),
            );
        }
    }
    if let Some((name, body)) = line.split_once(": ") {
        let name = name.trim();
        let plausible = !name.is_empty()
            && name.len() <= 40
            && name.split_whitespace().count() <= 3
            && !name.contains("-->")
            && !name.ends_with('.');
        if plausible {
            return (Some(name.to_string()), body.trim().to_string());
        }
    }
    (None, line.to_string())
}

/// Whisper-family JSON: `{"segments": [...]}`, a bare segment array, or just
/// `{"text": "..."}`.
fn parse_json(text: &str) -> AppResult<Vec<Segment>> {
    let val: serde_json::Value = serde_json::from_str(text)
        .map_err(|e| AppError::BadRequest(format!("Not valid JSON: {e}")))?;
    let rows = val
        .get("segments")
        .or_else(|| val.get("transcript"))
        .or(Some(&val))
        .and_then(|v| v.as_array().cloned());
    let Some(rows) = rows else {
        // No segment list — fall back to a whole-transcript text field.
        let whole = val
            .get("text")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .trim()
            .to_string();
        if whole.is_empty() {
            return Err(AppError::BadRequest(
                "JSON has no \"segments\" array and no \"text\" field.".into(),
            ));
        }
        return Ok(parse_plain(&whole));
    };
    let num = |v: Option<&serde_json::Value>| v.and_then(|x| x.as_f64()).unwrap_or(0.0);
    Ok(rows
        .iter()
        .filter_map(|r| {
            let text = r
                .get("text")
                .and_then(|v| v.as_str())
                .or_else(|| r.as_str())?
                .trim()
                .to_string();
            let speaker = r
                .get("speaker")
                .and_then(|v| v.as_str())
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty());
            Some(seg(text, num(r.get("start")), num(r.get("end")), speaker))
        })
        .collect())
}

/// Plain text, including Chronicle Keeper's own export shape: `[Speaker]`
/// headers followed by their lines. Timings are unknown, so everything is 0 —
/// the summarizer only reads text.
fn parse_plain(text: &str) -> Vec<Segment> {
    let mut out = Vec::new();
    let mut speaker: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') && line.len() > 2 {
            speaker = Some(line[1..line.len() - 1].trim().to_string());
            continue;
        }
        out.push(seg(line.to_string(), 0.0, 0.0, speaker.clone()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_srt() {
        let src = "1\n00:00:01,000 --> 00:00:04,500\nHello there\n\n2\n00:00:05,000 --> 00:00:06,000\nGeneral Kenobi\n";
        let segs = parse(src, Some("a.srt")).unwrap();
        assert_eq!(segs.len(), 2);
        assert_eq!(segs[0].text, "Hello there");
        assert_eq!(segs[0].start, 1.0);
        assert_eq!(segs[0].end, 4.5);
        assert_eq!(segs[1].start, 5.0);
    }

    #[test]
    fn parses_vtt_with_voice_spans() {
        let src =
            "WEBVTT\n\nNOTE something\n\n00:01.000 --> 00:02.000\n<v Alice>Watch the door</v>\n";
        let segs = parse(src, Some("a.vtt")).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(segs[0].text, "Watch the door");
        assert_eq!(segs[0].start, 1.0);
    }

    #[test]
    fn parses_whisper_json() {
        let src = r#"{"segments":[{"start":1.5,"end":2.0,"text":" hi ","speaker":"SPEAKER_00"}]}"#;
        let segs = parse(src, None).unwrap();
        assert_eq!(segs.len(), 1);
        assert_eq!(segs[0].text, "hi");
        assert_eq!(segs[0].speaker.as_deref(), Some("SPEAKER_00"));
        assert_eq!(segs[0].start, 1.5);
    }

    #[test]
    fn parses_bracketed_speaker_blocks() {
        let segs = parse(
            "[Alice]\nWe head north.\nThen we camp.\n\n[Bob]\nAgreed.",
            None,
        )
        .unwrap();
        assert_eq!(segs.len(), 3);
        assert_eq!(segs[0].speaker.as_deref(), Some("Alice"));
        assert_eq!(segs[1].speaker.as_deref(), Some("Alice"));
        assert_eq!(segs[2].speaker.as_deref(), Some("Bob"));
    }

    #[test]
    fn rejects_empty() {
        assert!(parse("   ", None).is_err());
    }
}
