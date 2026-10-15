// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF menu (DTF Craft): preparation commands for direct-to-film printing.
//!
//! * `filter.dtf.halftone`: DTF Halftone (see [`photocraft_algo::dtf`]): the screen is given in
//!   lines per inch and converted to a cell size with the document resolution.
//! * `filter.dtf.removeSemiTransparent`: alpha threshold, every pixel becomes opaque or clear.
//!
//! Both run through [`crate::filters::run_filter`], so selections, smart objects (as smart
//! filters), history and the schema dialog with live preview work as for every other filter.

use photocraft_algo::dtf::{HalftoneSpec, HalftoneTarget};
use photocraft_algo::FilterParams;
use photocraft_algo::quantize::HalftoneShape;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

pub const HALFTONE: &str = "filter.dtf.halftone";
pub const REMOVE_SEMI_TRANSPARENT: &str = "filter.dtf.removeSemiTransparent";

/// Cell size (pixels) the halftone uses when no document resolution was resolved.
const FALLBACK_DPI: f64 = 300.0;

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

fn num(p: &Value, k: &str, d: f64) -> f64 {
    p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(d)
}

fn text<'a>(p: &'a Value, k: &str, d: &'a str) -> &'a str {
    p.get(k).and_then(Value::as_str).unwrap_or(d)
}

/// The algorithm parameters for the DTF filter ids (dialog units: lpi, degrees, percent, levels).
pub fn params_for(id: &str, p: &Value) -> Option<FilterParams> {
    match id {
        HALFTONE => {
            let lpi = num(p, "frequency", 25.0).clamp(1.0, 300.0);
            let cell = num(p, "__cellPx", FALLBACK_DPI / lpi).clamp(2.0, 4096.0);
            let spec = HalftoneSpec {
                cell: cell as f32,
                angle: num(p, "angle", 22.5).clamp(-180.0, 180.0) as f32,
                shape: HalftoneShape::from_id(text(p, "shape", "round")).unwrap_or(HalftoneShape::Round),
                target: HalftoneTarget::from_id(text(p, "target", "alpha")).unwrap_or(HalftoneTarget::Alpha),
                min_dot: (num(p, "minDot", 5.0).clamp(0.0, 100.0) / 100.0) as f32,
                max_dot: (num(p, "maxDot", 95.0).clamp(0.0, 100.0) / 100.0) as f32,
                low: (num(p, "inBlack", 0.0).clamp(0.0, 255.0) / 255.0) as f32,
                high: (num(p, "inWhite", 255.0).clamp(0.0, 255.0) / 255.0) as f32,
                gamma: num(p, "gamma", 1.0).clamp(0.1, 9.99) as f32,
            };
            Some(FilterParams::DtfHalftone { spec })
        }
        REMOVE_SEMI_TRANSPARENT => Some(FilterParams::DtfAlphaThreshold { threshold: (num(p, "threshold", 62.0).clamp(1.0, 100.0) / 100.0) as f32 }),
        _ => None,
    }
}

fn check_choice(cmd: &str, p: &Value, key: &str, allowed: &[&str]) -> Result<()> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(()),
        Some(Value::String(s)) if allowed.contains(&s.as_str()) => Ok(()),
        Some(v) => Err(bad(cmd, format!("`{key}` must be one of {} (got {v})", allowed.join("|")))),
    }
}

fn check_range(cmd: &str, p: &Value, key: &str, lo: f64, hi: f64) -> Result<()> {
    match p.get(key) {
        None | Some(Value::Null) => Ok(()),
        Some(v) => match v.as_f64() {
            Some(x) if x.is_finite() && (lo..=hi).contains(&x) => Ok(()),
            _ => Err(bad(cmd, format!("`{key}` must be a number in {lo}..{hi} (got {v})"))),
        },
    }
}

fn halftone(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(HALFTONE, p, "frequency", 1.0, 300.0)?;
    check_range(HALFTONE, p, "angle", -180.0, 180.0)?;
    check_range(HALFTONE, p, "minDot", 0.0, 100.0)?;
    check_range(HALFTONE, p, "maxDot", 0.0, 100.0)?;
    check_range(HALFTONE, p, "inBlack", 0.0, 255.0)?;
    check_range(HALFTONE, p, "inWhite", 0.0, 255.0)?;
    check_range(HALFTONE, p, "gamma", 0.1, 9.99)?;
    check_choice(HALFTONE, p, "shape", &["round", "ellipse", "line", "square", "diamond", "cross"])?;
    check_choice(HALFTONE, p, "target", &["alpha", "darkGarment", "lightGarment"])?;
    let dpi = s.active().map_or(FALLBACK_DPI, |d| f64::from(d.doc.resolution_dpi)).max(1.0);
    let lpi = num(p, "frequency", 25.0).clamp(1.0, 300.0);
    let mut p2 = if p.is_object() { p.clone() } else { json!({}) };
    if let Value::Object(m) = &mut p2 {
        m.insert("__cellPx".into(), json!(dpi / lpi));
    }
    crate::filters::run_filter(s, HALFTONE, &p2)
}

