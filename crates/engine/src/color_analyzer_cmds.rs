// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Color Analyzer (DTF Craft): the main colours of the open document and a printable
//! sheet of variants around each one, to find the code that prints as expected.
//!
//! `dtf.colorAnalyzer` reads the document (RGB only, flattened, converted to sRGB), takes the
//! colours given in `colors` or extracts them itself (see
//! [`photocraft_algo::color_analyzer::extract`]), and renders the sheet as a new document: an
//! sRGB PNG at 300 ppi on a transparent background (the white underbase prints as in
//! production), one block of `grid × grid` variants per colour with each patch's number, code
//! and offsets underneath, the colour itself framed in the middle. Layers: « Cases » (the
//! patches), « Visuel » (the analysed visual, small, in the header) and « Textes » (black,
//! without semi-transparent edges).
//!
//! The window loads the document with [`load_document`] and extracts with [`analyze`], so what
//! it shows is what the command prints.

use photocraft_algo::color_analyzer::{self as ca, Axis, Board, Swatch};
use photocraft_cms::{Builtin, Intent, Transform};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, Size};
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::file_cmds;
use crate::{EngineError, Result, Session};

pub const COLOR_ANALYZER: &str = "dtf.colorAnalyzer";

/// Colours extracted when the window opens.
pub const DEFAULT_COLORS: usize = 6;
/// RGB distance under which two shades count as one colour.
pub const DEFAULT_MERGE: f64 = 40.0;
/// Step between two neighbouring variants (% or degrees, see [`Axis`]).
pub const DEFAULT_STEP: f64 = 5.0;
/// Variants per side of a block.
pub const DEFAULT_GRID: usize = 5;
/// Print resolution of the sheet.
pub const DEFAULT_RESOLUTION: f64 = 300.0;
/// Most colours on one sheet.
pub const MAX_COLORS: usize = 32;
/// Name of the sheet's profile, printed in its header.
pub const PROFILE_NAME: &str = "sRGB IEC61966-2.1";

/// Largest sheet side rendered, in pixels.
const MAX_SHEET_SIDE: i64 = 40_000;
/// The visual in the header fits this box (mm).
const THUMB_W: f64 = 70.0;
const THUMB_H: f64 = 24.0;
/// Text sizes (pt).
const TITLE_PT: f64 = 14.0;
const HEADER_PT: f64 = 9.0;
const NOTE_PT: f64 = 7.5;
const BLOCK_PT: f64 = 9.0;
const AXES_PT: f64 = 7.0;
const CELL_PT: f64 = 6.0;
/// Font asked for the texts (the text engine falls back to an installed one).
const FONT: &str = "Arial";

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: COLOR_ANALYZER.into(), msg: msg.into() }
}

fn num(p: &Value, k: &str, d: f64) -> f64 {
    p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(d)
}

fn check_range(p: &Value, key: &str, lo: f64, hi: f64) -> Result<()> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(()),
        Some(v) => match v.as_f64() {
            Some(x) if x.is_finite() && (lo..=hi).contains(&x) => Ok(()),
            _ => Err(bad(format!("`{key}` must be a number in {lo}..{hi} (got {v})"))),
        },
    }
}

/// 8-bit RGB with transparency.
pub fn format() -> PixelFormat {
    PixelFormat::new(ColorMode::Rgb, SampleType::U8, true)
}

