//! Export a page or a folder (incl. subfolders) to `<world>/Exports/` as
//! Markdown, HTML or PDF, optionally without GM-only content (see `gm`).
//!
//! PDF is written natively by `mini_pdf` — no webview print dialog, no
//! headless browser — so the file appears in `Exports/` like the others.

use std::path::{Path, PathBuf};

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag, TagEnd};
use serde::Serialize;

use crate::error::{AppError, AppResult};
use crate::mini_pdf::{self, Doc, Span, Style};
use crate::{gm, vault};

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Format {
    Pdf,
    Markdown,
    Html,
}

impl Format {
    pub fn parse(s: &str) -> AppResult<Format> {
        match s {
            "pdf" => Ok(Format::Pdf),
            "markdown" | "md" => Ok(Format::Markdown),
            "html" => Ok(Format::Html),
            _ => Err(AppError::BadRequest(format!("Unknown export format: {s}"))),
        }
    }

    fn ext(self) -> &'static str {
        match self {
            Format::Pdf => "pdf",
            Format::Markdown => "md",
            Format::Html => "html",
        }
    }
}

#[derive(Serialize, Debug)]
pub struct Exported {
    pub path: String,
    pub files: usize,
    pub skipped: usize,
}

struct Item {
    rel: String,
    title: String,
    content: String,
}

/// Frontmatter keys that never show as infobox fields in html/pdf.
const HIDDEN_KEYS: &[&str] = &[
    "aliases",
    "tags",
    "summary",
    "cssclasses",
    "publish",
    "permalink",
    "image",
    "cover",
    gm::FIELDS_KEY,
    gm::PAGE_KEY,
];

fn safe_name(s: &str) -> String {
    let n: String = s
        .chars()
        .map(|c| {
            if "/\\:*?\"<>|".contains(c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();
    let n = n.trim().trim_matches('.').to_string();
    if n.is_empty() {
        "export".into()
    } else {
        n
    }
}

fn title_of(rel: &str) -> String {
    Path::new(rel)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or(rel)
        .to_string()
}

fn unique(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("export")
        .to_string();
    let ext = path
        .extension()
        .and_then(|s| s.to_str())
        .map(|e| format!(".{e}"))
        .unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem}-{n}{ext}")))
        .find(|p| !p.exists())
        .unwrap()
}

/// `scope` is `"page"` or `"folder"`; a folder path of `""` exports the whole vault.
pub fn export(
    vault_dir: &Path,
    world_root: &Path,
    scope: &str,
    path: &str,
    format: Format,
    leave_out_gm: bool,
    world_name: &str,
) -> AppResult<Exported> {
    let rels: Vec<String> = match scope {
        "page" => vec![path.to_string()],
        "folder" if path.trim().is_empty() => vault::list_pages(vault_dir)?
            .into_iter()
            .map(|p| p.path)
            .collect(),
        "folder" => {
            let mut v = vault::page_paths_in_folder(vault_dir, path)?;
            v.sort();
            v
        }
        _ => return Err(AppError::BadRequest("scope must be page or folder".into())),
    };

    let mut items = Vec::new();
    let mut skipped = 0;
    for rel in &rels {
        let content = vault::read_page(vault_dir, rel)?.content;
        if leave_out_gm && gm::is_gm_page(&content) {
            skipped += 1;
            continue;
        }
        items.push(Item {
            rel: rel.clone(),
            title: title_of(rel),
            content: if leave_out_gm {
                gm::redact(&content)
            } else {
                content
            },
        });
    }
    if items.is_empty() {
        return Err(AppError::BadRequest(if skipped > 0 {
            "Everything selected is GM-only".into()
        } else {
            "Nothing to export".into()
        }));
    }

    let name = match scope {
        "page" => items[0].title.clone(),
        _ if path.trim().is_empty() => world_name.to_string(),
        _ => title_of(path),
    };
    let dir = world_root.join("Exports");
    std::fs::create_dir_all(&dir)
        .map_err(|e| AppError::Internal(anyhow::anyhow!("create Exports/: {e}")))?;
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let base = format!("{}-{stamp}", safe_name(&name));
    let write = |p: &Path, bytes: &[u8]| {
        std::fs::write(p, bytes)
            .map_err(|e| AppError::Internal(anyhow::anyhow!("write export: {e}")))
    };

    let out = if format == Format::Markdown && scope == "folder" {
        let out = unique(dir.join(&base));
        let prefix = if path.trim().is_empty() {
            String::new()
        } else {
            format!("{path}/")
        };
        for it in &items {
            let dest = out.join(it.rel.strip_prefix(&prefix).unwrap_or(&it.rel));
            if let Some(parent) = dest.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|e| AppError::Internal(anyhow::anyhow!("create export dir: {e}")))?;
            }
            write(&dest, it.content.as_bytes())?;
        }
        out
    } else {
        let out = unique(dir.join(format!("{base}.{}", format.ext())));
        match format {
            Format::Markdown => write(&out, items[0].content.as_bytes())?,
            Format::Html => write(
                &out,
                render_html(&name, &items, scope == "folder").as_bytes(),
            )?,
            Format::Pdf => write(
                &out,
                &render_pdf(vault_dir, &name, &items, scope == "folder"),
            )?,
        }
        out
    };
    Ok(Exported {
        path: out.to_string_lossy().into_owned(),
        files: items.len(),
        skipped,
    })
}