fn remove_semi(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(REMOVE_SEMI_TRANSPARENT, p, "threshold", 1.0, 100.0)?;
    crate::filters::run_filter(s, REMOVE_SEMI_TRANSPARENT, p)
}


// ---------------------------------------------------------------- gang sheet

pub const GANG_SHEET: &str = "dtf.gangSheet";

/// Largest sheet side the gang sheet builds, in pixels (about 4 m at 300 ppi per side).
const MAX_SHEET_SIDE: f64 = 60_000.0;
/// Most images (copies included) one gang sheet run places.
const MAX_ITEMS: usize = 2_000;

fn cm_to_px(cm: f64, dpi: f64) -> f64 {
    cm / 2.54 * dpi
}

/// One image to place: its name and its pixels (straight RGBA rows of `w × h`).
struct SheetItem {
    name: String,
    w: i64,
    h: i64,
    data: Vec<f32>,
}

/// Loads, trims and scales one file for a sheet at `dpi`.
fn load_item(path: &str, dpi: f64, trim: bool, keep_size: bool, fmt: photocraft_color::PixelFormat) -> Result<SheetItem> {
    let bytes = crate::file_cmds::read_file(path)?;
    let doc = crate::file_cmds::import(&crate::file_cmds::file_name(path), &bytes)?;
    let mut surf = crate::file_cmds::flattened(&doc, fmt);
    let full = photocraft_geom::Rect::new(0, 0, doc.size.width as i32, doc.size.height as i32);
    let mut rect = if trim { surf.content_bounds().intersect(&full) } else { full };
    if rect.is_empty() {
        return Err(bad(GANG_SHEET, format!("{}: the image is empty", crate::file_cmds::file_name(path))));
    }
    let src_dpi = f64::from(doc.resolution_dpi);
    if keep_size && src_dpi.is_finite() && src_dpi > 0.0 && (src_dpi - dpi).abs() > 0.01 {
        let k = dpi / src_dpi;
        let (nw, nh) = (f64::from(rect.width()) * k, f64::from(rect.height()) * k);
        if nw > MAX_SHEET_SIDE || nh > MAX_SHEET_SIDE {
            return Err(bad(GANG_SHEET, format!("{}: too large at {dpi} ppi", crate::file_cmds::file_name(path))));
        }
        surf = photocraft_algo::resample::resize_surface(&surf, k, k, photocraft_algo::resample::Resample::BicubicAutomatic);
        rect = photocraft_algo::resample::scaled_rect(rect, k, k);
        if trim {
            rect = surf.content_bounds().intersect(&rect);
        }
    }
    if rect.is_empty() {
        return Err(bad(GANG_SHEET, format!("{}: the image is empty", crate::file_cmds::file_name(path))));
    }
    Ok(SheetItem { name: crate::file_cmds::stem(path), w: i64::from(rect.width()), h: i64::from(rect.height()), data: surf.read_region(rect) })
}

/// The item's pixels turned 90° clockwise (`w × h` becomes `h × w`).
fn rotate_cw(data: &[f32], w: usize, h: usize, ch: usize) -> Vec<f32> {
    let mut out = vec![0.0f32; data.len()];
    for y in 0..h {
        for x in 0..w {
            // (x, y) → (h − 1 − y, x) in a `h` wide image.
            let (nx, ny) = (h - 1 - y, x);
            let (si, di) = ((y * w + x) * ch, (ny * h + nx) * ch);
            if let (Some(s), Some(d)) = (data.get(si..si + ch), out.get_mut(di..di + ch)) {
                d.copy_from_slice(s);
            }
        }
    }
    out
}

