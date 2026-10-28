// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! The Color Analyzer's colour sheet: an A4 PDF with the visual (and, when colours are replaced,
//! the recoloured visual beside it), its colours (hex, RGB, share) and the swatch-book references
//! found for them, with their ΔE 2000.
//!
//! Written directly (no PDF library): standard Helvetica, Helvetica-Bold and Courier fonts in
//! WinAnsi encoding (Western European text; other characters print as "?"), the Δ as a path, the
//! visual as an RGB image with its transparency as a soft mask, the patches as vector fills.

use std::io::Write as _;

/// One of the visual's colours on the sheet.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetRow {
    pub rgb: [u8; 3],
    /// Share of the visible pixels, in %.
    pub share: f32,
    /// A colour range's row (its first picked colour), not a colour of the analysis.
    pub range: bool,
    pub reference: Option<SheetRef>,
}

/// The swatch-book colour found for a row.
#[derive(Clone, Debug, PartialEq)]
pub struct SheetRef {
    pub name: String,
    pub rgb: [u8; 3],
    pub delta_e: f32,
}

/// Everything the sheet shows.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorSheet {
    pub file_name: String,
    pub client: String,
    pub date: String,
    pub profile: String,
    pub swatch_book: Option<String>,
    /// The visual's size in pixels and its resolution (ppi).
    pub size_px: (u32, u32),
    pub dpi: f32,
    /// The visual as straight RGBA, 8 bits: width, height, pixels.
    pub image: (usize, usize, Vec<u8>),
    /// The visual with its colours replaced, the same way (`None`: nothing replaced).
    pub replaced: Option<(usize, usize, Vec<u8>)>,
    pub rows: Vec<SheetRow>,
}

const PAGE_W: f64 = 595.28;
const PAGE_H: f64 = 841.89;
const MARGIN: f64 = 42.0;
const ROW_H: f64 = 38.0;
const PATCH: f64 = 30.0;
const FOOTER_H: f64 = 52.0;

#[derive(Clone, Copy, PartialEq)]
enum Font {
    Regular,
    Bold,
    Mono,
}

impl Font {
    fn resource(self) -> &'static str {
        match self {
            Font::Regular => "/F1",
            Font::Bold => "/F2",
            Font::Mono => "/F3",
        }
    }
}

/// Helvetica advance widths (1/1000 em) for ASCII 32–126.
const HELVETICA: [u16; 95] = [
    278, 278, 355, 556, 556, 889, 667, 191, 333, 333, 389, 584, 278, 333, 278, 278, 556, 556, 556, 556, 556, 556, 556, 556, 556, 556, 278, 278, 584, 584, 584,
    556, 1015, 667, 667, 722, 722, 667, 611, 778, 722, 278, 500, 667, 556, 833, 722, 778, 667, 778, 722, 667, 611, 722, 667, 944, 667, 667, 611, 278, 278, 278,
    469, 556, 333, 556, 556, 500, 556, 556, 278, 556, 556, 222, 222, 500, 222, 833, 556, 556, 556, 556, 333, 500, 278, 556, 500, 722, 500, 500, 500, 334, 260,
    334, 584,
];

/// The WinAnsi byte of `c` (`None`: not in the encoding).
fn win_ansi(c: char) -> Option<u8> {
    let u = c as u32;
    if (0x20..0x7f).contains(&u) || (0xa0..=0xff).contains(&u) {
        return Some(u as u8);
    }
    Some(match c {
        '\u{202f}' => 0xa0,
        '€' => 0x80,
        '‚' => 0x82,
        'ƒ' => 0x83,
        '„' => 0x84,
        '…' => 0x85,
        '†' => 0x86,
        '‡' => 0x87,
        'ˆ' => 0x88,
        '‰' => 0x89,
        'Š' => 0x8a,
        '‹' => 0x8b,
        'Œ' => 0x8c,
        'Ž' => 0x8e,
        '‘' => 0x91,
        '’' => 0x92,
        '“' => 0x93,
        '”' => 0x94,
        '•' => 0x95,
        '–' => 0x96,
        '—' => 0x97,
        '˜' => 0x98,
        '™' => 0x99,
        'š' => 0x9a,
        '›' => 0x9b,
        'œ' => 0x9c,
        'ž' => 0x9e,
        'Ÿ' => 0x9f,
        _ => return None,
    })
}

