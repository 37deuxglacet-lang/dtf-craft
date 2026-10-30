// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! The Vectorisation module's second engine: VectorCraft's Image Trace (`vectorcraft-trace`,
//! copied from VectorCraft, MIT OR Apache-2.0), with all its settings ([`TraceParams`]: mode,
//! palette, paths, corners, noise, method, ignore colour, shapes, gradients, strokes…).
//!
//! Its result is turned into the module's own form ([`Traced`] and a palette with each colour's
//! share), so the palette table, the swatch book, the colours left out, the SVG, the preview and
//! the PDF sheet work the same with either engine. Paths are kept in VectorCraft's order (bottom
//! first, which matters when layers overlap); following paths of one colour painted the same way
//! are joined into one.

use kurbo::PathEl;
pub use vectorcraft_trace::{Method, Mode, PRESET_NAMES, Palette, TraceError, TraceParams, preset, presets};
use vectorcraft_trace::{Raster, TracedPath};

use crate::vectorize::{ColorPath, LinearGradient, Paint, PaletteColor, Traced, num};

/// A trace with more anchor points than this is refused (a photo at high fidelity can make
/// millions): too heavy to preview, open or print.
pub const MAX_ANCHORS: usize = 1_500_000;

/// The VectorCraft trace in the module's form.
#[derive(Clone, Debug, PartialEq)]
pub struct VcTraced {
    /// The colours used, largest share first.
    pub palette: Vec<PaletteColor>,
    /// Every path, bottom first.
    pub traced: Traced,
    /// Anchor points of the trace.
    pub anchors: usize,
}

/// Why VectorCraft traced nothing.
#[derive(Clone, Debug, PartialEq)]
pub enum VcError {
    /// Flat Logo mode on a visual that is not a flat-colour logo (this share of its solid areas
    /// match its flat colours, 0–1).
    NotFlat(f64),
    /// More anchor points than [`MAX_ANCHORS`].
    TooComplex,
    /// A colour written wrong (Ignore Color, Logo Colors).
    BadColor(String),
    /// Nothing visible to trace.
    Empty,
    Other(String),
}

impl std::fmt::Display for VcError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VcError::NotFlat(k) => write!(f, "not a flat-colour logo ({:.0} % of its solid areas are flat): use Color mode", k * 100.0),
            VcError::TooComplex => write!(f, "more than {MAX_ANCHORS} anchor points: raise Noise, lower Paths or Colors"),
            VcError::BadColor(c) => write!(f, "\"{c}\" is not a colour: write it as #rrggbb"),
            VcError::Empty => write!(f, "nothing visible to trace"),
            VcError::Other(e) => write!(f, "{e}"),
        }
    }
}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `params` from a preset (`preset`, VectorCraft's name), then the fields of `fields` (camelCase,
/// as [`TraceParams`] serialises): what the `dtf.vectorize` command takes.
pub fn params_from(preset_name: Option<&str>, fields: Option<&serde_json::Value>) -> Result<TraceParams, String> {
    let base = match preset_name {
        Some(n) => preset(n).ok_or_else(|| format!("unknown VectorCraft preset \"{n}\" (one of: {})", PRESET_NAMES.join(", ")))?,
        None => TraceParams::default(),
    };
    let Some(fields) = fields.filter(|v| !v.is_null()) else { return Ok(base) };
    let obj = fields.as_object().ok_or("`vectorcraft` must be an object of VectorCraft settings")?;
    let mut merged = serde_json::to_value(&base).map_err(|e| e.to_string())?;
    if let Some(m) = merged.as_object_mut() {
        for (k, v) in obj {
            m.insert(k.clone(), v.clone());
        }
    }
    serde_json::from_value(merged).map_err(|e| format!("VectorCraft settings: {e}"))
}

/// Traces `px` (`w` × `h`, straight RGBA, sRGB) with VectorCraft. With the Swatch Book palette,
/// `params.swatches` holds its colours.
pub fn trace(w: usize, h: usize, px: &[[f32; 4]], params: &TraceParams) -> Result<VcTraced, VcError> {
    if w == 0 || h == 0 || w.checked_mul(h) != Some(px.len()) {
        return Err(VcError::Other(format!("{w} × {h} px does not match {} pixels", px.len())));
    }
    if !px.iter().any(|q| q[3] >= 0.5) {
        return Err(VcError::Empty);
    }
    let rgba: Vec<u8> = px.iter().flat_map(|q| [to8(q[0]), to8(q[1]), to8(q[2]), to8(q[3])]).collect();
    let (Ok(wu), Ok(hu)) = (u32::try_from(w), u32::try_from(h)) else { return Err(VcError::Other("visual too large".into())) };
    let raster = Raster::new(wu, hu, rgba).map_err(|e| VcError::Other(e.to_string()))?;
    // The tracer is a dependency: a panic in it becomes an error, never a crash.
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| vectorcraft_trace::trace_within(&raster, params, MAX_ANCHORS)))
        .map_err(|_| VcError::Other("the VectorCraft tracer failed".into()))?
        .map_err(|e| match e {
            TraceError::NotFlat { fraction } => VcError::NotFlat(fraction),
            TraceError::TooComplex { .. } => VcError::TooComplex,
            TraceError::BadPalette(c) => VcError::BadColor(c),
            TraceError::Empty => VcError::Empty,
            other => VcError::Other(other.to_string()),
        })?;
    if result.paths.is_empty() {
        return Err(VcError::Empty);
    }
    Ok(convert(w, h, &result.paths))
}