fn gang_sheet(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(GANG_SHEET, p, "filmWidth", 1.0, 500.0)?;
    check_range(GANG_SHEET, p, "sheetLength", 0.0, 2000.0)?;
    check_range(GANG_SHEET, p, "resolution", 72.0, 1200.0)?;
    check_range(GANG_SHEET, p, "spacing", 0.0, 100.0)?;
    check_range(GANG_SHEET, p, "margin", 0.0, 100.0)?;
    check_range(GANG_SHEET, p, "copies", 1.0, 999.0)?;
    let paths: Vec<String> = match p.get("input").or_else(|| p.get("paths")) {
        Some(Value::Array(a)) => a.iter().filter_map(Value::as_str).map(str::to_string).collect(),
        Some(Value::String(dir)) if !dir.trim().is_empty() => crate::file_cmds::list_images(dir)?,
        _ => return Err(bad(GANG_SHEET, "`input` must be a folder or a list of image files")),
    };
    if paths.is_empty() {
        return Err(bad(GANG_SHEET, "no image files to place"));
    }
    let dpi = num(p, "resolution", 300.0);
    let sheet_w = cm_to_px(num(p, "filmWidth", 56.0), dpi).round();
    let length_cm = num(p, "sheetLength", 100.0);
    let max_len = if length_cm > 0.0 { cm_to_px(length_cm, dpi).round() } else { 0.0 };
    let gap = (num(p, "spacing", 5.0) / 25.4 * dpi).round() as i64;
    let margin = (num(p, "margin", 5.0) / 25.4 * dpi).round() as i64;
    let copies = num(p, "copies", 1.0).round().clamp(1.0, 999.0) as usize;
    let rotate = p.get("rotate").and_then(Value::as_bool).unwrap_or(true);
    let trim = p.get("trim").and_then(Value::as_bool).unwrap_or(true);
    let keep_size = p.get("keepPrintSize").and_then(Value::as_bool).unwrap_or(true);
    if sheet_w > MAX_SHEET_SIDE || max_len > MAX_SHEET_SIDE {
        return Err(bad(GANG_SHEET, format!("the sheet is too large at {dpi} ppi (at most {MAX_SHEET_SIDE} px per side)")));
    }
    let inner_w = sheet_w as i64 - 2 * margin;
    if inner_w <= 0 || (max_len > 0.0 && max_len as i64 - 2 * margin <= 0) {
        return Err(bad(GANG_SHEET, "the margins leave no room on the sheet"));
    }
    if paths.len().saturating_mul(copies) > MAX_ITEMS {
        return Err(bad(GANG_SHEET, format!("at most {MAX_ITEMS} images (copies included) per gang sheet")));
    }
    let fmt = photocraft_color::PixelFormat::new(photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8, true);
    let ch = fmt.channels();
    let mut items = Vec::new();
    let mut errors = Vec::new();
    for path in &paths {
        match load_item(path, dpi, trim, keep_size, fmt) {
            Ok(it) => items.push(it),
            Err(e) => errors.push(json!({"file": path, "error": e.to_string()})),
        }
    }
    // Copies share the pixels: placements refer to the item by index.
    let mut slots: Vec<usize> = Vec::new();
    for (i, _) in items.iter().enumerate() {
        slots.extend(std::iter::repeat_n(i, copies));
    }
    let sizes: Vec<(i64, i64)> = slots.iter().filter_map(|i| items.get(*i)).map(|it| (it.w, it.h)).collect();
    let inner_len = if max_len > 0.0 { max_len as i64 - 2 * margin } else { 0 };
    let (sheets, rejected) = photocraft_algo::dtf::pack(&sizes, inner_w, inner_len, gap, rotate);
    for r in &rejected {
        if let Some(it) = slots.get(*r).and_then(|i| items.get(*i)) {
            errors.push(json!({"file": it.name, "error": "larger than the sheet"}));
        }
    }
    if sheets.is_empty() {
        return Err(bad(GANG_SHEET, "nothing could be placed on the sheet"));
    }
    let width_cm = num(p, "filmWidth", 56.0);
    let mut out = Vec::new();
    let total = sheets.len();
    for (n, sheet) in sheets.iter().enumerate() {
        let h = (sheet.length + 2 * margin).max(1);
        let size = photocraft_doc::Size::new(sheet_w as u32, h as u32);
        let name = if total > 1 { format!("Gang sheet {} of {total}", n + 1) } else { "Gang sheet".to_string() };
        let mut doc = photocraft_doc::Document::new(name.clone(), size, photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::U8);
        doc.resolution_dpi = dpi as f32;
        doc.layers.clear();
        let mut count: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
        for pl in &sheet.placements {
            let Some(item) = slots.get(pl.item).and_then(|i| items.get(*i)) else { continue };
            let (w, hh, data) = if pl.rotated {
                (item.h, item.w, rotate_cw(&item.data, item.w as usize, item.h as usize, ch))
            } else {
                (item.w, item.h, item.data.clone())
            };
            let (x, y) = ((pl.x + margin) as i32, (pl.y + margin) as i32);
            let mut surf = photocraft_raster::Surface::new(fmt);
            surf.write_region(photocraft_geom::Rect::new(x, y, x + w as i32, y + hh as i32), &data);
            let k = count.entry(slots.get(pl.item).copied().unwrap_or(0)).or_insert(0);
            *k += 1;
            let label = if *k > 1 { format!("{} {}", item.name, k) } else { item.name.clone() };
            doc.layers.push(photocraft_doc::Layer::new(label, photocraft_doc::LayerContent::Raster(surf)));
        }
        let placed = doc.layers.len();
        let length_cm = f64::from(size.height) / dpi * 2.54;
        let index = s.add_document(doc, None);
        if let Some(d) = s.active_mut() {
            d.active_layer = d.doc.layers.last().map(|l| l.id);
        }
        out.push(json!({"document": index, "name": name, "widthCm": width_cm, "lengthCm": (length_cm * 10.0).round() / 10.0, "images": placed}));
    }
    Ok(json!({"sheets": out, "errors": errors}))
}