/// Width of `s` in points at `size`.
fn width(font: Font, size: f64, s: &str) -> f64 {
    let em: f64 = s
        .chars()
        .map(|c| match font {
            Font::Mono => 600.0,
            _ => {
                let base = match c {
                    'Δ' => 612.0,
                    c if (' '..='~').contains(&c) => f64::from(HELVETICA[c as usize - 32]),
                    'à' | 'â' | 'ä' | 'é' | 'è' | 'ê' | 'ë' | 'ô' | 'ö' | 'ù' | 'û' | 'ü' | 'ñ' | 'ç' => 540.0,
                    'î' | 'ï' => 278.0,
                    '·' => 278.0,
                    '×' => 584.0,
                    c if c.is_uppercase() => 700.0,
                    _ => 556.0,
                };
                if font == Font::Bold { base * 1.06 } else { base }
            }
        })
        .sum();
    em * size / 1000.0
}

/// `s` cut with "…" to fit `max` points.
fn fit(font: Font, size: f64, s: &str, max: f64) -> String {
    if width(font, size, s) <= max {
        return s.to_string();
    }
    let mut out: String = s.to_string();
    while !out.is_empty() && width(font, size, &format!("{out}…")) > max {
        out.pop();
    }
    format!("{}…", out.trim_end())
}

