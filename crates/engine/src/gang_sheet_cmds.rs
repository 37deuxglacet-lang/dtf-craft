// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Gang Sheet (DTF Craft): visuals laid out on a film of fixed width, rendered as one
//! document at the print resolution and, optionally, written as a PNG.
//!
//! `dtf.gangSheet` takes the visuals (`sources`: PNG files by path or open documents, each
//! flattened and cropped to its opaque pixels) and either the layout drawn in the Gang Sheet
//! window (`placements`, in centimetres) or nothing, in which case it builds one itself
//! (`quantity` copies of each source, MaxRects, see [`photocraft_algo::gang_sheet`]). The sheet
//! is as long as its content needs, up to `maxLength`; every visual is a layer of its own.
//!
//! The window loads its visuals with [`load_file`] and [`load_document`], so what it shows and
//! measures is exactly what the command renders.

use photocraft_algo::gang_sheet::{self, Item, Sheet, UNITS_PER_CM};
use photocraft_color::{ColorMode, PixelFormat, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, Size};
use photocraft_geom::Rect;
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::file_cmds;
use crate::{EngineError, Result, Session};

pub const GANG_SHEET: &str = "dtf.gangSheet";

/// Film width (cm) the window starts with.
pub const DEFAULT_FILM_WIDTH_CM: f64 = 56.0;
/// Longest sheet (cm).
pub const DEFAULT_MAX_LENGTH_CM: f64 = 100.0;
/// Margin around every visual (mm): to the other visuals and to the film edges.
pub const DEFAULT_MARGIN_MM: f64 = 5.0;
/// Print resolution of the sheet.
pub const DEFAULT_RESOLUTION: f64 = 300.0;

/// Largest sheet side rendered, in pixels (about 5 m at 300 ppi).
const MAX_SHEET_SIDE: i64 = 60_000;
/// Most visuals (sources) and most placements one sheet takes.
const MAX_SOURCES: usize = 200;
const MAX_PLACEMENTS: usize = 2_000;
/// Resolution assumed for a file that records none or an absurd one.
const FALLBACK_DPI: f64 = 300.0;

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: GANG_SHEET.into(), msg: msg.into() }
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

/// The pixel format visuals are flattened to: 8-bit RGB with transparency.
pub fn format() -> PixelFormat {
    PixelFormat::new(ColorMode::Rgb, SampleType::U8, true)
}

/// One visual: its name, its pixels cropped to the visible ones (at the origin), their size and
/// the resolution the file records.
#[derive(Clone)]
pub struct Visual {
    pub name: String,
    pub pixels: Surface,
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
}

impl Visual {
    /// Its print width (layout units) at its own resolution, no wider than the sheet allows.
    pub fn natural_width(&self, sheet: &Sheet) -> i64 {
        let dpi = if self.dpi.is_finite() && self.dpi >= 1.0 { self.dpi } else { FALLBACK_DPI };
        let w = gang_sheet::from_px(f64::from(self.width), dpi).max(1);
        let (iw, il) = sheet.inner();
        // Too large for the sheet: the largest size that fits, proportions kept.
        let fit_h = if self.height > 0 { (il.max(1) as f64 * f64::from(self.width) / f64::from(self.height)).floor() as i64 } else { w };
        w.min(iw.max(1)).min(fit_h.max(1))
    }
}

/// The visible pixels of `surf` (alpha above zero) within `full`.
fn alpha_bounds(surf: &Surface, full: Rect) -> Rect {
    let w = full.width().max(0) as usize;
    if w == 0 || full.height() <= 0 {
        return Rect::EMPTY;
    }
    let mut out = Rect::EMPTY;
    // Bands of rows keep the scan's memory small on large visuals.
    let band = 256;
    let mut y = full.y0;
    while y < full.y1 {
        let y1 = (y + band).min(full.y1);
        let r = Rect::new(full.x0, y, full.x1, y1);
        let bytes = surf.to_interleaved(r);
        for (row, line) in bytes.chunks_exact(w * 4).enumerate() {
            let xs: Vec<usize> = line.chunks_exact(4).enumerate().filter(|(_, px)| px.get(3).is_some_and(|a| *a > 0)).map(|(x, _)| x).collect();
            if let (Some(a), Some(b)) = (xs.first(), xs.last()) {
                let yy = y + row as i32;
                out = out.union(&Rect::new(full.x0 + *a as i32, yy, full.x0 + *b as i32 + 1, yy + 1));
            }
        }
        y = y1;
    }
    out
}