// ---------------------------------------------------------------- AI upscale

pub const UPSCALE: &str = "dtf.upscale";

/// Upscales one raster layer's pixels (taken before the document was resized) by `scale` with
/// the network, returning the new surface at the scaled position. `None` when cancelled.
fn upscale_surface(
    model: &photocraft_algo::upscale::SrModel,
    surf: &photocraft_raster::Surface,
    canvas: photocraft_geom::Rect,
    scale: f64,
    alpha_model: bool,
    ir: &photocraft_raster::Interrupt,
) -> Option<photocraft_raster::Surface> {
    use photocraft_algo::upscale::SCALE;
    let fmt = surf.format();
    let ch = surf.channels();
    let bounds = surf.content_bounds().intersect(&canvas);
    if bounds.is_empty() || ch == 0 {
        return Some(photocraft_raster::Surface::new(fmt));
    }
    let (w, h) = (bounds.width() as usize, bounds.height() as usize);
    let src = surf.read_region(bounds);
    // To RGBA: grey (with alpha) is replicated, colour models with 3 channels pass through.
    let alpha = fmt.alpha;
    let colour = if alpha { ch - 1 } else { ch };
    let mut rgba = Vec::with_capacity(w * h * 4);
    for px in src.chunks_exact(ch) {
        let (r, g, b) = if colour >= 3 { (px[0], px[1], px[2]) } else { (px[0], px[0], px[0]) };
        rgba.extend_from_slice(&[r, g, b, if alpha { px[ch - 1] } else { 1.0 }]);
    }
    let big = model.upscale(&rgba, w, h, 4, alpha && alpha_model, 128, 16, ir)?;
    let (bw, bh) = (w * SCALE, h * SCALE);
    let mut native = Vec::with_capacity(bw * bh * ch);
    for px in big.chunks_exact(4) {
        if colour >= 3 {
            native.extend_from_slice(&px[..3]);
            native.extend(std::iter::repeat_n(0.0, colour - 3));
        } else {
            let grey = (px[0] + px[1] + px[2]) / 3.0;
            native.extend(std::iter::repeat_n(grey, colour));
        }
        if alpha {
            native.push(px[3]);
        }
    }
    let k = SCALE as i32;
    let at = photocraft_geom::Rect::new(bounds.x0 * k, bounds.y0 * k, bounds.x0 * k + bw as i32, bounds.y0 * k + bh as i32);
    let mut out = photocraft_raster::Surface::new(fmt);
    out.write_region(at, &native);
    let f = scale / SCALE as f64;
    if (f - 1.0).abs() > 1e-9 {
        out = photocraft_algo::resample::resize_surface(&out, f, f, photocraft_algo::resample::Resample::BicubicSharper);
    }
    Some(out)
}