/// `text` broken into lines of at most `max` points.
fn wrap(font: Font, size: f64, text: &str, max: f64) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split(' ') {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if !line.is_empty() && width(font, size, &candidate) > max {
            lines.push(std::mem::replace(&mut line, word.to_string()));
        } else {
            line = candidate;
        }
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Decimal numbers with the UI language's separator.
pub fn decimal(v: f64, digits: usize) -> String {
    let s = format!("{v:.digits$}");
    let comma = !matches!(crate::i18n::current().code(), "en" | "ja" | "ko" | "zh-hans" | "zh-hant");
    if comma { s.replace('.', ",") } else { s }
}

fn rgb_ops(rgb: [u8; 3]) -> String {
    format!("{:.3} {:.3} {:.3}", f64::from(rgb[0]) / 255.0, f64::from(rgb[1]) / 255.0, f64::from(rgb[2]) / 255.0)
}

/// `#RRGGBB`.
pub fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// One page's content stream, drawn with y measured from the top of the page.
#[derive(Default)]
struct Page {
    ops: String,
}

impl Page {
    /// `s` with its left end at `x` and its baseline `y` points from the top. Δ is drawn as a
    /// path: it is in no standard-14 font every viewer has (Symbol is missing from some).
    fn text(&mut self, font: Font, size: f64, x: f64, y: f64, rgb: [u8; 3], s: &str) {
        let mut cursor = x;
        for (i, part) in s.split('Δ').enumerate() {
            if i > 0 {
                self.delta(font, size, cursor, y, rgb);
                cursor += width(font, size, "Δ");
            }
            if part.is_empty() {
                continue;
            }
            let h: String = part.chars().map(|c| format!("{:02X}", win_ansi(c).unwrap_or(b'?'))).collect();
            self.ops.push_str(&format!("BT {} rg {cursor:.2} {:.2} Td {} {size:.2} Tf <{h}> Tj ET\n", rgb_ops(rgb), PAGE_H - y, font.resource()));
            cursor += width(font, size, part);
        }
    }

    /// A Δ outline in the advance of the glyph, at the cap height of Helvetica.
    fn delta(&mut self, font: Font, size: f64, x: f64, y: f64, rgb: [u8; 3]) {
        let advance = width(font, size, "Δ");
        let t = size * if font == Font::Bold { 0.11 } else { 0.075 };
        let base = PAGE_H - y + t / 2.0;
        let (left, right) = (x + 0.04 * size + t, x + advance - 0.04 * size - t);
        let top = PAGE_H - y + 0.718 * size - t;
        let mid = (left + right) / 2.0;
        self.ops.push_str(&format!("q {} RG {t:.2} w 0 j {left:.2} {base:.2} m {right:.2} {base:.2} l {mid:.2} {top:.2} l h S Q\n", rgb_ops(rgb)));
    }

    fn text_right(&mut self, font: Font, size: f64, right: f64, y: f64, rgb: [u8; 3], s: &str) {
        self.text(font, size, right - width(font, size, s), y, rgb, s);
    }

    /// A filled rectangle, `y` its top, with a grey border.
    fn patch(&mut self, x: f64, y: f64, w: f64, h: f64, rgb: [u8; 3]) {
        self.ops.push_str(&format!("q {} rg 0.741 0.741 0.741 RG 0.75 w {x:.2} {:.2} {w:.2} {h:.2} re B Q\n", rgb_ops(rgb), PAGE_H - y - h));
    }

    fn line(&mut self, x0: f64, x1: f64, y: f64, w: f64, grey: f64) {
        self.ops.push_str(&format!("q {grey:.3} G {w:.2} w {x0:.2} {:.2} m {x1:.2} {:.2} l S Q\n", PAGE_H - y, PAGE_H - y));
    }
}

const INK: [u8; 3] = [0x1c, 0x1c, 0x1c];
const DIM: [u8; 3] = [0x55, 0x55, 0x55];

fn de_colour(de: f32) -> [u8; 3] {
    if de <= 2.0 {
        [0x1b, 0x6b, 0x34]
    } else if de <= 5.0 {
        [0x8a, 0x53, 0x00]
    } else {
        [0xa1, 0x22, 0x22]
    }
}

/// The table's columns: (left, width) for patch, hex·RGB, share, swatch patch, reference, hex, ΔE.
fn columns() -> [(f64, f64); 7] {
    let gap = 9.0;
    let fixed = [39.0, 97.5, 48.0, 39.0, 0.0, 61.5, 36.0];
    let flex = PAGE_W - 2.0 * MARGIN - fixed.iter().sum::<f64>() - 6.0 * gap;
    let mut out = [(0.0, 0.0); 7];
    let mut x = MARGIN;
    for (i, w) in fixed.iter().enumerate() {
        let w = if i == 4 { flex } else { *w };
        out[i] = (x, w);
        x += w + gap;
    }
    out
}

fn table_header(page: &mut Page, y: f64) -> f64 {
    let c = columns();
    let size = 8.25;
    let base = y + 12.0;
    let de_label = "ΔE";
    for (i, label) in [tl!("Visual"), tl!("Hex · RGB"), tl!("Area"), tl!("Swatch Book"), tl!("Reference"), tl!("Hex"), de_label].iter().enumerate() {
        let s = fit(Font::Bold, size, label, c[i].1 + 8.0);
        if i == 2 || i == 6 {
            page.text_right(Font::Bold, size, c[i].0 + c[i].1, base, [0x33; 3], &s);
        } else {
            page.text(Font::Bold, size, c[i].0, base, [0x33; 3], &s);
        }
    }
    page.line(MARGIN, PAGE_W - MARGIN, y + 18.0, 0.75, 0.11);
    y + 18.0
}

fn table_row(page: &mut Page, y: f64, r: &SheetRow) {
    let c = columns();
    let top = y + (ROW_H - PATCH) / 2.0;
    let mid = y + ROW_H / 2.0;
    page.patch(c[0].0, top, PATCH, PATCH, r.rgb);
    page.text(Font::Mono, 9.0, c[1].0, mid - 2.0, INK, &hex(r.rgb));
    let second = if r.range { tl!("Color Range").to_string() } else { format!("{} {} · {} · {}", tl!("RGB"), r.rgb[0], r.rgb[1], r.rgb[2]) };
    let font = if r.range { Font::Regular } else { Font::Mono };
    page.text(font, 8.0, c[1].0, mid + 9.0, DIM, &fit(font, 8.0, &second, c[1].1 + 6.0));
    page.text_right(Font::Mono, 9.0, c[2].0 + c[2].1, mid + 3.0, INK, &format!("{} %", decimal(f64::from(r.share), 1)));
    match &r.reference {
        Some(s) => {
            page.patch(c[3].0, top, PATCH, PATCH, s.rgb);
            page.text(Font::Bold, 9.5, c[4].0, mid + 3.0, INK, &fit(Font::Bold, 9.5, &s.name, c[4].1));
            page.text(Font::Mono, 9.0, c[5].0, mid + 3.0, INK, &hex(s.rgb));
            page.text_right(Font::Mono, 9.0, c[6].0 + c[6].1, mid + 3.0, de_colour(s.delta_e), &decimal(f64::from(s.delta_e), 1));
        }
        None => page.text(Font::Regular, 9.0, c[4].0, mid + 3.0, DIM, "—"),
    }
    page.line(MARGIN, PAGE_W - MARGIN, y + ROW_H, 0.75, 0.784);
}

fn footer(page: &mut Page) {
    let mut y = PAGE_H - MARGIN - FOOTER_H + 6.0;
    page.line(MARGIN, PAGE_W - MARGIN, y, 0.75, 0.784);
    y += 14.0;
    for (text, colour) in [
        (tl!("ΔE 2000: up to 2 = imperceptible difference · 2 to 5 = visible up close · above 5 = another hue."), [0x44; 3]),
        (tl!("Colors shown in sRGB: the printed result depends on the printer, the inks and the substrate."), [0x44; 3]),
        (tl!("Sheet made with DTF Craft"), DIM),
    ] {
        page.text(Font::Regular, 8.5, MARGIN, y, colour, &fit(Font::Regular, 8.5, text, PAGE_W - 2.0 * MARGIN));
        y += 12.0;
    }
}

/// The summary under the title: number of colours, how close the references are, the size.
fn summary(sheet: &ColorSheet) -> (String, String) {
    let n = sheet.rows.len().to_string();
    let title = crate::i18n::fmt(tl!("Colors kept: {n}"), &[("n", &n)]);
    let des: Vec<f32> = sheet.rows.iter().filter_map(|r| r.reference.as_ref().map(|s| s.delta_e)).collect();
    let mut text = if sheet.swatch_book.is_none() || des.is_empty() {
        tl!("No swatch book loaded: the colors have no reference.").to_string()
    } else {
        let count = |f: &dyn Fn(f32) -> bool| des.iter().filter(|d| f(**d)).count().to_string();
        crate::i18n::fmt(
            tl!("Matches at ΔE 2 or less: {good}; to check (ΔE 2 to 5): {check}; above 5: {far}."),
            &[("good", &count(&|d| d <= 2.0)), ("check", &count(&|d| d > 2.0 && d <= 5.0)), ("far", &count(&|d| d > 5.0))],
        )
    };
    let (w, h) = sheet.size_px;
    let dpi = f64::from(sheet.dpi.max(1.0));
    let cm = |px: u32| decimal(f64::from(px) / dpi * 2.54, 1);
    text.push(' ');
    text.push_str(&crate::i18n::fmt(
        tl!("Visual size: {w} × {h} px, {wcm} × {hcm} cm at {dpi} ppi."),
        &[("w", &w.to_string()), ("h", &h.to_string()), ("wcm", &cm(w)), ("hcm", &cm(h)), ("dpi", &format!("{}", dpi.round()))],
    ));
    (title, text)
}

fn deflate(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

fn stream(dict: &str, data: &[u8]) -> Vec<u8> {
    let z = deflate(data);
    let mut body = format!("<< {dict} /Filter /FlateDecode /Length {} >>\nstream\n", z.len()).into_bytes();
    body.extend_from_slice(&z);
    body.extend_from_slice(b"\nendstream");
    body
}

/// The sheet as a PDF file.
pub fn pdf(sheet: &ColorSheet) -> Vec<u8> {
    let mut pages: Vec<Page> = vec![Page::default()];
    let p = &mut pages[0];
    // Header.
    p.text(Font::Bold, 22.5, MARGIN, MARGIN + 20.0, INK, &fit(Font::Bold, 22.5, tl!("Color Sheet"), 280.0));
    p.text(Font::Regular, 10.5, MARGIN, MARGIN + 38.0, INK, &fit(Font::Regular, 10.5, &sheet.file_name, 280.0));
    let book = sheet.swatch_book.clone().unwrap_or_else(|| "—".into());
    let client = if sheet.client.trim().is_empty() { "—".to_string() } else { sheet.client.clone() };
    let (lx, vx) = (PAGE_W - MARGIN - 200.0, PAGE_W - MARGIN - 150.0);
    for (i, (label, value)) in
        [(tl!("Client"), client), (tl!("Date"), sheet.date.clone()), (tl!("Profile"), sheet.profile.clone()), (tl!("Swatch Book"), book)].iter().enumerate()
    {
        let y = MARGIN + 10.0 + 13.0 * i as f64;
        p.text(Font::Regular, 9.0, lx, y, DIM, &fit(Font::Regular, 9.0, label, 48.0));
        p.text(Font::Regular, 9.0, vx, y, INK, &fit(Font::Regular, 9.0, value, 150.0));
    }
    let rule = MARGIN + 62.0;
    p.line(MARGIN, PAGE_W - MARGIN, rule, 1.5, 0.11);
    let top = rule + 21.0;
    // `Im{k}` fitted in a `w` × `h` box at (`x`, `y`).
    let place = |p: &mut Page, k: usize, image: &(usize, usize, Vec<u8>), x: f64, y: f64, bw: f64, bh: f64| {
        let (iw, ih, _) = image;
        if *iw > 0 && *ih > 0 {
            let s = (bw / *iw as f64).min(bh / *ih as f64);
            let (w, h) = (*iw as f64 * s, *ih as f64 * s);
            let (x, y) = (x + (bw - w) / 2.0, y + (bh - h) / 2.0);
            p.ops.push_str(&format!("q {w:.3} 0 0 {h:.3} {x:.3} {:.3} cm /Im{k} Do Q\n", PAGE_H - y - h));
        }
    };
    let (title, text) = summary(sheet);
    let table_top = match &sheet.replaced {
        // The original and the recoloured visual side by side, the summary below them.
        Some(replaced) => {
            let gap = 24.0;
            let bw = (PAGE_W - 2.0 * MARGIN - gap) / 2.0;
            let bh = 190.0;
            for (k, (label, image)) in [(tl!("Original Colors"), &sheet.image), (tl!("Replaced Colors"), replaced)].into_iter().enumerate() {
                let x = MARGIN + k as f64 * (bw + gap);
                p.text(Font::Bold, 10.0, x, top + 2.0, INK, &fit(Font::Bold, 10.0, label, bw));
                place(p, k, image, x, top + 12.0, bw, bh);
            }
            let sw = PAGE_W - 2.0 * MARGIN;
            let mut y = top + 12.0 + bh + 26.0;
            p.text(Font::Bold, 12.0, MARGIN, y, INK, &fit(Font::Bold, 12.0, &title, sw));
            y += 18.0;
            for line in wrap(Font::Regular, 9.5, &text, sw) {
                p.text(Font::Regular, 9.5, MARGIN, y, INK, &line);
                y += 13.5;
            }
            y += 2.0;
            let mut x = MARGIN;
            for r in &sheet.rows {
                if x + 30.0 > PAGE_W - MARGIN {
                    break;
                }
                p.patch(x, y, 30.0, 30.0, r.reference.as_ref().map_or(r.rgb, |s| s.rgb));
                x += 34.5;
            }
            y + 30.0 + 21.0
        }
        // The visual, fitted in a square, and the summary beside it.
        None => {
            let side = 195.0;
            place(p, 0, &sheet.image, MARGIN, top, side, side);
            let sx = MARGIN + side + 24.0;
            let sw = PAGE_W - MARGIN - sx;
            let mut y = top + 50.0;
            p.text(Font::Bold, 12.0, sx, y, INK, &fit(Font::Bold, 12.0, &title, sw));
            y += 18.0;
            for line in wrap(Font::Regular, 9.5, &text, sw) {
                p.text(Font::Regular, 9.5, sx, y, INK, &line);
                y += 13.5;
            }
            y += 2.0;
            let mut x = sx;
            for r in &sheet.rows {
                if x + 30.0 > PAGE_W - MARGIN {
                    break;
                }
                p.patch(x, y, 30.0, 30.0, r.reference.as_ref().map_or(r.rgb, |s| s.rgb));
                x += 34.5;
            }
            top + side + 21.0
        }
    };
    // The table, continued on further pages as needed; the footer on every page.
    let bottom = PAGE_H - MARGIN - FOOTER_H;
    let mut y = table_header(&mut pages[0], table_top);
    for r in &sheet.rows {
        if y + ROW_H > bottom {
            pages.push(Page::default());
            let next = pages.len() - 1;
            y = table_header(&mut pages[next], MARGIN);
        }
        let last = pages.len() - 1;
        table_row(&mut pages[last], y, r);
        y += ROW_H;
    }
    for page in &mut pages {
        footer(page);
    }
    let mut images = vec![&sheet.image];
    images.extend(sheet.replaced.as_ref());
    write_pdf(&pages, &images)
}

fn write_pdf(pages: &[Page], images: &[&(usize, usize, Vec<u8>)]) -> Vec<u8> {
    let mut out: Vec<u8> = b"%PDF-1.4\n%\xe2\xe3\xcf\xd3\n".to_vec();
    let mut offsets: Vec<usize> = Vec::new();
    let mut obj = |out: &mut Vec<u8>, body: &[u8]| {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n", offsets.len()).as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b"\nendobj\n");
    };
    // 1 catalog, 2 pages, 3–5 fonts, then each image and its mask (6, 7; 8, 9), then a page and
    // its contents per page.
    let first_page = 6 + 2 * images.len();
    let kids: Vec<String> = (0..pages.len()).map(|i| format!("{} 0 R", first_page + 2 * i)).collect();
    obj(&mut out, b"<< /Type /Catalog /Pages 2 0 R >>");
    obj(&mut out, format!("<< /Type /Pages /Kids [{}] /Count {} >>", kids.join(" "), pages.len()).as_bytes());
    for base in ["Helvetica", "Helvetica-Bold", "Courier"] {
        obj(&mut out, format!("<< /Type /Font /Subtype /Type1 /BaseFont /{base} /Encoding /WinAnsiEncoding >>").as_bytes());
    }
    for (k, (w, h, px)) in images.iter().map(|i| (i.0, i.1, &i.2)).enumerate() {
        let (w, h) = (w.max(1), h.max(1));
        let mut rgb = Vec::with_capacity(w * h * 3);
        let mut alpha = Vec::with_capacity(w * h);
        for q in px.chunks_exact(4).take(w * h) {
            rgb.extend_from_slice(&q[..3]);
            alpha.push(q[3]);
        }
        rgb.resize(w * h * 3, 255);
        alpha.resize(w * h, 0);
        let mask = 7 + 2 * k;
        obj(&mut out, &stream(&format!("/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceRGB /BitsPerComponent 8 /SMask {mask} 0 R"), &rgb));
        obj(&mut out, &stream(&format!("/Type /XObject /Subtype /Image /Width {w} /Height {h} /ColorSpace /DeviceGray /BitsPerComponent 8"), &alpha));
    }
    let xobjects: String = (0..images.len()).map(|k| format!("/Im{k} {} 0 R ", 6 + 2 * k)).collect();
    for (i, page) in pages.iter().enumerate() {
        let contents = first_page + 2 * i + 1;
        obj(
            &mut out,
            format!(
                "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_W:.2} {PAGE_H:.2}] /Resources << /Font << /F1 3 0 R /F2 4 0 R /F3 5 0 R >> /XObject << {xobjects}>> >> /Contents {contents} 0 R >>"
            )
            .as_bytes(),
        );
        // A white page under everything.
        let content = format!("q 1 g 0 0 {PAGE_W:.2} {PAGE_H:.2} re f Q\n{}", page.ops);
        obj(&mut out, &stream("", content.as_bytes()));
    }
    let xref = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", offsets.len() + 1).as_bytes());
    for o in &offsets {
        out.extend_from_slice(format!("{o:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", offsets.len() + 1).as_bytes());
    out
}

/// Today's date (UTC) as dd/mm/yyyy.
pub fn today() -> String {
    let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let days = (secs / 86_400) as i64;
    // Civil date from days since 1970-01-01 (H. Hinnant's algorithm).
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = yoe + era * 400 + i64::from(m <= 2);
    format!("{d:02}/{m:02}/{y}")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sheet(rows: usize) -> ColorSheet {
        ColorSheet {
            file_name: "logo-client.png".into(),
            client: "Atelier".into(),
            date: "21/10/2026".into(),
            profile: "sRGB IEC61966-2.1".into(),
            swatch_book: Some("Nuancier.ase".into()),
            size_px: (2400, 2400),
            dpi: 300.0,
            image: (2, 1, vec![255, 0, 0, 255, 0, 0, 255, 0]),
            replaced: None,
            rows: (0..rows)
                .map(|i| SheetRow {
                    rgb: [31, 58, 147],
                    share: 38.2,
                    range: i == 1,
                    reference: (i % 2 == 0).then(|| SheetRef { name: format!("EX-{i} Bleu roi"), rgb: [30, 59, 150], delta_e: 1.2 }),
                })
                .collect(),
        }
    }

    #[test]
    fn the_sheet_is_a_pdf_with_its_texts_and_pages() {
        let bytes = pdf(&sheet(5));
        assert!(bytes.starts_with(b"%PDF-1.4") && bytes.ends_with(b"%%EOF\n"));
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("/Count 1") && text.contains("/SMask 7 0 R") && !text.contains("/Symbol"));
        // With the recoloured visual: a second image, the pages after it.
        let two = pdf(&ColorSheet { replaced: Some((2, 1, vec![0, 0, 255, 255, 0, 0, 255, 0])), ..sheet(5) });
        let t2 = String::from_utf8_lossy(&two);
        assert!(t2.contains("/SMask 9 0 R") && t2.contains("/Im0 6 0 R /Im1 8 0 R") && t2.contains("/Kids [10 0 R]"), "{}", &t2[..t2.len().min(2000)]);
        // Many colours continue on a second page.
        assert!(String::from_utf8_lossy(&pdf(&sheet(20))).contains("/Count 2"));
        // The xref offsets point at their objects.
        let at = bytes.windows(6).rposition(|w| w == b"\nxref\n").unwrap() + 1;
        let table = String::from_utf8_lossy(&bytes[at..]).into_owned();
        for (n, line) in table.lines().skip(3).take(9).enumerate() {
            let offset = line[..10].parse::<usize>().unwrap();
            assert!(bytes[offset..].starts_with(format!("{} 0 obj", n + 1).as_bytes()), "object {}", n + 1);
        }
    }

    #[test]
    fn texts_are_win_ansi_and_measured() {
        let mut p = Page::default();
        p.text(Font::Regular, 10.0, 0.0, 0.0, INK, "Écart ΔE");
        // The text around the Δ, and the Δ as a stroked triangle in its advance.
        assert!(p.ops.contains("/F1 10.00 Tf <C96361727420> Tj ET") && p.ops.contains("/F1 10.00 Tf <45> Tj ET"), "{}", p.ops);
        assert_eq!(p.ops.matches(" l h S Q").count(), 1, "{}", p.ops);
        let e = p.ops.lines().last().unwrap();
        let after = width(Font::Regular, 10.0, "Écart Δ");
        assert!(e.contains(&format!("{after:.2} ")), "{e}");
        assert!((width(Font::Regular, 10.0, "AB") - 13.34).abs() < 1e-6);
        assert_eq!(fit(Font::Regular, 10.0, "Référence très longue", 40.0).chars().last(), Some('…'));
        assert_eq!(wrap(Font::Regular, 10.0, "un deux trois quatre", 45.0).len(), 3);
        assert_eq!(today().len(), 10);
    }
}