/// The analysed visual: its sRGB pixels (interleaved 8-bit RGBA, straight alpha) and size.
#[derive(Clone)]
pub struct Visual {
    pub name: String,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

impl Visual {
    /// The sRGB colour at (`x`, `y`): the mean of the opaque pixels within `radius`, or `None`
    /// on a transparent spot.
    pub fn sample(&self, x: i64, y: i64, radius: i64) -> Option<[u8; 3]> {
        let (w, h) = (i64::from(self.width), i64::from(self.height));
        let (mut acc, mut n) = ([0u64; 3], 0u64);
        for yy in (y - radius).max(0)..=(y + radius).min(h - 1) {
            for xx in (x - radius).max(0)..=(x + radius).min(w - 1) {
                let i = ((yy * w + xx) * 4) as usize;
                let Some(px) = self.rgba.get(i..i + 4) else { continue };
                if px[3] >= 128 {
                    acc[0] += u64::from(px[0]);
                    acc[1] += u64::from(px[1]);
                    acc[2] += u64::from(px[2]);
                    n += 1;
                }
            }
        }
        (n > 0).then(|| acc.map(|v| ((v + n / 2) / n).min(255) as u8))
    }
}

/// The document flattened and converted to sRGB (RGB documents only).
pub fn load_document(doc: &Document) -> Result<Visual> {
    if doc.mode != ColorMode::Rgb {
        return Err(bad("the Color Analyzer works on RGB documents (Image › Mode › RGB Color)"));
    }
    crate::allocation::checkpoint("flattening the visual to analyse")?;
    let (w, h) = (doc.size.width, doc.size.height);
    if w == 0 || h == 0 {
        return Err(bad("the image is empty"));
    }
    let surf = file_cmds::flattened(doc, format());
    let mut rgba = surf.to_interleaved(Rect::new(0, 0, w as i32, h as i32));
    let profile = crate::color_cmds::document_profile(doc);
    let srgb = Builtin::Srgb.profile();
    if *profile != *srgb {
        let t = Transform::new(&profile, srgb, Intent::RelativeColorimetric, true).map_err(|e| bad(format!("conversion to sRGB: {e}")))?;
        let src = rgba.clone();
        t.convert_u8(&src, 4, &mut rgba, 4, true);
    }
    Ok(Visual { name: doc.name.clone(), width: w, height: h, rgba })
}

/// The visual's main colours (see [`ca::extract`]).
pub fn analyze(v: &Visual, max_colors: usize, merge: f64) -> Vec<Swatch> {
    ca::extract(&v.rgba, v.width as usize, v.height as usize, max_colors.clamp(1, MAX_COLORS), merge)
}

/// One colour of the sheet: the colour found in the visual, the centre of its variants (the
/// same, or the code chosen after a first print) and its share of the visual, when known.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entry {
    pub original: [u8; 3],
    pub center: [u8; 3],
    pub share: Option<f64>,
}

/// How the variants are laid out and printed.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Settings {
    pub x: Axis,
    pub y: Axis,
    pub step: f64,
    pub board: Board,
    pub resolution: f64,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { x: Axis::Lightness, y: Axis::Saturation, step: DEFAULT_STEP, board: Board::default(), resolution: DEFAULT_RESOLUTION }
    }
}