fn wiki_label(inner: &str) -> String {
    match inner.split_once('|') {
        Some((_, label)) => label.trim().to_string(),
        None => inner.split('#').next().unwrap_or(inner).trim().to_string(),
    }
}

/// `![[pic.png|300]]` → `![pic.png](pic.png)` for the formats the PDF writer
/// can embed; other embeds (notes, GIF, SVG, …) yield `None`.
fn image_embed(inner: &str) -> Option<String> {
    let target = inner.split('|').next()?.split('#').next()?.trim();
    let ext = target.rsplit_once('.')?.1.to_ascii_lowercase();
    if !matches!(ext.as_str(), "png" | "jpg" | "jpeg") {
        return None;
    }
    let dest = target
        .replace('%', "%25")
        .replace(' ', "%20")
        .replace('(', "%28")
        .replace(')', "%29");
    Some(format!("![{}]({dest})", target.replace(['[', ']'], "")))
}

/// Wikilinks → their label, embeds dropped (PDF: image embeds kept), callout headers → bold title.
fn flatten_markup(text: &str, images: bool) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find("[[") {
        let embed = rest[..i].ends_with('!');
        out.push_str(&rest[..i - usize::from(embed)]);
        match rest[i + 2..].find("]]") {
            Some(j) => {
                let inner = &rest[i + 2..i + 2 + j];
                if !embed {
                    out.push_str(&wiki_label(inner));
                } else if images {
                    if let Some(md) = image_embed(inner) {
                        out.push_str(&md);
                    }
                }
                rest = &rest[i + 2 + j + 2..];
            }
            None => {
                out.push_str(&rest[i..]);
                rest = "";
            }
        }
    }
    out.push_str(rest);
    out.lines()
        .map(callout_header)
        .collect::<Vec<_>>()
        .join("\n")
}

fn callout_header(line: &str) -> String {
    let t = line.trim_start();
    let quotes = t.len() - t.trim_start_matches(['>', ' ']).len();
    let (bars, rest) = t.split_at(quotes);
    if bars.is_empty() || !rest.starts_with("[!") {
        return line.to_string();
    }
    let Some(end) = rest.find(']') else {
        return line.to_string();
    };
    let kind = rest[2..end].trim_end_matches(['-', '+']);
    let title = rest[end + 1..].trim_start_matches(['-', '+']).trim();
    let label = if title.is_empty() {
        let mut c = kind.chars();
        c.next()
            .map(|f| f.to_uppercase().collect::<String>() + c.as_str())
            .unwrap_or_default()
    } else {
        title.to_string()
    };
    format!("{bars}**{label}**  ")
}

struct Prepared {
    title: String,
    summary: String,
    fields: Vec<(String, String, bool)>,
    body: String,
}

// The frontmatter parser reads `[[Page]]` as a one-item inline list, leaving `[Page]`.
fn field_value(raw: &str) -> String {
    let fixed = if raw.starts_with('[') && raw.ends_with(']') && !raw.starts_with("[[") {
        format!("[{raw}]")
    } else {
        raw.to_string()
    };
    flatten_markup(&fixed, false)
}