/// A document's visual: flattened, cropped to its visible pixels.
pub fn load_document(doc: &Document, name: &str) -> Result<Visual> {
    crate::allocation::checkpoint("flattening a gang sheet visual")?;
    let surf = file_cmds::flattened(doc, format());
    let full = Rect::new(0, 0, doc.size.width as i32, doc.size.height as i32);
    let r = alpha_bounds(&surf, full);
    if r.is_empty() {
        return Err(bad(format!("{name}: the image is empty")));
    }
    Ok(Visual {
        name: name.to_string(),
        pixels: surf.translated(-r.x0, -r.y0, r),
        width: r.width() as u32,
        height: r.height() as u32,
        dpi: f64::from(doc.resolution_dpi),
    })
}

/// A PNG file's visual (see [`load_document`]).
pub fn load_bytes(file: &str, bytes: &[u8]) -> Result<Visual> {
    if file_cmds::extension(file).as_deref() != Some("png") {
        return Err(bad(format!("{}: only PNG files go on a gang sheet", file_cmds::file_name(file))));
    }
    let doc = file_cmds::import(&file_cmds::file_name(file), bytes)?;
    load_document(&doc, &file_cmds::stem(file))
}

/// A PNG file's visual, read from disk.
pub fn load_file(path: &str) -> Result<Visual> {
    let bytes = file_cmds::read_file(path)?;
    load_bytes(path, &bytes)
}

/// `px` (`w × h` interleaved RGBA bytes) turned 90° clockwise: `h × w`.
fn rotate_cw(px: &[u8], w: usize, h: usize) -> Vec<u8> {
    let mut out = vec![0u8; px.len()];
    for y in 0..h {
        for x in 0..w {
            // (x, y) → (h − 1 − y, x) in an image `h` wide.
            let (si, di) = ((y * w + x) * 4, (x * h + (h - 1 - y)) * 4);
            if let (Some(s), Some(d)) = (px.get(si..si + 4), out.get_mut(di..di + 4)) {
                d.copy_from_slice(s);
            }
        }
    }
    out
}

/// The visual at `tw × th` pixels (as printed, so turned first when `rotated`), at the origin.
fn rendition(v: &Visual, tw: i64, th: i64, rotated: bool) -> Result<Surface> {
    crate::allocation::checkpoint("rendering a gang sheet visual")?;
    let (mut src, mut sw, mut sh) = (v.pixels.clone(), i64::from(v.width), i64::from(v.height));
    if rotated {
        let px = src.to_interleaved(Rect::new(0, 0, sw as i32, sh as i32));
        let turned = rotate_cw(&px, sw as usize, sh as usize);
        (sw, sh) = (sh, sw);
        src = Surface::from_interleaved(format(), Rect::new(0, 0, sw as i32, sh as i32), &turned);
    }
    if sw <= 0 || sh <= 0 || tw <= 0 || th <= 0 {
        return Err(bad(format!("{}: nothing to render", v.name)));
    }
    if (sw, sh) == (tw, th) {
        return Ok(src);
    }
    let (kx, ky) = (tw as f64 / sw as f64, th as f64 / sh as f64);
    // The visual's box counts as its canvas: its edge pixels repeat outwards, so an opaque edge
    // stays opaque instead of fading (a DTF film would print the fade as a halo).
    let canvas = Rect::new(0, 0, sw as i32, sh as i32);
    let out = photocraft_algo::resample::resize_surface_in_canvas(&src, kx, ky, photocraft_algo::resample::Resample::BicubicAutomatic, canvas);
    let target = Rect::new(0, 0, tw as i32, th as i32);
    if target.contains_rect(&out.content_bounds()) {
        return Ok(out);
    }
    // Nothing may reach past the box (its neighbours are a margin away, not more).
    let bytes = out.to_interleaved(target);
    Ok(Surface::from_interleaved(format(), target, &bytes))
}