/// The paths in the module's form: the palette (each colour's share of the traced pixels) and the
/// paths, following ones of one colour painted alike joined.
fn convert(w: usize, h: usize, paths: &[TracedPath]) -> VcTraced {
    // Colours by first use, with their pixels.
    let mut colours: Vec<([u8; 3], usize)> = Vec::new();
    for p in paths {
        match colours.iter_mut().find(|c| c.0 == p.color) {
            Some(c) => c.1 += p.pixels,
            None => colours.push((p.color, p.pixels)),
        }
    }
    let mut order: Vec<usize> = (0..colours.len()).collect();
    order.sort_by(|a, b| colours[*b].1.cmp(&colours[*a].1).then(a.cmp(b)));
    let total: usize = colours.iter().map(|c| c.1).sum();
    let palette: Vec<PaletteColor> =
        order.iter().map(|&i| PaletteColor { rgb: colours[i].0, share: colours[i].1 as f32 / total.max(1) as f32, pixels: colours[i].1 }).collect();
    let index_of = |rgb: [u8; 3]| palette.iter().position(|c| c.rgb == rgb).unwrap_or(0);
    let mut out: Vec<ColorPath> = Vec::new();
    let mut anchors = 0;
    for p in paths {
        let (d, points) = path_data(&p.path);
        if d.is_empty() {
            continue;
        }
        anchors += points;
        let paint = match (&p.stroke, &p.gradient) {
            (Some(width), _) => Paint::Stroke { width: *width, square_caps: p.square_caps },
            (None, Some(g)) => Paint::Gradient(LinearGradient { start: [g.start.x, g.start.y], end: [g.end.x, g.end.y], stops: g.stops.clone() }),
            (None, None) => Paint::Fill,
        };
        let color = index_of(p.color);
        // Joined to the path before when it is the same colour painted the same plain way.
        if let Some(last) = out.last_mut()
            && last.color == color
            && last.paint == paint
            && !matches!(paint, Paint::Gradient(_))
        {
            last.d.push(' ');
            last.d.push_str(&d);
            last.shapes += 1;
            last.points += points;
            continue;
        }
        out.push(ColorPath { color, d, shapes: 1, points, paint });
    }
    VcTraced { palette, traced: Traced { width: w, height: h, paths: out }, anchors }
}

/// SVG path data (absolute, two decimals) and its anchor points.
fn path_data(path: &vectorcraft_geom::PathData) -> (String, usize) {
    let mut d = String::new();
    let mut points = 0;
    let push = |d: &mut String, s: String| {
        if !d.is_empty() {
            d.push(' ');
        }
        d.push_str(&s);
    };
    for el in path.to_bezpath().elements() {
        match *el {
            PathEl::MoveTo(p) => push(&mut d, format!("M{} {}", num(p.x), num(p.y))),
            PathEl::LineTo(p) => {
                points += 1;
                push(&mut d, format!("L{} {}", num(p.x), num(p.y)));
            }
            PathEl::QuadTo(a, p) => {
                points += 1;
                push(&mut d, format!("Q{} {} {} {}", num(a.x), num(a.y), num(p.x), num(p.y)));
            }
            PathEl::CurveTo(a, b, p) => {
                points += 1;
                push(&mut d, format!("C{} {} {} {} {} {}", num(a.x), num(a.y), num(b.x), num(b.y), num(p.x), num(p.y)));
            }
            PathEl::ClosePath => d.push('Z'),
        }
    }
    (d, points)
}

