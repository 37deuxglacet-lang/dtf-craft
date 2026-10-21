// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF menu (DTF Craft): preparation commands for direct-to-film printing.
//!
//! * `filter.dtf.halftone`: DTF Halftone (see [`photocraft_algo::dtf`]): the screen is given in
//!   lines per inch and converted to a cell size with the document resolution.
//! * `filter.dtf.removeSemiTransparent`: alpha threshold, every pixel becomes opaque or clear.
//! * `filter.dtf.removeWhiteBackground`: the plain (white) background removed from the edges
//!   inwards on a copy of the layer, the contour unmixed from it (see [`photocraft_algo::dtf`]).
//!
//! Both run through [`crate::filters::run_filter`], so selections, smart objects (as smart
//! filters), history and the schema dialog with live preview work as for every other filter.

use photocraft_algo::dtf::{CutoutSpec, HalftoneSpec, HalftoneTarget};
use photocraft_algo::FilterParams;
use photocraft_algo::quantize::HalftoneShape;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

pub const HALFTONE: &str = "filter.dtf.halftone";
pub const REMOVE_SEMI_TRANSPARENT: &str = "filter.dtf.removeSemiTransparent";
pub const REMOVE_WHITE: &str = "filter.dtf.removeWhiteBackground";
/// Name of the copy Remove White Background cuts out (the source layer stays, hidden).
pub const CUTOUT_LAYER: &str = "Sans fond";

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
                target: HalftoneTarget::from_id(p.get("mode").and_then(Value::as_str).unwrap_or_else(|| text(p, "target", "semiTransparent")))
                    .unwrap_or(HalftoneTarget::Alpha),
                min_dot: (num(p, "minDot", 5.0).clamp(0.0, 100.0) / 100.0) as f32,
                max_dot: (num(p, "maxDot", 95.0).clamp(0.0, 100.0) / 100.0) as f32,
                low: (num(p, "inBlack", 0.0).clamp(0.0, 255.0) / 255.0) as f32,
                high: (num(p, "inWhite", 255.0).clamp(0.0, 255.0) / 255.0) as f32,
                gamma: num(p, "gamma", 1.0).clamp(0.1, 9.99) as f32,
            };
            Some(FilterParams::DtfHalftone { spec })
        }
        REMOVE_SEMI_TRANSPARENT => Some(FilterParams::DtfAlphaThreshold { threshold: (num(p, "threshold", 62.0).clamp(1.0, 100.0) / 100.0) as f32 }),
        REMOVE_WHITE => {
            let detect = p.get("detectColor").and_then(Value::as_bool).unwrap_or(true);
            let color = if detect { None } else { Some(hex_rgb(text(p, "color", "#ffffff")).unwrap_or([1.0; 3])) };
            let spec = CutoutSpec {
                color,
                tolerance: (num(p, "tolerance", 10.0).clamp(0.0, 100.0) / 100.0) as f32,
                edge: num(p, "contour", 2.0).clamp(0.0, 10.0) as f32,
                decontaminate: p.get("removeHalo").and_then(Value::as_bool).unwrap_or(true),
                holes: p.get("interiorHoles").and_then(Value::as_bool).unwrap_or(false),
                min_hole: num(p, "minHoleSize", 100.0).clamp(1.0, 1_000_000.0) as u32,
            };
            Some(FilterParams::DtfCutout { spec })
        }
        _ => None,
    }
}

/// `#rrggbb` as straight sRGB 0–1.
fn hex_rgb(hex: &str) -> Option<[f32; 3]> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 || !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|b| f32::from(b) / 255.0);
    Some([v(0)?, v(2)?, v(4)?])
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