fn upscale(s: &mut Session, p: &Value) -> Result<Value> {
    check_choice(UPSCALE, p, "scale", &["2", "3", "4"])?;
    check_choice(UPSCALE, p, "alpha", &["model", "smooth"])?;
    let scale: f64 = match p.get("scale") {
        Some(Value::Number(n)) => n.as_f64().filter(|v| [2.0, 3.0, 4.0].contains(v)).ok_or_else(|| bad(UPSCALE, "`scale` must be 2, 3 or 4"))?,
        _ => text(p, "scale", "2").parse().unwrap_or(2.0),
    };
    let alpha_model = text(p, "alpha", "model") == "model";
    let keep_print = p.get("keepPrintSize").and_then(Value::as_bool).unwrap_or(true);
    let model = photocraft_algo::upscale::SrModel::bundled().map_err(|e| EngineError::Other(format!("AI model: {e}")))?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if !matches!(st.doc.mode, photocraft_color::ColorMode::Rgb | photocraft_color::ColorMode::Grayscale) {
        return Err(bad(UPSCALE, "AI upscaling works on RGB and Grayscale documents"));
    }
    let (ow, oh) = (st.doc.size.width, st.doc.size.height);
    let (nw, nh) = ((f64::from(ow) * scale).round() as u32, (f64::from(oh) * scale).round() as u32);
    crate::image_cmds::check_resample_budget(UPSCALE, &st.doc, nw, nh, "")?;
    crate::jobs::edit_job(
        s,
        "AI Upscale",
        move |doc, _, ctx| {
            let canvas = photocraft_geom::Rect::new(0, 0, doc.size.width as i32, doc.size.height as i32);
            let originals: Vec<(photocraft_doc::LayerId, photocraft_raster::Surface)> = doc
                .walk()
                .into_iter()
                .filter_map(|(_, _, l)| match &l.content {
                    photocraft_doc::LayerContent::Raster(surf) => Some((l.id, surf.clone())),
                    _ => None,
                })
                .collect();
            crate::image_cmds::resample_doc(doc, nw, nh, photocraft_algo::resample::Resample::BicubicSmoother);
            if keep_print {
                doc.resolution_dpi = (f64::from(doc.resolution_dpi) * scale).clamp(1.0, 10_000.0) as f32;
            }
            let n = originals.len().max(1);
            for (i, (id, surf)) in originals.iter().enumerate() {
                let (lo, hi) = (i as f32 / n as f32, (i + 1) as f32 / n as f32);
                let big = ctx.stage(lo, hi, "AI Upscale", |ir| upscale_surface(model, surf, canvas, scale, alpha_model, ir)).ok_or(EngineError::Cancelled)?;
                if let Some(target) = doc.layer_mut(*id).and_then(photocraft_doc::Layer::surface_mut) {
                    *target = big;
                }
            }
            Ok(json!({"width": nw, "height": nh, "layers": originals.len(), "resolution": doc.resolution_dpi}))
        },
        |r| r,
    )
}

// ---------------------------------------------------------------- BRODERIE workflow windows

pub const CONFORMITY: &str = "dtf.conformity";
pub const CONTROL_BACKGROUND: &str = "dtf.controlBackground";
pub const REMOVE_CONTROL_BACKGROUND: &str = "dtf.removeControlBackground";
pub const DEFRINGE: &str = "dtf.defringe";
pub const SHARPEN: &str = "dtf.sharpen";
pub const SATURATION: &str = "dtf.saturation";

/// Name of the control background fill layer (as the BRODERIE Photoshop actions call it).
const BACKFILL: &str = "backfill";

/// Print conformity: the artwork layer renamed, the document resized to a print height at a
/// resolution, set to 8 or 16 bits and trimmed to its visible pixels.
fn conformity(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(CONFORMITY, p, "height", 0.0, 500.0)?;
    check_range(CONFORMITY, p, "resolution", 72.0, 1200.0)?;
    check_choice(CONFORMITY, p, "depth", &["8", "16"])?;
    check_choice(CONFORMITY, p, "resample", &["bicubicAutomatic", "preserveDetails", "bicubic", "bicubicSmoother", "bicubicSharper", "lanczos", "bilinear", "nearest"])?;
    let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
    let has_background = doc.layers.first().is_some_and(|l| doc.is_background(l.id));
    if has_background {
        s.execute("layer.new.layerFromBackground", json!({}))?;
    }
    let name = text(p, "layerName", "origin").trim().to_string();
    if !name.is_empty() {
        s.execute("layer.setProps", json!({"name": name}))?;
    }
    let dpi = num(p, "resolution", 300.0);
    let height_cm = num(p, "height", 27.0);
    let resample = text(p, "resample", "bicubicAutomatic");
    if height_cm > 0.0 {
        let px = (height_cm / 2.54 * dpi).round().max(1.0);
        let d = &s.active().ok_or(EngineError::NoDocument)?.doc;
        let w = (f64::from(d.size.width) * px / f64::from(d.size.height.max(1))).round().max(1.0);
        s.execute("image.imageSize", json!({"width": w, "height": px, "resolution": dpi, "resample": resample}))?;
    } else {
        s.execute("image.imageSize", json!({"resolution": dpi, "resample": "none"}))?;
    }
    s.execute(if text(p, "depth", "8") == "16" { "image.mode.bits16" } else { "image.mode.bits8" }, json!({}))?;
    if p.get("trim").and_then(Value::as_bool).unwrap_or(true) {
        s.execute("image.trim", json!({"basedOn": "transparent"}))?;
    }
    let d = &s.active().ok_or(EngineError::NoDocument)?.doc;
    Ok(json!({"width": d.size.width, "height": d.size.height, "resolution": d.resolution_dpi}))
}