/// `t` without the paths of the colours `excluded` (one flag per palette colour).
pub fn without(t: &Traced, excluded: &[bool]) -> Traced {
    Traced { width: t.width, height: t.height, paths: t.paths.iter().filter(|p| !excluded.get(p.color).copied().unwrap_or(false)).cloned().collect() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vectorize::{Style, SvgSize, svg};

    const BLUE: [f32; 4] = [31.0 / 255.0, 79.0 / 255.0, 181.0 / 255.0, 1.0];
    const ORANGE: [f32; 4] = [228.0 / 255.0, 87.0 / 255.0, 46.0 / 255.0, 1.0];
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    /// 100 × 80 on white: a blue disc and an orange square.
    fn logo() -> Vec<[f32; 4]> {
        let mut px = Vec::new();
        for y in 0..80 {
            for x in 0..100 {
                let disc = (x as f32 - 30.0).powi(2) + (y as f32 - 40.0).powi(2) < 400.0;
                let square = (60..90).contains(&x) && (25..55).contains(&y);
                px.push(if disc { BLUE } else if square { ORANGE } else { WHITE });
            }
        }
        px
    }

    #[test]
    fn a_preset_traces_the_colours_with_their_shares() {
        let p = params_from(Some("3 Colors"), None).unwrap();
        let r = trace(100, 80, &logo(), &p).unwrap();
        let hexes: Vec<String> = r.palette.iter().map(|c| crate::vectorize::hex(c.rgb)).collect();
        assert_eq!(hexes.len(), 3, "{hexes:?}");
        assert_eq!(hexes[0], "#FFFFFF", "white, the largest share, first: {hexes:?}");
        assert!((r.palette.iter().map(|c| c.share).sum::<f32>() - 1.0).abs() < 1e-3);
        let s = svg(&r.traced, &r.palette, SvgSize::Mm(10.0, 8.0), Style::Fill { seal: false });
        assert!(s.contains("width=\"10mm\"") && s.contains("viewBox=\"0 0 100 80\""), "{s}");
        // Ignore Color drops white; the others stay.
        let p = params_from(Some("3 Colors"), Some(&serde_json::json!({"ignoreWhite": true}))).unwrap();
        let r = trace(100, 80, &logo(), &p).unwrap();
        assert!(r.palette.iter().all(|c| c.rgb != [255, 255, 255]), "{:?}", r.palette);
        // A colour left out goes from the paths.
        let blue = r.palette.iter().position(|c| c.rgb[2] > 150).unwrap();
        let mut excluded = vec![false; r.palette.len()];
        excluded[blue] = true;
        assert!(without(&r.traced, &excluded).paths.iter().all(|p| p.color != blue));
    }

    #[test]
    fn settings_merge_over_a_preset_and_bad_ones_are_errors() {
        let p = params_from(Some("Flat Logo"), Some(&serde_json::json!({"noise": 3, "method": "overlapping"}))).unwrap();
        assert_eq!((p.mode, p.noise, p.method), (Mode::Logo, 3, Method::Overlapping));
        assert!(params_from(Some("Nope"), None).is_err());
        assert!(params_from(None, Some(&serde_json::json!({"mode": "rainbow"}))).is_err());
        assert!(params_from(None, Some(&serde_json::json!(3))).is_err());
        let bad = params_from(Some("6 Colors"), Some(&serde_json::json!({"ignoreWhite": true, "ignoreColor": "bleu"}))).unwrap();
        assert_eq!(trace(100, 80, &logo(), &bad), Err(VcError::BadColor("bleu".into())));
        assert_eq!(trace(2, 1, &[[0.0; 4]; 2], &TraceParams::default()), Err(VcError::Empty));
    }

    #[test]
    fn black_and_white_traces_the_dark_areas() {
        let p = params_from(Some("Black and White Logo"), None).unwrap();
        let r = trace(100, 80, &logo(), &p).unwrap();
        assert_eq!(r.palette.len(), 1, "white ignored: {:?}", r.palette);
        assert_eq!(r.palette[0].rgb, [0, 0, 0]);
        assert!(r.traced.paths.iter().all(|p| p.paint == Paint::Fill));
    }
}