/// Names of what DTF Halftone builds: a group holding the screened copy over a control fill, the
/// source layer kept intact (hidden) underneath.
pub const HALFTONE_GROUP: &str = "Halftone DTF";
pub const HALFTONE_LAYER: &str = "Halftone";
pub const HALFTONE_BACKGROUND: &str = "Fond de contrôle";
/// The control fill under the screened copy, by mode: none for the semi-transparent pixels (the
/// film's own transparency), the dark control colour for a dark garment, white for a light one.
pub fn halftone_background(mode: &str) -> Option<&'static str> {
    match mode {
        "darkGarment" => Some("#231c1c"),
        "lightGarment" => Some("#ffffff"),
        _ => None,
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
    check_choice(HALFTONE, p, "mode", &["semiTransparent", "darkGarment", "lightGarment"])?;
    // `target` is the first version's name for the mode ("alpha" = semi-transparent only).
    check_choice(HALFTONE, p, "target", &["alpha", "semiTransparent", "darkGarment", "lightGarment"])?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let dpi = f64::from(st.doc.resolution_dpi).max(1.0);
    let source = st.active_layer.ok_or_else(|| EngineError::Other("select the layer to screen".into()))?;
    match st.doc.layer(source).map(|l| &l.content) {
        Some(photocraft_doc::LayerContent::Raster(_) | photocraft_doc::LayerContent::Smart(_)) => {}
        _ => return Err(EngineError::Other("DTF Halftone screens a pixel layer: select the visual's layer".into())),
    }
    let lpi = num(p, "frequency", 25.0).clamp(1.0, 300.0);
    let mut p2 = if p.is_object() { p.clone() } else { json!({}) };
    if let Value::Object(m) = &mut p2 {
        let mode = match (m.remove("mode"), m.remove("target")) {
            (Some(Value::String(mode)), _) => mode,
            (_, Some(Value::String(t))) if t != "alpha" => t,
            _ => "semiTransparent".into(),
        };
        m.insert("mode".into(), json!(mode));
        m.insert("__cellPx".into(), json!(dpi / lpi));
    }
    let background = halftone_background(p2.get("mode").and_then(Value::as_str).unwrap_or("semiTransparent"));
    // The layers first, as one step: a copy of the visual named "Halftone" (over a control fill
    // for a garment), in the "Halftone DTF" group above the source, hidden but left untouched.
    let screened = s.keeping_background(|s| crate::analysis_cmds::compound(s, HALFTONE_GROUP, |s| {
        s.execute("layer.duplicate", json!({"layer": source.0, "name": HALFTONE_LAYER}))?;
        let copy = s.active().and_then(|d| d.active_layer).ok_or(EngineError::NoDocument)?;
        if let Some(color) = background {
            s.execute("layer.newFillLayer.solidColor", json!({"color": color}))?;
            let fill = s.active().and_then(|d| d.active_layer).ok_or(EngineError::NoDocument)?;
            // A selection would become the fill's mask: the control background covers the canvas.
            if s.active().and_then(|d| d.doc.layer(fill)).is_some_and(|l| l.mask.is_some()) {
                s.execute("layer.layerMask.delete", json!({}))?;
            }
            s.execute("layer.setProps", json!({"layer": fill.0, "name": HALFTONE_BACKGROUND}))?;
            s.execute("layer.arrange.sendBackward", json!({"layer": fill.0}))?;
            s.execute("layer.select", json!({"layer": copy.0}))?;
            s.execute("layer.select", json!({"layer": fill.0, "mode": "add"}))?;
        } else {
            s.execute("layer.select", json!({"layer": copy.0}))?;
        }
        s.execute("layer.groupLayers", json!({"name": HALFTONE_GROUP}))?;
        s.execute("layer.setProps", json!({"layer": source.0, "visible": false}))?;
        s.execute("layer.select", json!({"layer": copy.0}))?;
        Ok(copy)
    }))?;
    // Then the screen on the copy (a background job in the app, like every filter).
    if let Value::Object(m) = &mut p2 {
        m.insert("layer".into(), json!(screened.0));
    }
    crate::filters::run_filter(s, HALFTONE, &p2)
}

fn remove_semi(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(REMOVE_SEMI_TRANSPARENT, p, "threshold", 1.0, 100.0)?;
    crate::filters::run_filter(s, REMOVE_SEMI_TRANSPARENT, p)
}