fn prepare(item: &Item, images: bool) -> Prepared {
    let (fm, body) = vault::split_frontmatter(&item.content);
    let gm_names = gm::gm_field_names(&item.content);
    let fields = fm
        .iter()
        .filter(|(k, v)| !HIDDEN_KEYS.contains(&k.as_str()) && !v.is_empty())
        .map(|(k, v)| {
            let val = v
                .iter()
                .map(|s| field_value(s))
                .collect::<Vec<_>>()
                .join(", ");
            (k.clone(), val, gm_names.contains(k))
        })
        .collect();
    Prepared {
        title: item.title.clone(),
        summary: vault::fm_get(&fm, "summary").unwrap_or("").to_string(),
        fields,
        body: flatten_markup(body, images),
    }
}

fn md_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

const CSS: &str = "@page{size:A4;margin:18mm}body{font:11pt/1.55 Georgia,'Times New Roman',serif;color:#1f1813;max-width:46rem;margin:2rem auto;padding:0 1rem}\
h1,h2,h3,h4{color:#7a2e1f;line-height:1.25}h1{font-size:1.9em;margin:.2em 0 .1em}section.page{margin-bottom:3rem}\
section.page+section.page{page-break-before:always}p.summary{font-style:italic;color:#6b6155;margin:.2em 0 1em}\
dl.infobox{display:grid;grid-template-columns:max-content 1fr;gap:.2em 1.2em;border:1px solid #d8d0c0;border-radius:6px;padding:.7em 1em;background:#faf6ec;font-size:.92em}\
dl.infobox dt{color:#6b6155}dl.infobox dd{margin:0}.gm{font-size:.75em;border:1px solid #d8d0c0;border-radius:3px;padding:0 .35em;color:#6b6155}\
blockquote{border-left:3px solid #d8d0c0;margin:1em 0;padding:.1em 1em;color:#4a4238}table{border-collapse:collapse}td,th{border:1px solid #d8d0c0;padding:.3em .6em}\
pre{background:#f3eee2;padding:.7em;overflow:auto}code{font-size:.92em}nav.toc{margin-bottom:2rem}";

fn render_html(name: &str, items: &[Item], multi: bool) -> String {
    let mut out = format!(
        "<!doctype html>\n<html><head><meta charset=\"utf-8\"><title>{}</title><style>{CSS}</style></head><body>\n",
        esc(name)
    );
    if multi {
        out.push_str(&format!("<h1>{}</h1><nav class=\"toc\"><ol>", esc(name)));
        for (i, it) in items.iter().enumerate() {
            out.push_str(&format!(
                "<li><a href=\"#p{i}\">{}</a></li>",
                esc(&it.title)
            ));
        }
        out.push_str("</ol></nav>\n");
    }
    for (i, it) in items.iter().enumerate() {
        let p = prepare(it, false);
        out.push_str(&format!(
            "<section class=\"page\" id=\"p{i}\"><h1>{}</h1>\n",
            esc(&p.title)
        ));
        if !p.summary.is_empty() {
            out.push_str(&format!("<p class=\"summary\">{}</p>\n", esc(&p.summary)));
        }
        if !p.fields.is_empty() {
            out.push_str("<dl class=\"infobox\">");
            for (k, v, is_gm) in &p.fields {
                let badge = if *is_gm {
                    " <span class=\"gm\">GM</span>"
                } else {
                    ""
                };
                out.push_str(&format!("<dt>{}{badge}</dt><dd>{}</dd>", esc(k), esc(v)));
            }
            out.push_str("</dl>\n");
        }
        pulldown_cmark::html::push_html(&mut out, Parser::new_ext(&p.body, md_options()));
        out.push_str("</section>\n");
    }
    out.push_str("</body></html>\n");
    out
}

