//! Minimal PDF writer for page exports: A4, the built-in Helvetica/Courier
//! faces (no embedding, no dependency), word-wrapped text with headings,
//! lists, tables, JPEG/PNG images and page numbers. Text is WinAnsi —
//! characters outside it (CJK, emoji, …) degrade to `?`. Images: gray/RGB
//! JPEG (baseline or progressive) and non-interlaced PNG (gray, RGB, palette,
//! 8-bit with alpha); anything else is skipped.

const PAGE_W: f32 = 595.0;
const PAGE_H: f32 = 842.0;
const MARGIN: f32 = 56.0;
const BODY: f32 = 10.5;

#[derive(Clone, Copy, Default, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub code: bool,
}

#[derive(Clone)]
pub struct Span {
    pub text: String,
    pub style: Style,
}

impl Span {
    pub fn plain(text: impl Into<String>) -> Self {
        Span {
            text: text.into(),
            style: Style::default(),
        }
    }
}

pub type Color = (f32, f32, f32);
pub const INK: Color = (0.11, 0.09, 0.07);
pub const MUTED: Color = (0.42, 0.38, 0.33);
pub const BURGUNDY: Color = (0.48, 0.18, 0.12);

const HELV: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556,
    556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584, 556, 1015, 667, 667, 722, 722, 667,
    611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667,
    667, 611, 278, 278, 278, 469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500,
    222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260, 334, 584,
];

fn char_w(c: char, st: Style) -> f32 {
    if st.code {
        return 0.6;
    }
    let w = match c {
        ' '..='~' => HELV[c as usize - 32] as f32,
        '\u{2014}' => 1000.0,
        '\u{2013}' => 556.0,
        '\u{2018}' | '\u{2019}' | '\u{201C}' | '\u{201D}' => 300.0,
        '\u{2022}' => 350.0,
        _ => 600.0,
    };
    (if st.bold { w * 1.07 } else { w }) / 1000.0
}

fn text_w(s: &str, st: Style) -> f32 {
    s.chars().map(|c| char_w(c, st)).sum()
}

fn win_ansi(c: char) -> u8 {
    match c {
        '\u{20}'..='\u{7E}' | '\u{A0}'..='\u{FF}' => c as u8,
        '\u{20AC}' => 0x80,
        '\u{2026}' => 0x85,
        '\u{2018}' => 0x91,
        '\u{2019}' => 0x92,
        '\u{201C}' => 0x93,
        '\u{201D}' => 0x94,
        '\u{2022}' => 0x95,
        '\u{2013}' => 0x96,
        '\u{2014}' => 0x97,
        '\t' => b' ',
        _ => b'?',
    }
}

fn pdf_string(s: &str) -> Vec<u8> {
    let mut out = vec![b'('];
    for c in s.chars() {
        match win_ansi(c) {
            b @ (b'(' | b')' | b'\\') => out.extend([b'\\', b]),
            b => out.push(b),
        }
    }
    out.push(b')');
    out
}

fn font_of(st: Style) -> &'static str {
    if st.code {
        "F4"
    } else if st.bold {
        "F2"
    } else if st.italic {
        "F3"
    } else {
        "F1"
    }
}

struct Run {
    style: Style,
    text: String,
}

enum Tok {
    Word(String, Style),
    Space(Style),
    Break,
}

fn tokenize(spans: &[Span]) -> Vec<Tok> {
    let mut toks = Vec::new();
    for sp in spans {
        let mut word = String::new();
        for c in sp.text.chars() {
            if c == '\n' {
                if !word.is_empty() {
                    toks.push(Tok::Word(std::mem::take(&mut word), sp.style));
                }
                toks.push(Tok::Break);
            } else if c.is_whitespace() {
                if !word.is_empty() {
                    toks.push(Tok::Word(std::mem::take(&mut word), sp.style));
                }
                toks.push(Tok::Space(sp.style));
            } else {
                word.push(c);
            }
        }
        if !word.is_empty() {
            toks.push(Tok::Word(word, sp.style));
        }
    }
    toks
}

fn push_run(line: &mut Vec<Run>, style: Style, text: &str) {
    match line.last_mut() {
        Some(r) if r.style == style => r.text.push_str(text),
        _ => line.push(Run {
            style,
            text: text.to_string(),
        }),
    }
}