/// A solid colour fill named "backfill" under everything, to judge the artwork on the garment colour.
fn control_background(s: &mut Session, p: &Value) -> Result<Value> {
    let color = text(p, "color", "#231c1c").to_string();
    if !(color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())) {
        return Err(bad(CONTROL_BACKGROUND, "`color` must be #rrggbb"));
    }
    let previous = s.active().and_then(|d| d.active_layer);
    s.execute("layer.newFillLayer.solidColor", json!({"color": color}))?;
    s.execute("layer.setProps", json!({"name": BACKFILL}))?;
    s.execute("layer.arrange.sendToBack", json!({}))?;
    if let Some(l) = previous {
        s.execute("layer.select", json!({"layer": l.0}))?;
    }
    Ok(Value::Null)
}

/// Deletes every layer named "backfill".
fn remove_control_background(s: &mut Session, _: &Value) -> Result<Value> {
    let ids: Vec<u64> = s.active().ok_or(EngineError::NoDocument)?.doc.walk().into_iter().filter(|(_, _, l)| l.name == BACKFILL).map(|(_, _, l)| l.id.0).collect();
    if ids.is_empty() {
        return Err(EngineError::Other("no control background (\"backfill\" layer) in this document".into()));
    }
    for id in &ids {
        s.execute("layer.delete", json!({"layer": id}))?;
    }
    Ok(json!({"deleted": ids.len()}))
}

fn defringe(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(DEFRINGE, p, "width", 1.0, 200.0)?;
    s.execute("layer.matting.defringe", json!({"width": num(p, "width", 1.0).round()}))
}

fn sharpen(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(SHARPEN, p, "amount", 1.0, 500.0)?;
    check_range(SHARPEN, p, "radius", 0.1, 64.0)?;
    check_range(SHARPEN, p, "reduceNoise", 0.0, 100.0)?;
    s.execute(
        "filter.sharpen.smartSharpen",
        json!({"amount": num(p, "amount", 100.0), "radius": num(p, "radius", 1.0), "reduceNoise": num(p, "reduceNoise", 10.0)}),
    )
}