/// Today's date (UTC), "16/10/2026"; none where the platform has no clock.
fn today() -> Option<String> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        let secs = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
        // Civil date from days since 1970-01-01 (Howard Hinnant's algorithm).
        let z = (secs / 86_400) as i64 + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let d = doy - (153 * mp + 2) / 5 + 1;
        let m = if mp < 10 { mp + 3 } else { mp - 9 };
        let y = yoe + era * 400 + i64::from(m <= 2);
        Some(format!("{d:02}/{m:02}/{y}"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// "23 %" with one decimal under 10 %.
pub fn share_text(share: f64) -> String {
    let p = share * 100.0;
    if p < 10.0 { format!("{p:.1} %").replace('.', ",") } else { format!("{p:.0} %") }
}

/// "5 %" or "2,5 °".
fn step_text(step: f64, axis: Axis) -> String {
    let n = if (step - step.round()).abs() < 1e-6 { format!("{step:.0}") } else { format!("{step:.1}").replace('.', ",") };
    format!("{n} {}", axis.unit())
}

/// One line of black text to render: baseline position (px), size (pt), centred or not, bold.
struct Line {
    x: f64,
    y: f64,
    text: String,
    pt: f64,
    center: bool,
    bold: bool,
}

/// The texts rendered black, as one layer: pixels at least half covered become opaque, the
/// others transparent (a DTF film would print a soft edge as a halo).
fn text_layer(s: &Session, lines: &[Line], w: u32, h: u32, dpi: f64) -> Result<Surface> {
    let mut tmp = Document::new("textes", Size::new(w, h), ColorMode::Rgb, SampleType::U8);
    tmp.resolution_dpi = dpi as f32;
    let mut alpha = vec![0u8; w as usize * h as usize];
    for l in lines {
        if l.text.trim().is_empty() {
            continue;
        }
        let mut p = json!({"x": l.x, "y": l.y, "text": l.text, "font": FONT, "size": l.pt, "color": "#000000"});
        if l.center {
            p["align"] = json!("center");
        }
        if l.bold {
            p["fauxBold"] = json!(true);
        }
        let mut t = crate::type_cmds::new_text_layer(s, &p, COLOR_ANALYZER)?;
        crate::type_cmds::refresh(&tmp, &mut t);
        let Some(surf) = t.cache.as_ref() else { continue };
        let b = surf.content_bounds().intersect(&Rect::new(0, 0, w as i32, h as i32));
        if b.is_empty() {
            continue;
        }
        let bytes = surf.to_interleaved(b);
        let bw = b.width() as usize;
        for (k, px) in bytes.chunks_exact(4).enumerate() {
            let (x, y) = (b.x0 as usize + k % bw, b.y0 as usize + k / bw);
            if let (Some(a), Some(&pa)) = (alpha.get_mut(y * w as usize + x), px.get(3)) {
                *a = (*a).max(pa);
            }
        }
    }
    let mut rgba = vec![0u8; alpha.len() * 4];
    for (a, out) in alpha.iter().zip(rgba.chunks_exact_mut(4)) {
        if *a >= 128 {
            out[3] = 255;
        }
    }
    Ok(Surface::from_interleaved(format(), Rect::new(0, 0, w as i32, h as i32), &rgba))
}

/// The visual, fitted into `w × h` pixels at (`x`, `y`).
fn thumbnail(v: &Visual, x: i64, y: i64, w: i64, h: i64) -> Option<Surface> {
    if v.width == 0 || v.height == 0 || w <= 0 || h <= 0 {
        return None;
    }
    let k = (w as f64 / f64::from(v.width)).min(h as f64 / f64::from(v.height));
    let src = Surface::from_interleaved(format(), Rect::new(0, 0, v.width as i32, v.height as i32), &v.rgba);
    let small = photocraft_algo::resample::resize_surface(&src, k, k, photocraft_algo::resample::Resample::BicubicAutomatic);
    let content = small.content_bounds();
    if content.is_empty() {
        return None;
    }
    // Right-aligned in its box.
    let tw = (f64::from(v.width) * k).round() as i64;
    Some(small.translated((x + w - tw) as i32, y as i32, content))
}

/// The sheet for `entries`, as a document (not yet added to the session).
pub fn render(s: &Session, v: &Visual, entries: &[Entry], set: &Settings, date: Option<&str>) -> Result<Document> {
    if entries.is_empty() {
        return Err(bad("no colour to print"));
    }
    if entries.len() > MAX_COLORS {
        return Err(bad(format!("at most {MAX_COLORS} colours on one sheet")));
    }
    let b = set.board;
    if !b.fits() {
        return Err(bad("the sheet is narrower than one block of variants"));
    }
    let dpi = set.resolution;
    let px = |mm: f64| (mm * dpi / 25.4).round() as i64;
    let (w, h) = (px(b.width).max(1), px(b.height(entries.len())).max(1));
    if w > MAX_SHEET_SIDE || h > MAX_SHEET_SIDE {
        return Err(bad(format!("the sheet is too large at {dpi} ppi (at most {MAX_SHEET_SIDE} px per side)")));
    }
    crate::allocation::checkpoint("rendering the colour sheet")?;
    let (wu, hu) = (w as usize, h as usize);
    let mut patches = vec![0u8; wu * hu * 4];
    let mut fill = |x0: i64, y0: i64, x1: i64, y1: i64, rgb: [u8; 3]| {
        for y in y0.max(0)..y1.min(h) {
            for x in x0.max(0)..x1.min(w) {
                let i = (y as usize * wu + x as usize) * 4;
                if let Some(p) = patches.get_mut(i..i + 4) {
                    p.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
                }
            }
        }
    };
    let mut lines: Vec<Line> = Vec::new();
    let mut line =
        |x: f64, y: f64, text: String, pt: f64, center: bool, bold: bool| lines.push(Line { x: px(x) as f64, y: px(y) as f64, text, pt, center, bold });
    // Header.
    let m = ca::BOARD_MARGIN;
    line(m, m + 6.0, format!("ANALYSEUR COULEURS — {}", v.name), TITLE_PT, false, true);
    let date = date.map(str::to_string).or_else(today).unwrap_or_default();
    let settings = format!(
        "{date}{}Profil {PROFILE_NAME}  ·  Horizontal : {} (pas {})  ·  Vertical : {} (pas {})  ·  {} × {} cases par couleur",
        if date.is_empty() { "" } else { "  ·  " },
        set.x.name_fr(),
        step_text(set.step, set.x),
        set.y.name_fr(),
        step_text(set.step, set.y),
        b.grid,
        b.grid
    );
    line(m, m + 13.0, settings, HEADER_PT, false, false);
    line(m, m + 20.0, "Machine : ____________________     Film : ____________________     Encre : ____________________".into(), HEADER_PT, false, false);
    line(
        m,
        m + 26.5,
        "Imprimer comme un fichier de production (même RIP, même profil, même film). La case encadrée est la couleur d'origine ; reporter la case la plus juste sous chaque bloc.".into(),
        NOTE_PT,
        false,
        false,
    );
    // Blocks.
    let (_, bh) = b.block_size();
    let ring = px(0.5).max(2);
    for (i, e) in entries.iter().enumerate() {
        let origin = b.block_origin(i);
        let (ox, oy) = origin;
        let mut title = format!("COULEUR {} — {}", i + 1, ca::hex(e.center));
        if e.center != e.original {
            title.push_str(&format!("  (origine {})", ca::hex(e.original)));
        }
        if let Some(sh) = e.share {
            title.push_str(&format!("  ·  {}", share_text(sh)));
        }
        line(ox, oy + 4.5, title, BLOCK_PT, false, true);
        line(
            ox,
            oy + 9.0,
            format!("→ {} +{} par case     ↑ {} +{} par case", set.x.name_fr(), step_text(set.step, set.x), set.y.name_fr(), step_text(set.step, set.y)),
            AXES_PT,
            false,
            false,
        );
        for c in ca::grid(e.center, set.x, set.y, set.step, b.grid) {
            let (x, y, side) = b.patch_rect(origin, c.col, c.row);
            let (x0, y0, x1, y1) = (px(x), px(y), px(x + side), px(y + side));
            fill(x0, y0, x1, y1, c.rgb);
            if c.is_center() {
                // A black frame just outside the patch.
                for (a0, b0, a1, b1) in
                    [(x0 - ring, y0 - ring, x1 + ring, y0), (x0 - ring, y1, x1 + ring, y1 + ring), (x0 - ring, y0, x0, y1), (x1, y0, x1 + ring, y1)]
                {
                    fill(a0, b0, a1, b1, [0, 0, 0]);
                }
            }
            let cx = x + side / 2.0;
            line(cx, y + side + 2.9, format!("{} {}", c.number, ca::hex(c.rgb)), CELL_PT, true, c.is_center());
            let offsets = if c.is_center() { "ORIGINE".to_string() } else { ca::offset_label(set.x, set.y, set.step, c.dx, c.dy) };
            line(cx, y + side + 5.5, offsets, CELL_PT, true, c.is_center());
        }
        line(ox, oy + bh - 2.0, "Case retenue : ________     Code : #______________".into(), AXES_PT, false, false);
    }
    let mut doc = Document::new(format!("Planche couleurs {}", v.name), Size::new(w as u32, h as u32), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = dpi as f32;
    doc.icc_profile = Some(Builtin::Srgb.profile().to_bytes());
    doc.layers.clear();
    let full = Rect::new(0, 0, w as i32, h as i32);
    let mut cases = Surface::from_interleaved(format(), full, &patches);
    cases.prune();
    doc.layers.push(Layer::new("Cases", LayerContent::Raster(cases)));
    if let Some(t) = thumbnail(v, px(b.width - m - THUMB_W), px(m), px(THUMB_W), px(THUMB_H)) {
        doc.layers.push(Layer::new("Visuel", LayerContent::Raster(t)));
    }
    let mut texts = text_layer(s, &lines, w as u32, h as u32, dpi)?;
    texts.prune();
    doc.layers.push(Layer::new("Textes", LayerContent::Raster(texts)));
    Ok(doc)
}

fn axis_param(p: &Value, key: &str, default: Axis) -> Result<Axis> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(default),
        Some(v) => v
            .as_str()
            .and_then(Axis::from_id)
            .ok_or_else(|| bad(format!("`{key}` must be one of {}", Axis::ALL.iter().map(|a| a.id()).collect::<Vec<_>>().join(", ")))),
    }
}

fn color_param(v: &Value, key: &str, i: usize) -> Result<Option<[u8; 3]>> {
    match v.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(x) => x.as_str().and_then(ca::parse_hex).map(Some).ok_or_else(|| bad(format!("colour {i}: `{key}` must be a code like \"#A43B2F\""))),
    }
}