fn wrap(spans: &[Span], size: f32, width: f32) -> Vec<Vec<Run>> {
    let mut lines: Vec<Vec<Run>> = Vec::new();
    let mut line: Vec<Run> = Vec::new();
    let mut x = 0.0f32;
    for tok in tokenize(spans) {
        match tok {
            Tok::Break => {
                lines.push(std::mem::take(&mut line));
                x = 0.0;
            }
            Tok::Space(st) => {
                let w = char_w(' ', st) * size;
                if x > 0.0 && x + w <= width {
                    push_run(&mut line, st, " ");
                    x += w;
                }
            }
            Tok::Word(mut w, st) => {
                loop {
                    let ww = text_w(&w, st) * size;
                    if x + ww <= width {
                        push_run(&mut line, st, &w);
                        x += ww;
                        break;
                    }
                    if x > 0.0 {
                        lines.push(std::mem::take(&mut line));
                        x = 0.0;
                        continue;
                    }
                    // Longer than a whole line: split by characters.
                    let mut cut = 0;
                    let mut acc = 0.0;
                    for (i, c) in w.char_indices() {
                        let cw = char_w(c, st) * size;
                        if acc + cw > width && i > 0 {
                            break;
                        }
                        acc += cw;
                        cut = i + c.len_utf8();
                    }
                    push_run(&mut line, st, &w[..cut]);
                    lines.push(std::mem::take(&mut line));
                    w = w[cut..].to_string();
                    if w.is_empty() {
                        break;
                    }
                }
            }
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    for l in &mut lines {
        if let Some(last) = l.last_mut() {
            last.text.truncate(last.text.trim_end().len());
        }
    }
    lines
}

/// An image ready to embed: `dict` holds the colour-space/filter entries,
/// `data` the already-encoded stream, `mask` an optional grayscale alpha stream.
pub struct PdfImage {
    w: u32,
    h: u32,
    dict: String,
    data: Vec<u8>,
    mask: Option<Vec<u8>>,
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn decode_jpeg(b: &[u8]) -> Option<PdfImage> {
    let mut i = 2;
    while i + 4 <= b.len() {
        if b[i] != 0xFF {
            i += 1;
            continue;
        }
        let m = b[i + 1];
        if m == 0xFF {
            i += 1;
            continue;
        }
        if (0xD0..=0xD9).contains(&m) || m == 0x01 {
            i += 2;
            continue;
        }
        let len = u16::from_be_bytes([b[i + 2], b[i + 3]]) as usize;
        if matches!(m, 0xC0..=0xC2) {
            let seg = b.get(i + 4..i + 10)?;
            let h = u16::from_be_bytes([seg[1], seg[2]]) as u32;
            let w = u16::from_be_bytes([seg[3], seg[4]]) as u32;
            let cs = match seg[5] {
                1 => "/DeviceGray",
                3 => "/DeviceRGB",
                _ => return None,
            };
            if seg[0] != 8 || w == 0 || h == 0 {
                return None;
            }
            return Some(PdfImage {
                w,
                h,
                dict: format!("/ColorSpace {cs} /BitsPerComponent 8 /Filter /DCTDecode"),
                data: b.to_vec(),
                mask: None,
            });
        }
        i += 2 + len;
    }
    None
}

fn zlib(data: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

fn unfilter(raw: &[u8], w: usize, h: usize, bpp: usize) -> Option<Vec<u8>> {
    let stride = w * bpp;
    if raw.len() < (stride + 1) * h {
        return None;
    }
    let mut out = vec![0u8; stride * h];
    for y in 0..h {
        let ft = raw[y * (stride + 1)];
        let line = &raw[y * (stride + 1) + 1..(y + 1) * (stride + 1)];
        for x in 0..stride {
            let a = if x >= bpp {
                out[y * stride + x - bpp] as i32
            } else {
                0
            };
            let b = if y > 0 {
                out[(y - 1) * stride + x] as i32
            } else {
                0
            };
            let c = if x >= bpp && y > 0 {
                out[(y - 1) * stride + x - bpp] as i32
            } else {
                0
            };
            let pred = match ft {
                0 => 0,
                1 => a,
                2 => b,
                3 => (a + b) / 2,
                4 => {
                    let p = a + b - c;
                    let (pa, pb, pc) = ((p - a).abs(), (p - b).abs(), (p - c).abs());
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
                _ => return None,
            };
            out[y * stride + x] = line[x].wrapping_add(pred as u8);
        }
    }
    Some(out)
}

fn decode_png(b: &[u8]) -> Option<PdfImage> {
    use std::io::Read;
    let (mut i, mut ihdr, mut plte, mut idat) = (8, None, Vec::new(), Vec::new());
    while i + 12 <= b.len() {
        let len = be32(&b[i..]) as usize;
        let body = b.get(i + 8..i + 8 + len)?;
        match &b[i + 4..i + 8] {
            b"IHDR" if len >= 13 => ihdr = Some(body.to_vec()),
            b"PLTE" => plte = body.to_vec(),
            b"IDAT" => idat.extend_from_slice(body),
            b"IEND" => break,
            _ => {}
        }
        i += 12 + len;
    }
    let h = ihdr?;
    let (w, ht, depth, ct, interlace) = (be32(&h), be32(&h[4..]), h[8], h[9], h[12]);
    if interlace != 0 || w == 0 || ht == 0 || idat.is_empty() {
        return None;
    }
    let predicted = |cs: String, colors: u32| {
        format!(
            "/ColorSpace {cs} /BitsPerComponent {depth} /Filter /FlateDecode \
             /DecodeParms << /Predictor 15 /Colors {colors} /BitsPerComponent {depth} /Columns {w} >>"
        )
    };
    let plain = |dict, data| {
        Some(PdfImage {
            w,
            h: ht,
            dict,
            data,
            mask: None,
        })
    };
    match (ct, depth) {
        (0, 1 | 2 | 4 | 8 | 16) => plain(predicted("/DeviceGray".into(), 1), idat),
        (2, 8 | 16) => plain(predicted("/DeviceRGB".into(), 3), idat),
        (3, 1 | 2 | 4 | 8) if plte.len() >= 3 => {
            let hex: String = plte.iter().map(|v| format!("{v:02x}")).collect();
            let cs = format!("[/Indexed /DeviceRGB {} <{hex}>]", plte.len() / 3 - 1);
            plain(predicted(cs, 1), idat)
        }
        (4 | 6, 8) => {
            let mut raw = Vec::new();
            flate2::read::ZlibDecoder::new(&idat[..])
                .read_to_end(&mut raw)
                .ok()?;
            let bpp = if ct == 4 { 2 } else { 4 };
            let px = unfilter(&raw, w as usize, ht as usize, bpp)?;
            let (mut color, mut alpha) = (Vec::new(), Vec::new());
            for p in px.chunks(bpp) {
                color.extend_from_slice(&p[..bpp - 1]);
                alpha.push(p[bpp - 1]);
            }
            let cs = if ct == 4 { "/DeviceGray" } else { "/DeviceRGB" };
            Some(PdfImage {
                w,
                h: ht,
                dict: format!("/ColorSpace {cs} /BitsPerComponent 8 /Filter /FlateDecode"),
                data: zlib(&color),
                mask: Some(zlib(&alpha)),
            })
        }
        _ => None,
    }
}

/// JPEG or PNG bytes → embeddable image; `None` when unsupported or damaged.
pub fn decode_image(bytes: &[u8]) -> Option<PdfImage> {
    if bytes.starts_with(&[0xFF, 0xD8]) {
        decode_jpeg(bytes)
    } else if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        decode_png(bytes)
    } else {
        None
    }
}

pub struct Doc {
    title: String,
    pages: Vec<Vec<u8>>,
    page_imgs: Vec<Vec<usize>>,
    cur: Vec<u8>,
    cur_imgs: Vec<usize>,
    images: Vec<PdfImage>,
    y: f32,
}

impl Doc {
    pub fn new(title: &str) -> Self {
        Doc {
            title: title.to_string(),
            pages: Vec::new(),
            page_imgs: Vec::new(),
            cur: Vec::new(),
            cur_imgs: Vec::new(),
            images: Vec::new(),
            y: PAGE_H - MARGIN,
        }
    }

    /// Place an image at the current position, scaled down to fit the column.
    pub fn image(&mut self, img: PdfImage) {
        let (w, h) = (img.w as f32 * 0.75, img.h as f32 * 0.75);
        let k = ((PAGE_W - 2.0 * MARGIN) / w).min(420.0 / h).min(1.0);
        let (w, h) = (w * k, h * k);
        self.ensure(h + 8.0);
        self.y -= h;
        let idx = self.images.len();
        self.images.push(img);
        self.cur_imgs.push(idx);
        self.cur.extend(
            format!(
                "q {w:.2} 0 0 {h:.2} {MARGIN:.2} {:.2} cm /Im{idx} Do Q\n",
                self.y
            )
            .into_bytes(),
        );
        self.y -= 8.0;
    }

    pub fn new_page(&mut self) {
        if !self.cur.is_empty() {
            self.pages.push(std::mem::take(&mut self.cur));
            self.page_imgs.push(std::mem::take(&mut self.cur_imgs));
        }
        self.y = PAGE_H - MARGIN;
    }

    fn ensure(&mut self, h: f32) {
        if self.y - h < MARGIN && self.y < PAGE_H - MARGIN {
            self.new_page();
        }
    }

    fn set_color(&mut self, c: Color) {
        self.cur
            .extend(format!("{} {} {} rg\n", c.0, c.1, c.2).into_bytes());
    }

    fn draw_line(&mut self, runs: &[Run], x0: f32, size: f32, color: Color) {
        self.set_color(color);
        let mut x = x0;
        for r in runs {
            if r.text.is_empty() {
                continue;
            }
            self.cur.extend(
                format!(
                    "BT /{} {} Tf {:.2} {:.2} Td ",
                    font_of(r.style),
                    size,
                    x,
                    self.y
                )
                .into_bytes(),
            );
            self.cur.extend(pdf_string(&r.text));
            self.cur.extend(b" Tj ET\n");
            x += text_w(&r.text, r.style) * size;
        }
    }

    /// One wrapped block. `prefix` (a bullet or number) hangs in the indent.
    #[allow(clippy::too_many_arguments)]
    pub fn para(
        &mut self,
        spans: &[Span],
        size: f32,
        indent: f32,
        prefix: Option<&str>,
        color: Color,
        space_after: f32,
    ) {
        let lead = size * 1.4;
        let hang = if prefix.is_some() { 16.0 } else { 0.0 };
        let width = PAGE_W - 2.0 * MARGIN - indent - hang;
        let lines = wrap(spans, size, width);
        for (i, l) in lines.iter().enumerate() {
            self.ensure(lead);
            self.y -= lead;
            if i == 0 {
                if let Some(p) = prefix {
                    let run = [Run {
                        style: Style::default(),
                        text: p.to_string(),
                    }];
                    self.draw_line(&run, MARGIN + indent, size, color);
                }
            }
            self.draw_line(l, MARGIN + indent + hang, size, color);
        }
        self.y -= space_after;
    }

    pub fn body(&mut self, spans: &[Span], indent: f32, prefix: Option<&str>, color: Color) {
        self.para(spans, BODY, indent, prefix, color, 6.0);
    }

    pub fn heading(&mut self, level: u8, spans: &[Span]) {
        let (size, before) = match level {
            1 => (22.0, 4.0),
            2 => (16.0, 10.0),
            3 => (13.5, 8.0),
            _ => (11.5, 6.0),
        };
        self.ensure(size * 1.4 * 3.0);
        if self.y < PAGE_H - MARGIN {
            self.y -= before;
        }
        let bold: Vec<Span> = spans
            .iter()
            .map(|s| Span {
                text: s.text.clone(),
                style: Style {
                    bold: true,
                    ..s.style
                },
            })
            .collect();
        self.para(
            &bold,
            size,
            0.0,
            None,
            if level <= 2 { BURGUNDY } else { INK },
            4.0,
        );
    }

    pub fn rule(&mut self) {
        self.ensure(12.0);
        self.y -= 6.0;
        self.cur.extend(
            format!(
                "0.8 0.76 0.7 RG 0.6 w {:.2} {:.2} m {:.2} {:.2} l S\n",
                MARGIN,
                self.y,
                PAGE_W - MARGIN,
                self.y
            )
            .into_bytes(),
        );
        self.y -= 8.0;
    }

    pub fn code(&mut self, text: &str) {
        let code = Style {
            code: true,
            ..Style::default()
        };
        for line in text.trim_end_matches('\n').split('\n') {
            self.para(
                &[Span {
                    text: line.to_string(),
                    style: code,
                }],
                9.0,
                10.0,
                None,
                INK,
                0.0,
            );
        }
        self.y -= 6.0;
    }

    pub fn table(&mut self, rows: &[Vec<Vec<Span>>]) {
        let cols = rows.iter().map(Vec::len).max().unwrap_or(0);
        if cols == 0 {
            return;
        }
        let cw = (PAGE_W - 2.0 * MARGIN) / cols as f32;
        let size = 9.5;
        let lead = size * 1.4;
        for (ri, row) in rows.iter().enumerate() {
            let cells: Vec<Vec<Vec<Run>>> = row
                .iter()
                .map(|c| {
                    let spans: Vec<Span> = if ri == 0 {
                        c.iter()
                            .map(|s| Span {
                                text: s.text.clone(),
                                style: Style {
                                    bold: true,
                                    ..s.style
                                },
                            })
                            .collect()
                    } else {
                        c.clone()
                    };
                    wrap(&spans, size, cw - 8.0)
                })
                .collect();
            let n = cells.iter().map(Vec::len).max().unwrap_or(1);
            self.ensure(lead * n as f32 + 6.0);
            let top = self.y;
            for (ci, cell) in cells.iter().enumerate() {
                for (li, l) in cell.iter().enumerate() {
                    self.y = top - lead * (li as f32 + 1.0);
                    self.draw_line(l, MARGIN + cw * ci as f32 + 2.0, size, INK);
                }
            }
            self.y = top - lead * n as f32 - 3.0;
            self.cur.extend(
                format!(
                    "0.85 0.82 0.77 RG 0.4 w {:.2} {:.2} m {:.2} {:.2} l S\n",
                    MARGIN,
                    self.y,
                    PAGE_W - MARGIN,
                    self.y
                )
                .into_bytes(),
            );
            self.y -= 3.0;
        }
        self.y -= 6.0;
    }

    pub fn finish(mut self) -> Vec<u8> {
        self.new_page();
        if self.pages.is_empty() {
            self.pages.push(Vec::new());
            self.page_imgs.push(Vec::new());
        }
        let n = self.pages.len();
        // Objects: 1 catalog, 2 pages, 3..6 fonts, 7 info, then (page, content) pairs.
        let mut objs: Vec<Vec<u8>> = Vec::new();
        let kids: String = (0..n)
            .map(|i| format!("{} 0 R", 8 + i * 2))
            .collect::<Vec<_>>()
            .join(" ");
        objs.push(b"<< /Type /Catalog /Pages 2 0 R >>".to_vec());
        objs.push(format!("<< /Type /Pages /Kids [{kids}] /Count {n} >>").into_bytes());
        for base in [
            "Helvetica",
            "Helvetica-Bold",
            "Helvetica-Oblique",
            "Courier",
        ] {
            objs.push(
                format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>")
                    .into_bytes(),
            );
        }
        let mut info = b"<< /Title ".to_vec();
        info.extend(pdf_string(&self.title));
        info.extend(b" /Producer (Chronicle Keeper) >>");
        objs.push(info);
        let mut next = 8 + n * 2;
        let ids: Vec<(usize, Option<usize>)> = self
            .images
            .iter()
            .map(|im| {
                let id = next;
                next += if im.mask.is_some() { 2 } else { 1 };
                (id, im.mask.as_ref().map(|_| id + 1))
            })
            .collect();
        for (i, content) in self.pages.iter().enumerate() {
            let mut stream = content.clone();
            let label = (i + 1).to_string();
            let x = PAGE_W / 2.0 - text_w(&label, Style::default()) * 8.0 / 2.0;
            stream.extend(format!("{} {} {} rg\n", MUTED.0, MUTED.1, MUTED.2).into_bytes());
            stream.extend(format!("BT /F1 8 Tf {x:.2} 28 Td ({label}) Tj ET\n").into_bytes());
            let xobjs: String = self.page_imgs[i]
                .iter()
                .map(|&k| format!("/Im{k} {} 0 R ", ids[k].0))
                .collect();
            objs.push(
                format!(
                    "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W} {PAGE_H}] \
                     /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R /F4 6 0 R >> \
                     /XObject << {xobjs}>> >> /Contents {} 0 R >>",
                    9 + i * 2
                )
                .into_bytes(),
            );
            let mut s = format!("<< /Length {} >>\nstream\n", stream.len()).into_bytes();
            s.extend(stream);
            s.extend(b"\nendstream");
            objs.push(s);
        }
        for (im, (_, mask_id)) in self.images.iter().zip(&ids) {
            let sm = mask_id
                .map(|m| format!(" /SMask {m} 0 R"))
                .unwrap_or_default();
            let mut o = format!(
                "<< /Type /XObject /Subtype /Image /Width {} /Height {} {}{sm} /Length {} >>\nstream\n",
                im.w,
                im.h,
                im.dict,
                im.data.len()
            )
            .into_bytes();
            o.extend(&im.data);
            o.extend(b"\nendstream");
            objs.push(o);
            if let Some(m) = &im.mask {
                let mut o = format!(
                    "<< /Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace /DeviceGray \
                     /BitsPerComponent 8 /Filter /FlateDecode /Length {} >>\nstream\n",
                    im.w,
                    im.h,
                    m.len()
                )
                .into_bytes();
                o.extend(m);
                o.extend(b"\nendstream");
                objs.push(o);
            }
        }
        let mut out = b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n".to_vec();
        let mut offsets = Vec::new();
        for (i, o) in objs.iter().enumerate() {
            offsets.push(out.len());
            out.extend(format!("{} 0 obj\n", i + 1).into_bytes());
            out.extend(o);
            out.extend(b"\nendobj\n");
        }
        let xref = out.len();
        out.extend(format!("xref\n0 {}\n0000000000 65535 f \n", objs.len() + 1).into_bytes());
        for off in offsets {
            out.extend(format!("{off:010} 00000 n \n").into_bytes());
        }
        out.extend(
            format!(
                "trailer\n<< /Size {} /Root 1 0 R /Info 7 0 R >>\nstartxref\n{xref}\n%%EOF\n",
                objs.len() + 1
            )
            .into_bytes(),
        );
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_a_wellformed_multipage_pdf() {
        let mut d = Doc::new("T (test)");
        d.heading(1, &[Span::plain("Hello Übermensch — “quoted”")]);
        let long = "word ".repeat(2000);
        d.body(&[Span::plain(long)], 0.0, None, INK);
        d.code("let x = (1);\nsecond");
        d.table(&[
            vec![vec![Span::plain("A")], vec![Span::plain("B")]],
            vec![vec![Span::plain("1")], vec![Span::plain("2")]],
        ]);
        let pdf = d.finish();
        let s = String::from_utf8_lossy(&pdf);
        assert!(s.starts_with("%PDF-1.4"));
        assert!(s.trim_end().ends_with("%%EOF"));
        let pages = s.matches("/Type /Page ").count();
        assert!(pages >= 3, "long text should paginate, got {pages}");
        assert!(s.contains(&format!("/Count {pages}")));
        // startxref points at the xref table
        let at: usize = s
            .rsplit("startxref\n")
            .next()
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .parse()
            .unwrap();
        assert!(pdf[at..].starts_with(b"xref"));
    }

    fn png(ct: u8, w: u32, h: u32, px: &[u8]) -> Vec<u8> {
        let mut raw = Vec::new();
        for row in px.chunks(px.len() / h as usize) {
            raw.push(0);
            raw.extend(row);
        }
        let chunk = |t: &[u8], d: &[u8]| {
            let mut c = (d.len() as u32).to_be_bytes().to_vec();
            c.extend(t);
            c.extend(d);
            c.extend([0; 4]); // CRC is not checked by the reader
            c
        };
        let mut ihdr = w.to_be_bytes().to_vec();
        ihdr.extend(h.to_be_bytes());
        ihdr.extend([8, ct, 0, 0, 0]);
        let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
        out.extend(chunk(b"IHDR", &ihdr));
        out.extend(chunk(b"IDAT", &zlib(&raw)));
        out.extend(chunk(b"IEND", &[]));
        out
    }

    #[test]
    fn embeds_png_and_jpeg_images() {
        let rgb = decode_image(&png(2, 2, 2, &[255; 12])).unwrap();
        assert!(rgb.mask.is_none() && rgb.dict.contains("/Predictor 15"));
        let rgba = decode_image(&png(6, 2, 2, &[200; 16])).unwrap();
        assert!(rgba.mask.is_some());
        let jpg = [
            0xFF, 0xD8, 0xFF, 0xC0, 0, 11, 8, 0, 4, 0, 6, 3, 1, 0x22, 0, 0xFF, 0xD9,
        ];
        let j = decode_image(&jpg).unwrap();
        assert_eq!((j.w, j.h), (6, 4));
        assert!(decode_image(b"GIF89a").is_none());

        let mut d = Doc::new("img");
        d.image(rgb);
        d.image(rgba);
        d.image(j);
        let pdf = d.finish();
        let s = String::from_utf8_lossy(&pdf);
        assert_eq!(s.matches("/Subtype /Image").count(), 4);
        assert!(s.contains("/SMask") && s.contains("/DCTDecode"));
        assert!(s.contains("/Im2 "));
    }

    #[test]
    fn wrap_breaks_long_words_and_lines() {
        let l = wrap(&[Span::plain("a ".repeat(200))], 10.0, 100.0);
        assert!(l.len() > 3);
        let l = wrap(&[Span::plain("x".repeat(400))], 10.0, 100.0);
        assert!(l.len() > 3);
    }
}