/// Remove White Background: the source layer is copied (the copy "Sans fond" above it, the
/// source hidden and untouched), then the background is removed from the copy's pixels.
fn remove_white(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(REMOVE_WHITE, p, "tolerance", 0.0, 100.0)?;
    check_range(REMOVE_WHITE, p, "contour", 0.0, 10.0)?;
    check_range(REMOVE_WHITE, p, "minHoleSize", 1.0, 1_000_000.0)?;
    if let Some(c) = p.get("color").filter(|v| !v.is_null())
        && c.as_str().and_then(hex_rgb).is_none()
    {
        return Err(bad(REMOVE_WHITE, format!("`color` must be #rrggbb (got {c})")));
    }
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let source = st.active_layer.ok_or_else(|| EngineError::Other("select the layer to cut out".into()))?;
    match st.doc.layer(source).map(|l| &l.content) {
        Some(photocraft_doc::LayerContent::Raster(_) | photocraft_doc::LayerContent::Smart(_)) => {}
        _ => return Err(EngineError::Other("Remove White Background works on a pixel layer: select the visual's layer".into())),
    }
    let copy = s.keeping_background(|s| crate::analysis_cmds::compound(s, "Remove White Background", |s| {
        s.execute("layer.duplicate", json!({"layer": source.0, "name": CUTOUT_LAYER}))?;
        let copy = s.active().and_then(|d| d.active_layer).ok_or(EngineError::NoDocument)?;
        s.execute("layer.setProps", json!({"layer": source.0, "visible": false}))?;
        s.execute("layer.select", json!({"layer": copy.0}))?;
        Ok(copy)
    }))?;
    let mut p2 = if p.is_object() { p.clone() } else { json!({}) };
    if let Value::Object(m) = &mut p2 {
        m.insert("layer".into(), json!(copy.0));
    }
    crate::filters::run_filter(s, REMOVE_WHITE, &p2)
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

/// The upscaled copy's name ("BOISERIE x2" for BOISERIE.png at ×2) and its Save As hint: the
/// original's folder and extension under that name, when the original has a path.
fn upscale_copy_name(doc_name: &str, path: Option<&str>, scale: f64) -> (String, Option<String>) {
    let stem = match std::path::Path::new(doc_name).extension().and_then(|e| e.to_str()) {
        Some(ext) if photocraft_codecs::from_extension(&ext.to_ascii_lowercase()).is_some() || ext.eq_ignore_ascii_case("psd") => {
            std::path::Path::new(doc_name).file_stem().and_then(|s| s.to_str()).unwrap_or(doc_name).to_string()
        }
        _ => doc_name.to_string(),
    };
    let name = format!("{stem} x{}", scale.round() as u32);
    let hint = path.map(|p| {
        let p = std::path::Path::new(p);
        let file = match p.extension().and_then(|e| e.to_str()) {
            Some(ext) => format!("{name}.{ext}"),
            None => name.clone(),
        };
        p.with_file_name(file).to_string_lossy().into_owned()
    });
    (name, hint)
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
    // A resolution given sets the copy's ppi (Print Conformity's ×2/×3/×4 buttons): no resampling.
    check_range(UPSCALE, p, "resolution", 72.0, 1200.0)?;
    let resolution = p.get("resolution").and_then(Value::as_f64);
    let model = photocraft_algo::upscale::SrModel::bundled().map_err(|e| EngineError::Other(format!("AI model: {e}")))?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    if !matches!(st.doc.mode, photocraft_color::ColorMode::Rgb | photocraft_color::ColorMode::Grayscale) {
        return Err(bad(UPSCALE, "AI upscaling works on RGB and Grayscale documents"));
    }
    let (ow, oh) = (st.doc.size.width, st.doc.size.height);
    let (nw, nh) = ((f64::from(ow) * scale).round() as u32, (f64::from(oh) * scale).round() as u32);
    crate::image_cmds::check_resample_budget(UPSCALE, &st.doc, nw, nh, "")?;
    // The original is never touched: the network enlarges a copy named "<name> x2" (x3, x4),
    // whose Save As starts in the original's folder and format.
    let (name, hint) = upscale_copy_name(&st.doc.name, st.path.as_deref(), scale);
    s.keeping_background(|s| s.execute("image.duplicate", json!({"name": name})))?;
    if let Some(copy) = s.active_mut() {
        copy.save_hint = hint;
    }
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
            if let Some(dpi) = resolution {
                doc.resolution_dpi = dpi as f32;
            } else if keep_print {
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
/// Clicks an interior hole into the cut-out (Remove White Background tab's magic wand).
pub const REMOVE_HOLE: &str = "dtf.removeHole";
/// The transparent margin, in px, Print Conformity leaves around a trimmed visual.
pub const TRIM_MARGIN: u32 = 2;

/// Name of the control background fill layer (as the BRODERIE Photoshop actions call it).
const BACKFILL: &str = "backfill";

/// Print conformity: the artwork layer renamed, the document resized to a print height at a
/// resolution, set to 8 or 16 bits and trimmed to its visible pixels.
/// The pixel size for a print of `width_cm` × `height_cm` (0 = follows the other side) at `dpi`,
/// keeping the proportions of `w` × `h`: with both sides, the largest size that fits inside them.
/// `None` when neither side is given (the pixels stay, only the resolution changes).
pub fn conformity_size(w: u32, h: u32, width_cm: f64, height_cm: f64, dpi: f64) -> Option<(u32, u32)> {
    let (w, h) = (f64::from(w.max(1)), f64::from(h.max(1)));
    let to_px = |cm: f64| cm / 2.54 * dpi;
    let k = match (width_cm > 0.0, height_cm > 0.0) {
        (false, false) => return None,
        (true, false) => to_px(width_cm) / w,
        (false, true) => to_px(height_cm) / h,
        (true, true) => (to_px(width_cm) / w).min(to_px(height_cm) / h),
    };
    Some(((w * k).round().max(1.0) as u32, (h * k).round().max(1.0) as u32))
}

fn conformity(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(CONFORMITY, p, "width", 0.0, 500.0)?;
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
    // Transparent borders go first, so the width and height given are the visual's own.
    let trim = p.get("trim").and_then(Value::as_bool).unwrap_or(true);
    if trim {
        s.execute("image.trim", json!({"basedOn": "transparent"}))?;
    }
    let dpi = num(p, "resolution", 300.0);
    let resample = text(p, "resample", "bicubicAutomatic");
    let d = &s.active().ok_or(EngineError::NoDocument)?.doc;
    match conformity_size(d.size.width, d.size.height, num(p, "width", 0.0), num(p, "height", 27.0), dpi) {
        Some((w, h)) => s.execute("image.imageSize", json!({"width": w, "height": h, "resolution": dpi, "resample": resample}))?,
        None => s.execute("image.imageSize", json!({"resolution": dpi, "resample": "none"}))?,
    };
    // A trimmed visual keeps a thin transparent margin: its edge never touches the canvas edge.
    if trim {
        let m = f64::from(2 * TRIM_MARGIN);
        s.execute("image.canvasSize", json!({"width": m, "height": m, "relative": true, "anchor": "center", "extensionColor": "transparent"}))?;
    }
    s.execute(if text(p, "depth", "8") == "16" { "image.mode.bits16" } else { "image.mode.bits8" }, json!({}))?;
    let d = &s.active().ok_or(EngineError::NoDocument)?.doc;
    Ok(json!({"width": d.size.width, "height": d.size.height, "resolution": d.resolution_dpi}))
}

/// A solid colour fill named "backfill" under everything, to judge the artwork on the garment colour.
/// It replaces a control background already there (a new colour is one undo step).
fn control_background(s: &mut Session, p: &Value) -> Result<Value> {
    let color = text(p, "color", "#231c1c").to_string();
    if !(color.len() == 7 && color.starts_with('#') && color[1..].chars().all(|c| c.is_ascii_hexdigit())) {
        return Err(bad(CONTROL_BACKGROUND, "`color` must be #rrggbb"));
    }
    let previous = s.active().and_then(|d| d.active_layer);
    crate::analysis_cmds::compound(s, "Control Background", |s| {
        let old: Vec<u64> = s.active().ok_or(EngineError::NoDocument)?.doc.walk().into_iter().filter(|(_, _, l)| l.name == BACKFILL).map(|(_, _, l)| l.id.0).collect();
        for id in old {
            s.execute("layer.delete", json!({"layer": id}))?;
        }
        // Nothing goes under a Background layer (a JPEG's picture): it becomes a normal layer first.
        let doc = &s.active().ok_or(EngineError::NoDocument)?.doc;
        if let Some(bg) = doc.layers.first().map(|l| l.id).filter(|id| doc.is_background(*id)) {
            s.execute("layer.select", json!({"layer": bg.0}))?;
            s.execute("layer.new.layerFromBackground", json!({}))?;
        }
        s.execute("layer.newFillLayer.solidColor", json!({"color": color}))?;
        let fill = s.active().and_then(|d| d.active_layer).ok_or(EngineError::NoDocument)?;
        // A selection would become the fill's mask: the control background covers the canvas.
        if s.active().and_then(|d| d.doc.layer(fill)).is_some_and(|l| l.mask.is_some()) {
            s.execute("layer.layerMask.delete", json!({}))?;
        }
        s.execute("layer.setProps", json!({"name": BACKFILL}))?;
        // Already the bottom layer (a document of one layer, selected at the bottom): stays there.
        let bottom = s.active().and_then(|d| d.doc.layers.first().map(|l| l.id)) == Some(fill);
        if !bottom {
            s.execute("layer.arrange.sendToBack", json!({}))?;
        }
        if let Some(l) = previous.filter(|l| s.active().is_some_and(|d| d.doc.layer(*l).is_some())) {
            s.execute("layer.select", json!({"layer": l.0}))?;
        }
        Ok(Value::Null)
    })
}

/// One interior hole in the cut-out: the area the magic wand picks at (x, y) on the active layer
/// becomes transparent, in one undo step; the selection is left as it was.
fn remove_hole(s: &mut Session, p: &Value) -> Result<Value> {
    check_range(REMOVE_HOLE, p, "tolerance", 0.0, 255.0)?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let layer = st.active_layer.ok_or_else(|| EngineError::Other("select the cut-out layer".into()))?;
    if !matches!(st.doc.layer(layer).map(|l| &l.content), Some(photocraft_doc::LayerContent::Raster(_))) {
        return Err(EngineError::Other("the active layer has no pixels to cut".into()));
    }
    let kept = st.doc.selection.clone();
    let (x, y) = (num(p, "x", 0.0).floor(), num(p, "y", 0.0).floor());
    crate::analysis_cmds::compound(s, "Interior Hole", |s| {
        let picked = s.execute(
            "select.magicWand",
            json!({"x": x, "y": y, "tolerance": num(p, "tolerance", 26.0), "contiguous": true, "antiAlias": true, "sampleAllLayers": false, "mode": "replace"}),
        )?;
        if picked.get("selected").and_then(Value::as_bool) != Some(true) {
            return Err(EngineError::Other("no area at this point".into()));
        }
        s.execute("edit.clear", json!({}))?;
        s.edit("Interior Hole", |doc, _| {
            doc.selection = kept;
            Ok(())
        })?;
        Ok(json!({"x": x, "y": y}))
    })
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
            params: r##"{"width":0..500=0 (cm; proportional when 0),"height":0..500=27 (cm; proportional when 0; both 0 keep the size),"resolution":72..1200=300,"resample":"bicubicAutomatic|preserveDetails|bicubic|bicubicSmoother|bicubicSharper|lanczos|bilinear|nearest"="bicubicAutomatic","depth":"8|16"="8","trim":bool=true (crop the transparency, leaving 2 px around the visual),"layerName":text="origin"}"##,
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
            // Run from Print Conformity's ×2/×3/×4 buttons, not the DTF menu.
            menu: &[],
            shortcut: None,
            params: r##"{"scale":"2|3|4"="2","alpha":"model|smooth"="model" (edges of the transparency through the network, or smoothly enlarged),"keepPrintSize":bool=true (raise the resolution so the print size stays),"resolution":json (optional ppi of the copy, 72–1200; overrides keepPrintSize)} → {width,height,layers,resolution}. Real-ESRGAN general ×4 model on the CPU; raster layers are enlarged by the network, other layers resampled"##,
            enabled: has_doc,
            run: upscale,
            journal: true,
        },
        CommandSpec {
            id: HALFTONE,
            label: "DTF Halftone…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"mode":"semiTransparent|darkGarment|lightGarment"="semiTransparent" (semi-transparent pixels only, or the whole visual by its lightness for a dark or light garment),"frequency":1..300=25,"angle":-180..180=22.5,"shape":"round|ellipse|line|square|diamond|cross"="round","minDot":0..100=5,"maxDot":0..100=95,"inBlack":0..255=0,"inWhite":0..255=255,"gamma":0.1..9.99=1} (frequency in lines/inch at the document resolution; alpha becomes 0 or 1, colours are kept). Builds a "Halftone DTF" group above the layer: the screened copy "Halftone", over a "Fond de contrôle" fill for a garment (#231c1c dark, #ffffff light; none for the semi-transparent pixels); the source layer is hidden, unchanged"##,
            enabled: crate::filters::has_filterable_layer,
            run: halftone,
            journal: true,
        },
        CommandSpec {
            id: REMOVE_WHITE,
            label: "Remove White Background…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"detectColor":bool=true (read the background colour on the image edges),"color":color=#ffffff (the background colour when it is not read),"tolerance":0..100=10 (% difference still taken as background; absorbs JPEG noise),"contour":0..10=2 (px of contour unmixed from the background),"removeHalo":bool=true (unmix the contour so no light halo is left),"interiorHoles":bool=false (also remove the background colour enclosed by the visual),"minHoleSize":1..1000000=100 (px; smaller enclosed areas stay)} Removes the plain background from the image edges inwards on a copy of the layer named "Sans fond"; the source layer is hidden, unchanged"##,
            enabled: crate::filters::has_filterable_layer,
            run: remove_white,
            journal: true,
        },
        CommandSpec {
            id: REMOVE_HOLE,
            label: "Interior Hole",
            menu: &[],
            shortcut: None,
            params: r##"{"x":px,"y":px,"tolerance":0..255=26} (the magic wand's contiguous area at x, y on the active layer becomes transparent; the selection is kept)"##,
            enabled: crate::filters::has_filterable_layer,
            run: remove_hole,
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
    fn halftone_builds_its_group_and_keeps_the_source() {
        let mut s = session_with_ramp();
        let source = s.active().unwrap().active_layer.unwrap();
        let before = alphas(&s);
        s.execute(HALFTONE, json!({"mode": "semiTransparent", "frequency": 30})).unwrap();
        let d = s.active().unwrap();
        let names: Vec<String> = d.doc.walk().into_iter().map(|(_, _, l)| l.name.clone()).collect();
        for want in [HALFTONE_GROUP, HALFTONE_LAYER] {
            assert!(names.iter().any(|n| n == want), "{want} in {names:?}");
        }
        assert!(!names.iter().any(|n| n == HALFTONE_BACKGROUND), "no control fill for the semi-transparent pixels: {names:?}");
        let src = d.doc.layer(source).unwrap();
        assert!(!src.visible, "the source is hidden");
        assert_eq!(d.doc.layer(d.active_layer.unwrap()).unwrap().name, HALFTONE_LAYER);
        // The screened copy is binary; the source keeps its soft alpha.
        assert!(alphas(&s).iter().all(|v| *v == 0.0 || *v == 1.0));
        let surf = d.doc.layer(source).unwrap().surface().unwrap();
        assert!((surf.pixel(100, 5)[3] - before[5 * 200 + 100]).abs() < 1e-6);
        // Opaque areas stay solid in semi-transparent mode.
        assert_eq!(alphas(&s)[5 * 200 + 199], 1.0);
        // The structure step and the screen undo back to the original.
        assert!(s.undo() && s.undo());
        assert_eq!(s.active().unwrap().doc.layers.len(), 1);
        assert!(s.active().unwrap().doc.layers[0].visible);
    }

    #[test]
    fn halftone_control_fill_follows_the_garment() {
        for (mode, want) in [("darkGarment", Some("#231c1c")), ("lightGarment", Some("#ffffff")), ("semiTransparent", None)] {
            let mut s = session_with_ramp();
            s.execute(HALFTONE, json!({"mode": mode})).unwrap();
            let d = s.active().unwrap();
            let fill = d.doc.walk().into_iter().find(|(_, _, l)| l.name == HALFTONE_BACKGROUND).map(|(_, _, l)| l.clone());
            assert_eq!(fill.is_some(), want.is_some(), "{mode}");
            if let (Some(l), Some(hex)) = (fill, want) {
                let photocraft_doc::LayerContent::Fill(photocraft_doc::Fill::Solid(c)) = &l.content else { panic!("{mode}: not a solid fill") };
                let rgb = [0, 1, 2].map(|i| (c.c[i] * 255.0).round() as u8);
                assert_eq!(format!("#{:02x}{:02x}{:02x}", rgb[0], rgb[1], rgb[2]), hex, "{mode}");
            }
        }
    }

    /// A white 30 × 20 picture (as a JPEG opens: a Background layer) with a black square.
    fn session_with_logo() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 30, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        s.edit("Logo", |doc, active| {
            let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
            for y in 5..15 {
                for x in 10..20 {
                    let c = if (13..17).contains(&x) && (8..12).contains(&y) { 1.0 } else { 0.0 };
                    surf.write_pixel(x, y, &[c, c, c, 1.0]);
                }
            }
            Ok(())
        })
        .unwrap();
        s
    }

    #[test]
    fn remove_white_cuts_out_a_copy_and_keeps_the_source() {
        let mut s = session_with_logo();
        let source = s.active().unwrap().active_layer.unwrap();
        s.execute(REMOVE_WHITE, json!({})).unwrap();
        let d = s.active().unwrap();
        let copy = d.doc.layer(d.active_layer.unwrap()).unwrap();
        assert_eq!(copy.name, CUTOUT_LAYER);
        let surf = copy.surface().unwrap();
        assert_eq!(surf.pixel(0, 0)[3], 0.0, "the white around is removed");
        assert_eq!(surf.pixel(11, 6)[3], 1.0, "the visual stays");
        assert_eq!(surf.pixel(15, 10)[3], 1.0, "the white inside stays (interior holes off)");
        let src = d.doc.layer(source).unwrap();
        assert!(!src.visible, "the source is hidden");
        assert_eq!(src.surface().unwrap().pixel(0, 0)[3], 1.0, "and untouched");
        // With interior holes, the white square inside goes too.
        let mut s = session_with_logo();
        s.execute(REMOVE_WHITE, json!({"interiorHoles": true, "minHoleSize": 4})).unwrap();
        let d = s.active().unwrap();
        assert_eq!(d.doc.layer(d.active_layer.unwrap()).unwrap().surface().unwrap().pixel(15, 10)[3], 0.0);
        // The copy and the cut-out undo back to the picture alone.
        assert!(s.undo() && s.undo());
        assert_eq!(s.active().unwrap().doc.layers.len(), 1);
        assert!(s.active().unwrap().doc.layers[0].visible);
        assert!(s.execute(REMOVE_WHITE, json!({"color": "white"})).is_err());
        assert!(s.execute(REMOVE_WHITE, json!({"tolerance": 300})).is_err());
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





    /// A resolution given (Print Conformity's buttons) sets the copy's ppi, whatever keepPrintSize.
    #[test]
    fn ai_upscale_takes_the_resolution_given() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 24, "height": 16, "resolution": 72, "background": "white"})).unwrap();
        s.execute(UPSCALE, json!({"scale": "3", "resolution": 300})).unwrap();
        let d = s.active().unwrap();
        assert_eq!((d.doc.size.width, d.doc.size.height), (72, 48));
        assert!((d.doc.resolution_dpi - 300.0).abs() < 0.01);
        assert!(s.execute(UPSCALE, json!({"scale": "2", "resolution": 5000})).is_err());
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
        // The upscale runs on a copy named "<name> x2"; the original keeps its size.
        assert_eq!(s.documents().len(), 2);
        assert!(d.doc.name.ends_with(" x2"), "{}", d.doc.name);
        assert_eq!(s.documents()[0].doc.size.width, 24);
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc.size.width, 24);
        assert!(s.execute(UPSCALE, json!({"scale": "5"})).is_err());
    }


    /// Started as in the app (`Session::start`), the enlargement and the screen run as background
    /// jobs after their copy and layers, so the progress dialog shows and the window stays live.
    #[test]
    fn upscale_and_halftone_run_as_background_jobs() {
        use crate::jobs::Started;
        let wait = |s: &mut Session| {
            let t = std::time::Instant::now();
            while !s.jobs().is_empty() {
                s.poll_jobs();
                assert!(t.elapsed().as_secs() < 120, "the job did not finish");
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 24, "height": 16, "resolution": 72, "background": "white"})).unwrap();
        let r = s.start(UPSCALE, json!({"scale": "2"})).unwrap();
        assert!(matches!(r, Started::Job(_)), "AI Upscale ran inline");
        assert_eq!(s.documents().len(), 2);
        let copy = s.active().unwrap().doc.id;
        assert!(s.job_on(copy).is_some(), "the job runs on the copy");
        wait(&mut s);
        assert_eq!(s.active().unwrap().doc.size.width, 48);
        let mut s = session_with_ramp();
        let r = s.start(HALFTONE, json!({"frequency": 30})).unwrap();
        assert!(matches!(r, Started::Job(_)), "DTF Halftone ran inline");
        wait(&mut s);
        assert!(alphas(&s).iter().all(|v| *v == 0.0 || *v == 1.0));
    }

    #[test]
    fn control_background_goes_under_a_background_layer_picture() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 30, "height": 20, "background": "white"})).unwrap();
        s.execute(CONTROL_BACKGROUND, json!({})).unwrap();
        let d = s.active().unwrap();
        assert_eq!(d.doc.layers.len(), 2);
        assert_eq!(d.doc.layers[0].name, BACKFILL, "the fill is at the bottom");
        assert!(!d.doc.is_background(d.doc.layers[1].id));
        assert!(s.undo());
        assert_eq!(s.active().unwrap().doc.layers.len(), 1);
    }

    /// After Print Conformity (2 px of transparency around the visual), Remove White Background
    /// still reads the background colour and removes it.
    #[test]
    fn the_cutout_works_after_conformity() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 60, "height": 40, "resolution": 300, "background": "white"})).unwrap();
        s.execute("select.rect", json!({"x": 20, "y": 10, "width": 20, "height": 20})).unwrap();
        s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
        s.execute("select.deselect", json!({})).unwrap();
        s.execute(CONFORMITY, json!({"width": 0, "height": 0, "resolution": 300})).unwrap();
        s.execute(REMOVE_WHITE, json!({})).unwrap();
        let d = s.active().unwrap();
        let surf = d.doc.layer(d.active_layer.unwrap()).unwrap().surface().unwrap();
        assert_eq!(surf.pixel(5, 5)[3], 0.0, "the white background is removed");
        assert_eq!(surf.pixel(30, 20)[3], 1.0, "the visual stays");
    }

    /// A trimmed visual gets 2 px of transparency on every side, after the resize.
    #[test]
    fn conformity_leaves_a_transparent_margin_around_a_trimmed_visual() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 100, "height": 100, "resolution": 300, "background": "transparent"})).unwrap();
        s.execute("select.rect", json!({"x": 20, "y": 30, "width": 40, "height": 20})).unwrap();
        s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
        s.execute("select.deselect", json!({})).unwrap();
        // 40 px wide at 300 ppi stays 40 px: 40 / 300 * 2.54 cm.
        s.execute(CONFORMITY, json!({"width": 40.0 / 300.0 * 2.54, "height": 0, "resolution": 300})).unwrap();
        let d = s.active().unwrap();
        assert_eq!((d.doc.size.width, d.doc.size.height), (44, 24));
        let px = photocraft_compose::flatten(&d.doc);
        let alpha = |x: usize, y: usize| px.px[y * 44 + x][3];
        assert_eq!((alpha(0, 0), alpha(1, 12), alpha(2, 12), alpha(41, 12), alpha(42, 12)), (0.0, 0.0, 1.0, 1.0, 0.0));
        assert_eq!(d.doc.layers[0].name, "origin");
    }

    #[test]
    fn a_click_with_the_hole_wand_clears_one_enclosed_area_and_keeps_the_selection() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 30, "height": 30, "resolution": 300, "background": "transparent"})).unwrap();
        s.execute("select.rect", json!({"x": 0, "y": 0, "width": 30, "height": 30})).unwrap();
        s.execute("edit.fill", json!({"color": "#ff0000"})).unwrap();
        s.execute("select.rect", json!({"x": 10, "y": 10, "width": 10, "height": 10})).unwrap();
        s.execute("edit.fill", json!({"color": "#ffffff"})).unwrap();
        s.execute("select.rect", json!({"x": 0, "y": 0, "width": 5, "height": 5})).unwrap();
        s.execute(REMOVE_HOLE, json!({"x": 15, "y": 15})).unwrap();
        let d = s.active().unwrap();
        let layer = d.doc.layer(d.active_layer.unwrap()).unwrap().surface().unwrap();
        assert_eq!(layer.pixel(15, 15)[3], 0.0, "the white square is a hole");
        assert_eq!(layer.pixel(2, 2)[3], 1.0, "the red frame stays");
        assert!(d.doc.selection.is_some(), "the selection is kept");
        assert!(s.undo());
        let d = s.active().unwrap();
        assert_eq!(d.doc.layer(d.active_layer.unwrap()).unwrap().surface().unwrap().pixel(15, 15)[3], 1.0, "one undo step");
    }

    /// A second control background (another colour) replaces the first: one "backfill" only.
    #[test]
    fn control_background_replaces_the_previous_one() {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 30, "height": 20, "background": "transparent"})).unwrap();
        s.execute(CONTROL_BACKGROUND, json!({"color": "#231c1c"})).unwrap();
        s.execute(CONTROL_BACKGROUND, json!({"color": "#ffffff"})).unwrap();
        let d = s.active().unwrap();
        let fills = d.doc.walk().into_iter().filter(|(_, _, l)| l.name == BACKFILL).count();
        assert_eq!((fills, d.doc.layers.len()), (1, 2));
        s.execute(REMOVE_CONTROL_BACKGROUND, json!({})).unwrap();
        assert_eq!(s.active().unwrap().doc.layers.len(), 1);
    }

    #[test]
    fn conformity_keeps_the_proportions() {
        // 200 × 100 px at 300 ppi: 10 cm wide is 1181 px, the height follows.
        assert_eq!(conformity_size(200, 100, 10.0, 0.0, 300.0), Some((1181, 591)));
        assert_eq!(conformity_size(200, 100, 0.0, 10.0, 300.0), Some((2362, 1181)));
        // Both sides: the largest size that fits inside 10 × 10 cm.
        assert_eq!(conformity_size(200, 100, 10.0, 10.0, 300.0), Some((1181, 591)));
        assert_eq!(conformity_size(100, 200, 10.0, 10.0, 300.0), Some((591, 1181)));
        assert_eq!(conformity_size(200, 100, 0.0, 0.0, 300.0), None);
    }

    #[test]
    fn upscale_copy_takes_the_scale_in_its_name_and_the_original_folder() {
        let (name, hint) = upscale_copy_name("BOISERIE.png", Some("/films/BOISERIE.png"), 3.0);
        assert_eq!(name, "BOISERIE x3");
        // Compared as paths: Windows joins the name with `\`.
        assert_eq!(hint.map(std::path::PathBuf::from), Some(std::path::Path::new("/films").join("BOISERIE x3.png")));
        assert_eq!(upscale_copy_name("Untitled-1", None, 2.0), ("Untitled-1 x2".to_string(), None));
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