fn render_pdf(vault_dir: &Path, name: &str, items: &[Item], multi: bool) -> Vec<u8> {
    let mut doc = Doc::new(name);
    if multi {
        doc.heading(1, &[Span::plain(name)]);
        for it in items {
            doc.body(
                &[Span::plain(it.title.clone())],
                0.0,
                Some("\u{2022}"),
                mini_pdf::INK,
            );
        }
    }
    for (i, it) in items.iter().enumerate() {
        if multi || i > 0 {
            doc.new_page();
        }
        let p = prepare(it, true);
        doc.heading(1, &[Span::plain(p.title.clone())]);
        if !p.summary.is_empty() {
            let it = Style {
                italic: true,
                ..Style::default()
            };
            doc.body(
                &[Span {
                    text: p.summary.clone(),
                    style: it,
                }],
                0.0,
                None,
                mini_pdf::MUTED,
            );
        }
        for (k, v, is_gm) in &p.fields {
            let key = Style {
                bold: true,
                ..Style::default()
            };
            let mut spans = vec![
                Span {
                    text: format!("{k}: "),
                    style: key,
                },
                Span::plain(v.clone()),
            ];
            if *is_gm {
                spans.push(Span::plain("  [GM]"));
            }
            doc.para(&spans, 9.5, 0.0, None, mini_pdf::MUTED, 1.0);
        }
        if !p.fields.is_empty() {
            doc.rule();
        }
        md_to_pdf(&mut doc, vault_dir, &p.body);
    }
    doc.finish()
}

/// Local vault image (`![](Assets/x.png)`) → PDF image; remote URLs and
/// formats other than JPEG/PNG return `None`.
fn load_image(vault_dir: &Path, src: &str) -> Option<mini_pdf::PdfImage> {
    if src.contains("://") || src.starts_with("data:") {
        return None;
    }
    let decoded = percent_decode(src);
    let path = vault::find_asset(vault_dir, &decoded).ok()?;
    mini_pdf::decode_image(&std::fs::read(path).ok()?)
}