/// One placed visual: the source it shows, where (layout units) and whether it is turned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Placed {
    pub source: usize,
    pub x: i64,
    pub y: i64,
    pub rotated: bool,
}

/// The visual's footprint on the sheet for a source printed `w × h` (unrotated).
pub fn footprint(p: &Placed, w: i64, h: i64) -> Item {
    if p.rotated { Item::new(p.x, p.y, h, w) } else { Item::new(p.x, p.y, w, h) }
}

fn cm(v: f64) -> i64 {
    (v * UNITS_PER_CM as f64).round() as i64
}

fn gang_sheet_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(p, "filmWidth", 1.0, 500.0)?;
    check_range(p, "maxLength", 1.0, 2000.0)?;
    check_range(p, "margin", 0.0, 100.0)?;
    check_range(p, "resolution", 72.0, 1200.0)?;
    let dpi = num(p, "resolution", DEFAULT_RESOLUTION);
    let sheet = Sheet::new(cm(num(p, "filmWidth", DEFAULT_FILM_WIDTH_CM)), cm(num(p, "maxLength", DEFAULT_MAX_LENGTH_CM)), (num(p, "margin", DEFAULT_MARGIN_MM) * 10.0).round() as i64);
    let (iw, il) = sheet.inner();
    if iw <= 0 || il <= 0 {
        return Err(bad("the margins leave no room on the sheet"));
    }
    if gang_sheet::to_px(sheet.width, dpi) > MAX_SHEET_SIDE || gang_sheet::to_px(sheet.max_length, dpi) > MAX_SHEET_SIDE {
        return Err(bad(format!("the sheet is too large at {dpi} ppi (at most {MAX_SHEET_SIDE} px per side)")));
    }
    let Some(list) = p.get("sources").and_then(Value::as_array).filter(|a| !a.is_empty()) else {
        return Err(bad("`sources` must list the visuals: {\"path\": …} or {\"document\": …}"));
    };
    if list.len() > MAX_SOURCES {
        return Err(bad(format!("at most {MAX_SOURCES} visuals per gang sheet")));
    }
    // Each source: its pixels, its print size (layout units, unrotated) and its copies.
    let mut visuals: Vec<(Visual, i64, i64, usize)> = Vec::new();
    for (i, src) in list.iter().enumerate() {
        let v = if let Some(path) = src.get("path").and_then(Value::as_str) {
            load_file(path)?
        } else if let Some(d) = src.get("document").and_then(Value::as_u64) {
            let st = s.documents().get(d as usize).ok_or_else(|| bad(format!("source {i}: no document {d}")))?;
            load_document(&st.doc, &st.doc.name)?
        } else {
            return Err(bad(format!("source {i}: give a \"path\" or a \"document\"")));
        };
        check_range(src, "widthCm", 0.1, 500.0)?;
        check_range(src, "quantity", 1.0, 999.0)?;
        let width = src.get("widthCm").and_then(Value::as_f64).map_or_else(|| v.natural_width(&sheet), cm);
        let (w, h) = gang_sheet::size_for_width(v.width, v.height, width);
        let copies = num(src, "quantity", 1.0).round().clamp(1.0, 999.0) as usize;
        visuals.push((v, w, h, copies));
    }
    // The layout: the window's, or one built here.
    let mut placed: Vec<Placed> = Vec::new();
    let mut unplaced = 0usize;
    match p.get("placements") {
        Some(Value::Array(a)) => {
            if a.len() > MAX_PLACEMENTS {
                return Err(bad(format!("at most {MAX_PLACEMENTS} visuals on one sheet")));
            }
            for (i, pl) in a.iter().enumerate() {
                let source = pl.get("source").and_then(Value::as_u64).map(|v| v as usize).filter(|v| *v < visuals.len());
                let (Some(source), Some(x), Some(y)) = (source, pl.get("x").and_then(Value::as_f64), pl.get("y").and_then(Value::as_f64)) else {
                    return Err(bad(format!("placement {i}: needs a valid \"source\", \"x\" and \"y\" (cm)")));
                };
                if !(x.is_finite() && y.is_finite()) {
                    return Err(bad(format!("placement {i}: x and y must be numbers")));
                }
                let rotated = pl.get("rotated").and_then(Value::as_bool).unwrap_or(false);
                let one = Placed { source, x: cm(x), y: cm(y), rotated };
                let (w, h) = visuals.get(source).map_or((0, 0), |v| (v.1, v.2));
                let r = footprint(&one, w, h);
                if r.x < 0 || r.y < 0 || r.x + r.w > sheet.width || r.y + r.h > sheet.max_length {
                    return Err(bad(format!("placement {i}: off the sheet")));
                }
                placed.push(one);
            }
        }
        None | Some(Value::Null) => {
            let rotate = p.get("rotate").and_then(Value::as_bool).unwrap_or(true);
            let mut slots = Vec::new();
            for (i, v) in visuals.iter().enumerate() {
                slots.extend(std::iter::repeat_n(i, v.3));
            }
            if slots.len() > MAX_PLACEMENTS {
                return Err(bad(format!("at most {MAX_PLACEMENTS} visuals on one sheet")));
            }
            let sizes: Vec<(i64, i64)> = slots.iter().map(|i| visuals.get(*i).map_or((0, 0), |v| (v.1, v.2))).collect();
            for (slot, spot) in slots.iter().zip(sheet.pack(&sizes, &[], rotate)) {
                match spot {
                    Some(sp) => placed.push(Placed { source: *slot, x: sp.x, y: sp.y, rotated: sp.rotated }),
                    None => unplaced += 1,
                }
            }
        }
        Some(_) => return Err(bad("`placements` must be a list")),
    }
    if placed.is_empty() {
        return Err(bad("nothing to place on the sheet"));
    }
    let items: Vec<Item> = placed.iter().map(|pl| visuals.get(pl.source).map_or_else(Item::default, |v| footprint(pl, v.1, v.2))).collect();
    let conflicts = sheet.conflicts(&items).iter().filter(|c| **c).count();
    let length = sheet.used_length(&items).max(1);
    // Render: one layer per visual, in sheet pixels.
    let (pw, ph) = (gang_sheet::to_px(sheet.width, dpi).max(1), gang_sheet::to_px(length, dpi).max(1));
    let mut doc = Document::new("Gang sheet", Size::new(pw as u32, ph as u32), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = dpi as f32;
    doc.layers.clear();
    let mut cache: std::collections::HashMap<(usize, bool), Surface> = std::collections::HashMap::new();
    let mut count = vec![0usize; visuals.len()];
    for (pl, it) in placed.iter().zip(&items) {
        let Some(v) = visuals.get(pl.source) else { continue };
        // The pixel box: both edges rounded, so neighbours never drift apart or overlap.
        let (x0, y0) = (gang_sheet::to_px(it.x, dpi), gang_sheet::to_px(it.y, dpi));
        let (tw, th) = (gang_sheet::to_px(it.x + it.w, dpi) - x0, gang_sheet::to_px(it.y + it.h, dpi) - y0);
        let key = (pl.source, pl.rotated);
        let pixels = match cache.get(&key) {
            Some(px) => px.clone(),
            None => {
                let px = rendition(&v.0, tw.max(1), th.max(1), pl.rotated)?;
                cache.insert(key, px.clone());
                px
            }
        };
        let content = pixels.content_bounds();
        let surf = pixels.translated(x0 as i32, y0 as i32, content);
        let k = count.get_mut(pl.source).map_or(1, |c| {
            *c += 1;
            *c
        });
        let name = if k > 1 { format!("{} {k}", v.0.name) } else { v.0.name.clone() };
        doc.layers.push(Layer::new(name, LayerContent::Raster(surf)));
    }
    let layers = doc.layers.len();
    let mut out = json!({
        "widthCm": sheet.width as f64 / UNITS_PER_CM as f64,
        "lengthCm": length as f64 / UNITS_PER_CM as f64,
        "pixels": [pw, ph],
        "images": layers,
        "conflicts": conflicts,
        "unplaced": unplaced,
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

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: GANG_SHEET,
        label: "Gang Sheet…",
        menu: &["DTF"],
        shortcut: None,
        params: r##"{"sources":[{"path":str|"document":int,"widthCm":0.1..500 (print width; height proportional; default the file's own size),"quantity":1..999=1}],"placements":[{"source":int,"x":cm,"y":cm,"rotated":bool}] (omit to build the layout automatically),"filmWidth":1..500=56 (cm),"maxLength":1..2000=100 (cm),"margin":0..100=5 (mm, around every visual),"resolution":72..1200=300,"rotate":bool=true (automatic layout may turn visuals 90°),"output":str (a .png path),"open":bool=true}"##,
        enabled: crate::commands::always,
        run: gang_sheet_cmd,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `w × h` PNG of an opaque `color` square padded by `pad` transparent pixels, at `dpi`.
    fn png(dir: &std::path::Path, name: &str, w: u32, h: u32, pad: u32, dpi: f32) -> String {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": w + 2 * pad, "height": h + 2 * pad, "resolution": dpi, "background": "transparent"})).unwrap();
        s.execute("select.rect", json!({"x": pad, "y": pad, "width": w, "height": h})).unwrap();
        s.execute("edit.fill", json!({"color": "#c03020"})).unwrap();
        let path = dir.join(name).to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": path})).unwrap();
        path
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dtf-gang-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn visuals_are_cropped_to_their_visible_pixels() {
        let dir = temp_dir("crop");
        let path = png(&dir, "logo.png", 120, 60, 10, 300.0);
        let v = load_file(&path).unwrap();
        assert_eq!((v.width, v.height), (120, 60));
        assert_eq!(v.name, "logo");
        assert!((v.dpi - 300.0).abs() < 0.5);
        // 120 px at 300 ppi: 1.016 cm.
        assert_eq!(v.natural_width(&Sheet::new(5600, 10_000, 50)), 102);
        assert!(load_bytes("logo.jpg", &[]).is_err(), "PNG only");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn automatic_sheet_renders_every_copy_and_writes_a_png() {
        let dir = temp_dir("auto");
        let a = png(&dir, "a.png", 300, 150, 4, 300.0);
        let b = png(&dir, "b.png", 200, 200, 0, 300.0);
        let out = dir.join("sheet.png").to_string_lossy().into_owned();
        let mut s = Session::new();
        let r = s
            .execute(
                GANG_SHEET,
                json!({"sources": [{"path": a, "widthCm": 10, "quantity": 3}, {"path": b, "widthCm": 5, "quantity": 2}], "filmWidth": 30, "output": out}),
            )
            .unwrap();
        assert_eq!(r["images"], 5);
        assert_eq!(r["conflicts"], 0);
        assert_eq!(r["unplaced"], 0);
        // 30 cm at 300 ppi.
        assert_eq!(r["pixels"][0], 3543);
        let st = s.active().unwrap();
        assert_eq!(st.doc.layers.len(), 5);
        assert!((st.doc.resolution_dpi - 300.0).abs() < 0.01);
        assert_eq!(st.doc.layers[1].name, "a 2");
        // Every layer is a visual at its print size: 10 cm = 1181 px wide (or high, turned).
        let sizes: Vec<(i32, i32)> = st
            .doc
            .layers
            .iter()
            .map(|l| match &l.content {
                LayerContent::Raster(px) => {
                    let b = px.content_bounds();
                    (b.width(), b.height())
                }
                _ => (0, 0),
            })
            .collect();
        assert!(sizes.iter().filter(|(w, h)| (*w - 1181).abs() <= 1 || (*h - 1181).abs() <= 1).count() >= 3, "{sizes:?}");
        let written = std::fs::read(&out).unwrap();
        let reopened = photocraft_io::import("sheet.png", &written).unwrap().document;
        assert_eq!(reopened.size.width, 3543);
        assert!((reopened.resolution_dpi - 300.0).abs() < 0.5, "pHYs keeps 300 ppi");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn drawn_layout_is_rendered_where_it_was_placed() {
        let dir = temp_dir("drawn");
        let a = png(&dir, "a.png", 100, 100, 0, 254.0);
        let mut s = Session::new();
        let r = s
            .execute(
                GANG_SHEET,
                json!({"sources": [{"path": a, "widthCm": 2}], "placements": [{"source": 0, "x": 1, "y": 1}, {"source": 0, "x": 3.5, "y": 1, "rotated": true}], "filmWidth": 10, "resolution": 254}),
            )
            .unwrap();
        // 254 ppi: 100 px per cm. Bottom of the visuals at 3 cm, plus the 5 mm margin.
        assert_eq!(r["pixels"], json!([1000, 350]));
        assert_eq!(r["conflicts"], 0);
        let st = s.active().unwrap();
        let b = match &st.doc.layers[1].content {
            LayerContent::Raster(px) => px.content_bounds(),
            _ => Rect::EMPTY,
        };
        assert_eq!(b, Rect::new(350, 100, 550, 300));
        // Too close together: reported, still rendered.
        let r = s
            .execute(GANG_SHEET, json!({"sources": [{"path": a, "widthCm": 2}], "placements": [{"source": 0, "x": 1, "y": 1}, {"source": 0, "x": 3.2, "y": 1}], "filmWidth": 10, "open": false}))
            .unwrap();
        assert_eq!(r["conflicts"], 2);
        assert!(r.get("document").is_none());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn open_document_is_flattened_onto_the_sheet() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 80, "height": 40, "resolution": 300})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("edit.fill", json!({"color": "#2040c0"})).unwrap();
        let r = s.execute(GANG_SHEET, json!({"sources": [{"document": 0, "quantity": 2}]})).unwrap();
        assert_eq!(r["images"], 2);
        assert_eq!(s.documents().len(), 2);
    }

    #[test]
    fn bad_params_are_errors() {
        let mut s = Session::new();
        for p in [
            json!({}),
            json!({"sources": []}),
            json!({"sources": [{}]}),
            json!({"sources": [{"document": 9}]}),
            json!({"sources": [{"path": "/nonexistent/x.png"}]}),
            json!({"sources": [{"path": "x.png"}], "filmWidth": -3}),
            json!({"sources": [{"path": "x.png"}], "margin": 1e9}),
            json!({"sources": [{"path": "x.png"}], "filmWidth": 1, "margin": 6}),
            json!({"sources": [{"path": "x.png"}], "filmWidth": 500, "resolution": 1200}),
        ] {
            assert!(s.execute(GANG_SHEET, p.clone()).is_err(), "{p}");
        }
        s.execute("file.new", json!({"width": 40, "height": 40})).unwrap();
        s.execute("layer.new.layer", json!({})).unwrap();
        s.execute("edit.fill", json!({"color": "#2040c0"})).unwrap();
        for p in [
            json!({"sources": [{"document": 0}], "placements": [{"source": 3, "x": 1, "y": 1}]}),
            json!({"sources": [{"document": 0}], "placements": [{"source": 0, "x": -1, "y": 1}]}),
            json!({"sources": [{"document": 0}], "placements": [{"source": 0, "x": 99, "y": 1}]}),
            json!({"sources": [{"document": 0}], "placements": "all"}),
            json!({"sources": [{"document": 0}], "placements": []}),
            json!({"sources": [{"document": 0, "widthCm": 0}]}),
            json!({"sources": [{"document": 0}], "output": "sheet.tif"}),
        ] {
            assert!(s.execute(GANG_SHEET, p.clone()).is_err(), "{p}");
        }
    }

    #[test]
    fn rotation_turns_clockwise() {
        // 2 × 1: [a b] → 1 × 2: [a; b] turned clockwise is [a; b] read top to bottom.
        let px = [1, 0, 0, 255, 2, 0, 0, 255];
        assert_eq!(rotate_cw(&px, 2, 1), px.to_vec());
        // 1 × 2: [a; b] → 2 × 1: [b a].
        assert_eq!(rotate_cw(&[1, 0, 0, 255, 2, 0, 0, 255], 1, 2), vec![2, 0, 0, 255, 1, 0, 0, 255]);
    }
}