/// The settings in `p` (defaults for the missing ones).
pub fn settings(p: &Value) -> Result<Settings> {
    check_range(p, "step", 0.5, 50.0)?;
    check_range(p, "grid", 3.0, 7.0)?;
    check_range(p, "patch", 5.0, 40.0)?;
    check_range(p, "pitch", 5.0, 60.0)?;
    check_range(p, "width", 10.0, 300.0)?;
    check_range(p, "resolution", 72.0, 1200.0)?;
    let grid = num(p, "grid", DEFAULT_GRID as f64).round() as usize;
    if grid.is_multiple_of(2) {
        return Err(bad("`grid` must be 3, 5 or 7"));
    }
    let d = Board::default();
    let patch = num(p, "patch", d.patch);
    let pitch = num(p, "pitch", if p.get("patch").is_some() { patch * 1.1 } else { d.pitch });
    if pitch < patch {
        return Err(bad("`pitch` must be at least the patch size"));
    }
    let x = axis_param(p, "axisX", Axis::Lightness)?;
    let y = axis_param(p, "axisY", Axis::Saturation)?;
    if x == y {
        return Err(bad("`axisX` and `axisY` must differ"));
    }
    Ok(Settings {
        x,
        y,
        step: num(p, "step", DEFAULT_STEP),
        board: Board { width: num(p, "width", d.width / 10.0) * 10.0, patch, pitch, grid },
        resolution: num(p, "resolution", DEFAULT_RESOLUTION),
    })
}