fn percent_decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let hex = b
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match (b[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(v)) => {
                out.push(v);
                i += 3;
            }
            (c, _) => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn md_to_pdf(doc: &mut Doc, vault_dir: &Path, md: &str) {
    let mut spans: Vec<Span> = Vec::new();
    let mut style = Style::default();
    let (mut bold, mut italic) = (0u32, 0u32);
    let mut lists: Vec<Option<u64>> = Vec::new();
    let mut quote = 0u32;
    let mut prefix: Option<String> = None;
    let mut code: Option<String> = None;
    let mut image = 0u32;
    let mut image_src = String::new();
    let mut alt = String::new();
    let mut rows: Vec<Vec<Vec<Span>>> = Vec::new();
    let mut row: Vec<Vec<Span>> = Vec::new();

    macro_rules! restyle {
        () => {
            style = Style {
                bold: bold > 0,
                italic: italic > 0 || quote > 0,
                code: false,
            };
        };
    }
    let flush = |doc: &mut Doc,
                 spans: &mut Vec<Span>,
                 prefix: &mut Option<String>,
                 lists: &[Option<u64>],
                 quote: u32| {
        if spans.iter().all(|s| s.text.trim().is_empty()) {
            spans.clear();
            return;
        }
        let indent = lists.len().saturating_sub(1) as f32 * 16.0 + quote as f32 * 14.0;
        let color = if quote > 0 {
            mini_pdf::MUTED
        } else {
            mini_pdf::INK
        };
        doc.body(spans, indent, prefix.take().as_deref(), color);
        spans.clear();
    };

    for ev in Parser::new_ext(md, md_options()) {
        match ev {
            Event::Start(Tag::Heading { .. }) => {}
            Event::End(TagEnd::Heading(level)) => {
                let lv = match level {
                    HeadingLevel::H1 => 1,
                    HeadingLevel::H2 => 2,
                    HeadingLevel::H3 => 3,
                    _ => 4,
                };
                doc.heading(lv, &spans);
                spans.clear();
            }
            Event::End(TagEnd::Paragraph) => flush(doc, &mut spans, &mut prefix, &lists, quote),
            Event::Start(Tag::List(start)) => {
                flush(doc, &mut spans, &mut prefix, &lists, quote);
                lists.push(start);
            }
            Event::End(TagEnd::List(_)) => {
                lists.pop();
            }
            Event::Start(Tag::Item) => {
                prefix = Some(match lists.last_mut() {
                    Some(Some(n)) => {
                        let s = format!("{n}.");
                        *n += 1;
                        s
                    }
                    _ => "\u{2022}".to_string(),
                });
            }
            Event::End(TagEnd::Item) => flush(doc, &mut spans, &mut prefix, &lists, quote),
            Event::TaskListMarker(done) => {
                spans.push(Span::plain(if done { "[x] " } else { "[ ] " }));
            }
            Event::Start(Tag::BlockQuote(_)) => {
                quote += 1;
                restyle!();
            }
            Event::End(TagEnd::BlockQuote(_)) => {
                quote = quote.saturating_sub(1);
                restyle!();
            }
            Event::Start(Tag::CodeBlock(_)) => code = Some(String::new()),
            Event::End(TagEnd::CodeBlock) => {
                if let Some(c) = code.take() {
                    doc.code(&c);
                }
            }
            Event::Start(Tag::Table(_)) => {
                rows.clear();
            }
            Event::End(TagEnd::TableCell) => {
                row.push(std::mem::take(&mut spans));
            }
            Event::End(TagEnd::TableHead) | Event::End(TagEnd::TableRow) => {
                rows.push(std::mem::take(&mut row));
            }
            Event::End(TagEnd::Table) => {
                doc.table(&rows);
                rows.clear();
            }
            Event::Start(Tag::Strong) => {
                bold += 1;
                restyle!();
            }
            Event::End(TagEnd::Strong) => {
                bold = bold.saturating_sub(1);
                restyle!();
            }
            Event::Start(Tag::Emphasis) => {
                italic += 1;
                restyle!();
            }
            Event::End(TagEnd::Emphasis) => {
                italic = italic.saturating_sub(1);
                restyle!();
            }
            Event::Start(Tag::Image { dest_url, .. }) => {
                if image == 0 {
                    image_src = dest_url.to_string();
                    alt.clear();
                }
                image += 1;
            }
            Event::End(TagEnd::Image) => {
                image = image.saturating_sub(1);
                if image == 0 {
                    match load_image(vault_dir, &image_src) {
                        Some(img) => {
                            flush(doc, &mut spans, &mut prefix, &lists, quote);
                            doc.image(img);
                        }
                        None if !alt.trim().is_empty() => spans.push(Span {
                            text: format!("[image: {}]", alt.trim()),
                            style,
                        }),
                        None => {}
                    }
                }
            }
            Event::Text(t) => {
                if let Some(c) = code.as_mut() {
                    c.push_str(&t);
                } else if image == 0 {
                    spans.push(Span {
                        text: t.to_string(),
                        style,
                    });
                } else {
                    alt.push_str(&t);
                }
            }
            Event::Code(t) => spans.push(Span {
                text: t.to_string(),
                style: Style {
                    code: true,
                    ..style
                },
            }),
            Event::SoftBreak => spans.push(Span {
                text: " ".into(),
                style,
            }),
            Event::HardBreak => spans.push(Span {
                text: "\n".into(),
                style,
            }),
            Event::Rule => doc.rule(),
            _ => {}
        }
    }
    flush(doc, &mut spans, &mut prefix, &lists, quote);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn world(tag: &str) -> (PathBuf, PathBuf) {
        let root = std::env::temp_dir().join(format!("ck-pexp-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&root).ok();
        let vault = root.join("Codex");
        std::fs::create_dir_all(vault.join("NPCs/Sub")).unwrap();
        std::fs::create_dir_all(root.join(".ck")).unwrap();
        std::fs::write(
            vault.join("NPCs/Mayor.md"),
            "---\nkind: npc\nrole: Mayor\nsecret_debt: Owes 400 gp\ngm_fields: [secret_debt]\nsummary: Runs the town\n---\nHello [[Cinderhold|the town]].\n\n> [!secret] GM only\n> cultist\n\n| a | b |\n|---|---|\n| 1 | 2 |\n",
        )
        .unwrap();
        std::fs::write(
            vault.join("NPCs/Sub/Ghost.md"),
            "---\ngm_only: true\n---\nboo",
        )
        .unwrap();
        std::fs::write(vault.join("NPCs/Sub/Guard.md"), "- item one\n- item two\n").unwrap();
        (root, vault)
    }

    #[test]
    fn page_markdown_html_pdf_respect_gm_flag() {
        let (root, vault) = world("page");
        for fmt in [Format::Markdown, Format::Html, Format::Pdf] {
            let r = export(&vault, &root, "page", "NPCs/Mayor.md", fmt, true, "W").unwrap();
            let bytes = std::fs::read(&r.path).unwrap();
            let text = String::from_utf8_lossy(&bytes);
            assert!(
                !text.contains("400 gp") && !text.contains("cultist"),
                "{fmt:?} leaked"
            );
            assert!(r.path.contains("/Exports/"));
        }
        let r = export(
            &vault,
            &root,
            "page",
            "NPCs/Mayor.md",
            Format::Markdown,
            false,
            "W",
        )
        .unwrap();
        assert!(std::fs::read_to_string(r.path).unwrap().contains("400 gp"));
        let r = export(
            &vault,
            &root,
            "page",
            "NPCs/Mayor.md",
            Format::Html,
            false,
            "W",
        )
        .unwrap();
        let html = std::fs::read_to_string(r.path).unwrap();
        assert!(
            html.contains("400 gp") && html.contains("class=\"gm\"") && html.contains("the town")
        );
        assert!(html.contains("<table>"));
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn folder_export_skips_gm_pages_and_keeps_structure() {
        let (root, vault) = world("folder");
        let r = export(&vault, &root, "folder", "NPCs", Format::Markdown, true, "W").unwrap();
        assert_eq!((r.files, r.skipped), (2, 1));
        assert!(Path::new(&r.path).join("Mayor.md").is_file());
        assert!(Path::new(&r.path).join("Sub/Guard.md").is_file());
        assert!(!Path::new(&r.path).join("Sub/Ghost.md").exists());

        let r = export(&vault, &root, "folder", "NPCs", Format::Pdf, false, "W").unwrap();
        assert_eq!((r.files, r.skipped), (3, 0));
        assert!(std::fs::read(&r.path).unwrap().starts_with(b"%PDF"));

        let err = export(
            &vault,
            &root,
            "page",
            "NPCs/Sub/Ghost.md",
            Format::Pdf,
            true,
            "W",
        );
        assert!(err.is_err());
        std::fs::remove_dir_all(&root).ok();
    }

    #[test]
    fn flatten_handles_wikilinks_embeds_and_callouts() {
        let s = flatten_markup(
            "a [[X|y]] b ![[img.png]] c [[Z#h]]\n> [!note]- Title\n> body\n> [!tip]\n",
            false,
        );
        assert!(s.starts_with("a y b  c Z"));
        assert!(s.contains("> **Title**") && s.contains("> **Tip**"));
    }

    #[test]
    fn pdf_embeds_obsidian_image_embeds() {
        let (root, vault) = world("embed");
        std::fs::create_dir_all(vault.join("Assets")).unwrap();
        let png = {
            let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
            std::io::Write::write_all(&mut e, &[0, 255, 0, 0, 0, 255, 0, 0, 0, 0, 255, 0]).unwrap();
            let idat = e.finish().unwrap();
            let chunk = |t: &[u8], d: &[u8]| {
                let mut c = (d.len() as u32).to_be_bytes().to_vec();
                c.extend_from_slice(t);
                c.extend_from_slice(d);
                c.extend_from_slice(&[0; 4]);
                c
            };
            let mut b = b"\x89PNG\r\n\x1a\n".to_vec();
            b.extend(chunk(b"IHDR", &[0, 0, 0, 2, 0, 0, 0, 2, 8, 2, 0, 0, 0]));
            b.extend(chunk(b"IDAT", &idat));
            b.extend(chunk(b"IEND", &[]));
            b
        };
        std::fs::write(vault.join("Assets/My Map.png"), png).unwrap();
        std::fs::write(
            vault.join("Pic.md"),
            "Before\n\n![[My Map.png|300]]\n\n![[anim.gif]]\n\nAfter\n",
        )
        .unwrap();
        let r = export(&vault, &root, "page", "Pic.md", Format::Pdf, false, "W").unwrap();
        let pdf = String::from_utf8_lossy(&std::fs::read(&r.path).unwrap()).into_owned();
        assert_eq!(pdf.matches("/Subtype /Image").count(), 1);
        assert_eq!(
            image_embed("a b.png|200").as_deref(),
            Some("![a b.png](a%20b.png)")
        );
        assert!(image_embed("anim.gif").is_none() && image_embed("Note").is_none());
        std::fs::remove_dir_all(&root).ok();
    }
}