fn saturation(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(SATURATION, p, "saturation", -100.0, 100.0)?;
    s.execute("image.adjustments.hueSaturation", json!({"hue": 0, "saturation": num(p, "saturation", 15.0).round(), "lightness": 0, "colorize": false}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: CONFORMITY,
            label: "Print Conformity…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"height":0..500=27 (cm; 0 keeps the size),"resolution":72..1200=300,"resample":"bicubicAutomatic|preserveDetails|bicubic|bicubicSmoother|bicubicSharper|lanczos|bilinear|nearest"="bicubicAutomatic","depth":"8|16"="8","trim":bool=true,"layerName":str="origin"}"##,
            enabled: has_doc,
            run: conformity,
            journal: true,
        },
        CommandSpec {
            id: CONTROL_BACKGROUND,
            label: "Control Background…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"color":color=#231c1c} (a solid fill named "backfill" under every layer)"##,
            enabled: has_doc,
            run: control_background,
            journal: true,
        },
        CommandSpec {
            id: REMOVE_CONTROL_BACKGROUND,
            label: "Remove Control Background",
            menu: &["DTF"],
            shortcut: None,
            params: "{} (deletes the layers named \"backfill\")",
            enabled: has_doc,
            run: remove_control_background,
            journal: true,
        },
        CommandSpec {
            id: DEFRINGE,
            label: "Defringe…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"width":1..200=1}"##,
            enabled: crate::filters::has_filterable_layer,
            run: defringe,
            journal: true,
        },
        CommandSpec {
            id: SHARPEN,
            label: "Smart Sharpen…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"amount":1..500=100,"radius":0.1..64=1,"reduceNoise":0..100=10}"##,
            enabled: crate::filters::has_filterable_layer,
            run: sharpen,
            journal: true,
        },
        CommandSpec {
            id: SATURATION,
            label: "Saturation…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"saturation":-100..100=15}"##,
            enabled: crate::filters::has_filterable_layer,
            run: saturation,
            journal: true,
        },
        CommandSpec {
            id: UPSCALE,
            label: "AI Upscale…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"scale":"2|3|4"="2","alpha":"model|smooth"="model" (edges of the transparency through the network, or smoothly enlarged),"keepPrintSize":bool=true (raise the resolution so the print size stays)} → {width,height,layers,resolution}. Real-ESRGAN general ×4 model on the CPU; raster layers are enlarged by the network, other layers resampled"##,
            enabled: has_doc,
            run: upscale,
            journal: true,
        },
        CommandSpec {
            id: GANG_SHEET,
            label: "Gang Sheet…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"input":folder|[paths],"filmWidth":1..500=56 (cm),"sheetLength":0..2000=100 (cm per sheet; 0 = one sheet as long as needed),"resolution":72..1200=300,"spacing":0..100=5 (mm between images),"margin":0..100=5 (mm),"copies":1..999=1 (of each image),"rotate":bool=true,"trim":bool=true (crop transparent borders),"keepPrintSize":bool=true (scale each file from its own resolution)} → {sheets:[{document,name,widthCm,lengthCm,images}], errors}"##,
            enabled: crate::commands::always,
            run: gang_sheet,
            journal: true,
        },
        CommandSpec {
            id: HALFTONE,
            label: "DTF Halftone…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"target":"alpha|darkGarment|lightGarment"="alpha","frequency":1..300=25,"angle":-180..180=22.5,"shape":"round|ellipse|line|square|diamond|cross"="round","minDot":0..100=5,"maxDot":0..100=95,"inBlack":0..255=0,"inWhite":0..255=255,"gamma":0.1..9.99=1} (frequency in lines/inch at the document resolution; alpha becomes 0 or 1, colours are kept)"##,
            enabled: crate::filters::has_filterable_layer,
            run: halftone,
            journal: true,
        },
        CommandSpec {
            id: REMOVE_SEMI_TRANSPARENT,
            label: "Remove Semi-Transparent Pixels…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"threshold":1..100=62} (% opacity: at or above becomes opaque, below transparent)"##,
            enabled: crate::filters::has_filterable_layer,
            run: remove_semi,
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session_with_ramp() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 200, "height": 40, "resolution": 300, "background": "transparent"})).unwrap();
        // Paint an alpha ramp on the active layer.
        s.edit("Ramp", |doc, active| {
            let id = active.unwrap();
            let surf = doc.layer_mut(id).unwrap().surface_mut().unwrap();
            for x in 0..200 {
                for y in 0..40 {
                    surf.write_pixel(x, y, &[1.0, 0.0, 0.0, x as f32 / 199.0]);
                }
            }
            Ok(())
        })
        .unwrap();
        s
    }

    fn alphas(s: &Session) -> Vec<f32> {
        let d = s.active().unwrap();
        let l = d.doc.layer(d.active_layer.unwrap()).unwrap();
        let surf = l.surface().unwrap();
        let mut v = vec![];
        for y in 0..40 {
            for x in 0..200 {
                v.push(surf.pixel(x, y)[3]);
            }
        }
        v
    }

    #[test]
    fn remove_semi_transparent_makes_alpha_binary_and_undoes() {
        let mut s = session_with_ramp();
        let before = alphas(&s);
        s.execute(REMOVE_SEMI_TRANSPARENT, json!({"threshold": 50})).unwrap();
        let a = alphas(&s);
        assert!(a.iter().all(|v| *v == 0.0 || *v == 1.0));
        assert!(a.iter().any(|v| *v == 1.0) && a.iter().any(|v| *v == 0.0));
        assert!(s.undo());
        assert_eq!(alphas(&s), before);
    }

    #[test]
    fn halftone_is_binary_and_uses_the_resolution() {
        let mut s = session_with_ramp();
        s.execute(HALFTONE, json!({"frequency": 30, "angle": 0, "minDot": 0, "maxDot": 100})).unwrap();
        let a = alphas(&s);
        assert!(a.iter().all(|v| *v == 0.0 || *v == 1.0));
        let mean: f32 = a.iter().sum::<f32>() / a.len() as f32;
        assert!((mean - 0.5).abs() < 0.08, "mean coverage {mean}");
    }

    #[test]
    fn bad_params_are_refused_without_editing() {
        let mut s = session_with_ramp();
        let before = alphas(&s);
        assert!(s.execute(HALFTONE, json!({"shape": "star"})).is_err());
        assert!(s.execute(HALFTONE, json!({"frequency": 0})).is_err());
        assert!(s.execute(HALFTONE, json!({"target": 3})).is_err());
        assert!(s.execute(REMOVE_SEMI_TRANSPARENT, json!({"threshold": 500})).is_err());
        assert_eq!(alphas(&s), before);
    }


    #[test]
    fn gang_sheet_places_copies_on_a_new_document() {
        let dir = std::env::temp_dir().join(format!("dtf-gang-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Two small images saved through the engine (300 ppi, 2 × 1 cm and 1 × 1 cm).
        let mut s = Session::new();
        for (name, w, h) in [("a", 236, 118), ("b", 118, 118)] {
            s.execute("file.new", json!({"width": w, "height": h, "resolution": 300, "background": "#ff0000"})).unwrap();
            s.execute("file.saveACopy", json!({"path": dir.join(format!("{name}.png")).to_string_lossy()})).unwrap();
        }
        let r = s
            .execute(GANG_SHEET, json!({"input": dir.to_string_lossy(), "filmWidth": 10, "sheetLength": 0, "copies": 3, "spacing": 2, "margin": 2}))
            .unwrap();
        assert_eq!(r["errors"], json!([]), "{r}");
        assert_eq!(r["sheets"].as_array().unwrap().len(), 1);
        assert_eq!(r["sheets"][0]["images"], 6);
        let d = s.active().unwrap();
        assert_eq!(d.doc.layers.len(), 6);
        assert_eq!(d.doc.size.width, 1181, "10 cm at 300 ppi");
        assert!((d.doc.resolution_dpi - 300.0).abs() < 0.01);
        std::fs::remove_dir_all(&dir).ok();
        assert!(s.execute(GANG_SHEET, json!({"input": []})).is_err());
        assert!(s.execute(GANG_SHEET, json!({"input": "/nonexistent/x", "filmWidth": 0})).is_err());
    }

    #[test]
    fn rotation_turns_pixels_clockwise() {
        // 2 × 1 image: [A B] → rotated 1 × 2: [A] over [B]? Clockwise: top = A, bottom = B.
        let data = vec![1.0, 2.0];
        assert_eq!(rotate_cw(&data, 2, 1, 1), vec![1.0, 2.0]);
        // 2 × 2: [1 2 / 3 4] → [3 1 / 4 2].
        assert_eq!(rotate_cw(&[1.0, 2.0, 3.0, 4.0], 2, 2, 1), vec![3.0, 1.0, 4.0, 2.0]);
    }


    #[test]
    fn ai_upscale_doubles_the_document_and_keeps_print_size() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 24, "height": 16, "resolution": 150, "background": "transparent"})).unwrap();
        s.edit("Dot", |doc, active| {
            let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
            for y in 4..12 {
                for x in 6..18 {
                    surf.write_pixel(x, y, &[0.9, 0.2, 0.1, 1.0]);
                }
            }
            Ok(())
        })
        .unwrap();
        let r = s.execute(UPSCALE, json!({"scale": "2"})).unwrap();
        assert_eq!((r["width"].as_u64(), r["height"].as_u64()), (Some(48), Some(32)), "{r}");
        let d = s.active().unwrap();
        assert_eq!((d.doc.size.width, d.doc.size.height), (48, 32));
        assert!((d.doc.resolution_dpi - 300.0).abs() < 0.01);
        let surf = d.doc.layer(d.active_layer.unwrap()).unwrap().surface().unwrap();
        let c = surf.pixel(24, 16);
        assert!(c[3] > 0.9 && c[0] > 0.7 && c[1] < 0.4, "the red block is enlarged: {c:?}");
        assert!(surf.pixel(2, 2)[3] < 0.05, "transparent corner stays clear");
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc.size.width, 24);
        assert!(s.execute(UPSCALE, json!({"scale": "5"})).is_err());
    }


    #[test]
    fn workflow_windows_run_their_steps() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 300, "height": 200, "resolution": 72, "background": "white"})).unwrap();
        let r = s.execute(CONFORMITY, json!({"height": 10, "resolution": 300, "trim": false})).unwrap();
        assert_eq!(r["height"], 1181, "10 cm at 300 ppi: {r}");
        let d = s.active().unwrap();
        assert!(!d.doc.is_background(d.doc.layers[0].id));
        assert_eq!(d.doc.layers[0].name, "origin");
        s.execute(CONTROL_BACKGROUND, json!({"color": "#102030"})).unwrap();
        let d = s.active().unwrap();
        assert_eq!(d.doc.layers.len(), 2);
        assert_eq!(d.doc.layers[0].name, BACKFILL, "the fill is at the bottom");
        assert_eq!(d.doc.layer(d.active_layer.unwrap()).unwrap().name, "origin", "the artwork stays targeted");
        s.execute(SATURATION, json!({"saturation": 20})).unwrap();
        s.execute(SHARPEN, json!({})).unwrap();
        s.execute(DEFRINGE, json!({"width": 2})).unwrap();
        let r = s.execute(REMOVE_CONTROL_BACKGROUND, json!({})).unwrap();
        assert_eq!(r["deleted"], 1);
        assert!(s.execute(REMOVE_CONTROL_BACKGROUND, json!({})).is_err());
        assert!(s.execute(CONTROL_BACKGROUND, json!({"color": "red"})).is_err());
        assert!(s.execute(CONFORMITY, json!({"depth": "32"})).is_err());
    }

    #[test]
    fn disabled_without_a_document() {
        let s = Session::new();
        assert!(!s.is_enabled(HALFTONE));
        assert!(!s.is_enabled(REMOVE_SEMI_TRANSPARENT));
    }
}