fn color_analyzer_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(p, "maxColors", 1.0, MAX_COLORS as f64)?;
    check_range(p, "merge", 5.0, 200.0)?;
    let set = settings(p)?;
    let index = match p.get("document") {
        None | Some(Value::Null) => s.active_index().ok_or(EngineError::NoDocument)?,
        Some(v) => v.as_u64().map(|v| v as usize).filter(|i| *i < s.documents().len()).ok_or_else(|| bad(format!("no document {v}")))?,
    };
    let visual = {
        let st = s.documents().get(index).ok_or(EngineError::NoDocument)?;
        load_document(&st.doc)?
    };
    let entries: Vec<Entry> = match p.get("colors") {
        None | Some(Value::Null) => {
            let max = num(p, "maxColors", DEFAULT_COLORS as f64).round() as usize;
            analyze(&visual, max, num(p, "merge", DEFAULT_MERGE)).iter().map(|sw| Entry { original: sw.rgb, center: sw.rgb, share: Some(sw.share) }).collect()
        }
        Some(Value::Array(list)) => {
            if list.len() > MAX_COLORS {
                return Err(bad(format!("at most {MAX_COLORS} colours on one sheet")));
            }
            let mut out = Vec::new();
            for (i, c) in list.iter().enumerate() {
                let center = color_param(c, "hex", i)?.ok_or_else(|| bad(format!("colour {i}: needs a \"hex\" code")))?;
                let original = color_param(c, "original", i)?.unwrap_or(center);
                let share = c.get("share").and_then(Value::as_f64).filter(|v| v.is_finite() && (0.0..=1.0).contains(v));
                out.push(Entry { original, center, share });
            }
            out
        }
        Some(_) => return Err(bad("`colors` must be a list of {\"hex\": \"#A43B2F\"}")),
    };
    if entries.is_empty() {
        return Err(bad("no colour found: the image is empty or transparent"));
    }
    let date = p.get("date").and_then(Value::as_str);
    let doc = render(s, &visual, &entries, &set, date)?;
    let mut out = json!({
        "colors": entries.iter().map(|e| json!({"original": ca::hex(e.original), "hex": ca::hex(e.center), "share": e.share})).collect::<Vec<_>>(),
        "pixels": [doc.size.width, doc.size.height],
        "widthCm": set.board.width / 10.0,
        "heightCm": (set.board.height(entries.len()) * 10.0).round() / 100.0,
        "profile": PROFILE_NAME,
    });
    if let Some(path) = p.get("output").and_then(Value::as_str).filter(|v| !v.trim().is_empty()) {
        if file_cmds::extension(path).as_deref() != Some("png") {
            return Err(bad("the sheet is written as a PNG: give a .png `output`"));
        }
        let warnings = file_cmds::save_doc(&doc, path, file_cmds::SaveOpts::default())?;
        out["path"] = json!(path);
        out["warnings"] = json!(warnings);
    }
    if p.get("open").and_then(Value::as_bool).unwrap_or(true) {
        let index = s.add_document(doc, None);
        if let Some(d) = s.active_mut() {
            d.active_layer = d.doc.layers.last().map(|l| l.id);
        }
        out["document"] = json!(index);
    }
    Ok(out)
}

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    if s.active().is_some() { Ok(()) } else { Err("no document".into()) }
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: COLOR_ANALYZER,
        label: "Color Analyzer…",
        menu: &["DTF"],
        shortcut: None,
        params: r##"{"document":int (default the active one; RGB only),"colors":[{"hex":"#A43B2F" (centre of the variants),"original":"#rrggbb"?,"share":0..1?}] (omit to extract them),"maxColors":1..32=6,"merge":5..200=40 (RGB distance merging close shades),"axisX":"lightness|saturation|hue|red|green|blue"="lightness","axisY":…="saturation","step":0.5..50=5 (% or degrees for hue),"grid":3|5|7=5,"patch":5..40=15 (mm),"pitch":5..60=16.5 (mm),"width":10..300=56 (cm),"resolution":72..1200=300,"date":str?,"output":str (a .png path),"open":bool=true} → {colors, pixels, widthCm, heightCm, path?, document?}"##,
        enabled: has_doc,
        run: color_analyzer_cmd,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 300 ppi document: left half red, right half blue, on a transparent band at the bottom.
    fn two_colours(s: &mut Session) {
        s.execute("file.new", json!({"width": 200, "height": 120, "resolution": 300, "background": "transparent"})).unwrap();
        s.execute("select.rect", json!({"x": 0, "y": 0, "width": 100, "height": 100})).unwrap();
        s.execute("edit.fill", json!({"color": "#c03020"})).unwrap();
        s.execute("select.rect", json!({"x": 100, "y": 0, "width": 100, "height": 100})).unwrap();
        s.execute("edit.fill", json!({"color": "#2040c0"})).unwrap();
        s.execute("select.deselect", json!({})).unwrap();
    }

    #[test]
    fn colours_are_extracted_and_the_sheet_is_rendered() {
        let mut s = Session::new();
        two_colours(&mut s);
        let v = load_document(&s.active().unwrap().doc).unwrap();
        let found = analyze(&v, 6, 40.0);
        assert_eq!(found.len(), 2);
        assert!(found.iter().any(|c| c.rgb == [0xc0, 0x30, 0x20]));
        assert!(found.iter().any(|c| c.rgb == [0x20, 0x40, 0xc0]));
        assert_eq!(v.sample(10, 10, 1), Some([0xc0, 0x30, 0x20]));
        assert_eq!(v.sample(10, 115, 2), None, "transparent");
        let r = s.execute(COLOR_ANALYZER, json!({"date": "16/10/2026"})).unwrap();
        assert_eq!(r["colors"].as_array().unwrap().len(), 2);
        assert_eq!(r["widthCm"], 56.0);
        // 56 cm at 300 ppi.
        assert_eq!(r["pixels"][0], 6614);
        let st = s.active().unwrap();
        assert_eq!(s.documents().len(), 2);
        assert!((st.doc.resolution_dpi - 300.0).abs() < 0.01);
        assert!(st.doc.icc_profile.is_some(), "tagged sRGB");
        let names: Vec<&str> = st.doc.layers.iter().map(|l| l.name.as_str()).collect();
        assert_eq!(names, ["Cases", "Visuel", "Textes"]);
        // The centre patch of the first block (13th, row 2, column 2) is the colour itself.
        let b = Board::default();
        let (x, y, side) = b.patch_rect(b.block_origin(0), 2, 2);
        let px = |mm: f64| (mm * 300.0 / 25.4).round() as i32;
        let (cx, cy) = (px(x + side / 2.0), px(y + side / 2.0));
        let LayerContent::Raster(cases) = &st.doc.layers[0].content else { panic!("raster") };
        let centre = cases.to_interleaved(Rect::new(cx, cy, cx + 1, cy + 1));
        let first = r["colors"][0]["hex"].as_str().unwrap();
        assert_eq!(ca::hex([centre[0], centre[1], centre[2]]), first);
        assert_eq!(centre[3], 255);
        // Between the blocks the film stays transparent; the frame around the centre is black.
        let gap = cases.to_interleaved(Rect::new(px(ca::BOARD_MARGIN / 2.0), cy, px(ca::BOARD_MARGIN / 2.0) + 1, cy + 1));
        assert_eq!(gap[3], 0);
        let frame = cases.to_interleaved(Rect::new(cx, px(y) - 2, cx + 1, px(y) - 1));
        assert_eq!(&frame[..], &[0, 0, 0, 255]);
        // Texts: opaque black or transparent, nothing in between.
        let LayerContent::Raster(texts) = &st.doc.layers[2].content else { panic!("raster") };
        let tb = texts.content_bounds();
        if !tb.is_empty() {
            let bytes = texts.to_interleaved(tb);
            assert!(bytes.chunks_exact(4).all(|p| p[3] == 0 || p == [0, 0, 0, 255]));
        }
    }

    #[test]
    fn chosen_codes_become_the_centres_and_a_png_is_written() {
        let mut s = Session::new();
        two_colours(&mut s);
        let dir = std::env::temp_dir().join(format!("dtf-colors-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("planche.png").to_string_lossy().into_owned();
        let r = s
            .execute(
                COLOR_ANALYZER,
                json!({"colors": [{"hex": "#B04436", "original": "#c03020", "share": 0.5}], "axisX": "hue", "axisY": "lightness", "step": 3, "grid": 3, "width": 20, "output": out, "open": false}),
            )
            .unwrap();
        assert_eq!(r["colors"][0]["hex"], "#B04436");
        assert_eq!(r["colors"][0]["original"], "#C03020");
        assert!(r.get("document").is_none());
        assert_eq!(s.documents().len(), 1);
        let written = std::fs::read(&out).unwrap();
        let reopened = photocraft_io::import("planche.png", &written).unwrap().document;
        assert_eq!(reopened.size.width, 2362, "20 cm at 300 ppi");
        assert!((reopened.resolution_dpi - 300.0).abs() < 0.5);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn bad_params_and_documents_are_errors() {
        let mut s = Session::new();
        assert!(s.execute(COLOR_ANALYZER, json!({})).is_err(), "no document");
        two_colours(&mut s);
        for p in [
            json!({"document": 9}),
            json!({"colors": []}),
            json!({"colors": "red"}),
            json!({"colors": [{"hex": "#zz0000"}]}),
            json!({"colors": [{}]}),
            json!({"axisX": "chroma"}),
            json!({"axisX": "hue", "axisY": "hue"}),
            json!({"grid": 4}),
            json!({"grid": 99}),
            json!({"step": -1}),
            json!({"patch": 20, "pitch": 10}),
            json!({"width": 5}),
            json!({"width": 300, "resolution": 1200}),
            json!({"output": "planche.tif"}),
            json!({"maxColors": 0}),
        ] {
            assert!(s.execute(COLOR_ANALYZER, p.clone()).is_err(), "{p}");
        }
        // An empty (transparent) image has no colour.
        s.execute("file.new", json!({"width": 40, "height": 40, "background": "transparent"})).unwrap();
        assert!(s.execute(COLOR_ANALYZER, json!({})).is_err());
        // CMYK documents are refused.
        s.execute("file.new", json!({"width": 40, "height": 40, "mode": "cmyk"})).unwrap();
        assert!(s.execute(COLOR_ANALYZER, json!({})).is_err());
    }

    #[test]
    fn dates_and_shares_read_well() {
        let d = today().unwrap();
        assert_eq!(d.len(), 10);
        assert_eq!(share_text(0.234), "23 %");
        assert_eq!(share_text(0.056), "5,6 %");
        assert_eq!(step_text(5.0, Axis::Hue), "5 °");
        assert_eq!(step_text(2.5, Axis::Lightness), "2,5 %");
    }
}
