// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Mockup (DTF Craft, beta): visuals shown on a photo of the garment at their real size.
//!
//! The photo's scale comes from a line drawn on it whose real length is known (`scale`: its two
//! ends in photo pixels and its length in cm) or straight from `pixelsPerCm`. Each visual (a PNG
//! or SVG file, or an open document, flattened and cropped like a gang sheet's) is placed by its
//! centre on the photo, at its print size in cm (by default its own: pixels at its resolution),
//! turned by `rotation` degrees clockwise; or by its four `corners` on the photo (Edit › Transform's
//! Skew, Distort and Perspective), and bent by a `warp` (Edit › Transform › Warp: a preset of
//! [`STYLES`] with a bend, or a custom 4 × 4 control grid). See [`Placement`] and [`place`].
//!
//! The print follows the fabric ([`composite`]):
//! * **folds** move the ink along the photo's light (the gradient of its blurred luminance), up
//!   to [`MAX_FOLD_CM`] at 100;
//! * **shadows** carry the photo's light onto the ink: a pixel darker than the print area's median
//!   darkens it as much, a lighter one lightens it a little;
//! * **opacity** lets the fabric show through.
//!
//! The result is one document the size of the photo (opened by default) and, with `output`, a
//! PNG or JPG file. Only a preview of the print: the files to print are not changed.

use photocraft_algo::transform::Homography;
use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, Size};
use photocraft_geom::Rect;
use photocraft_geom::warp::{BezierMesh, StyleWarp, Warp, WarpStyle};
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::file_cmds;
use crate::gang_sheet_cmds::{self as gang, Visual};
use crate::{EngineError, Result, Session};

/// DTF › Mockup….
pub const MOCKUP: &str = "dtf.mockup";
/// The fold displacement at 100, in cm on the garment.
pub const MAX_FOLD_CM: f64 = 0.4;
/// Default render settings (0..100).
pub const DEFAULT_SHADOWS: f64 = 60.0;
pub const DEFAULT_FOLDS: f64 = 35.0;
pub const DEFAULT_OPACITY: f64 = 95.0;
/// How much lighter fabric lightens the ink (darker fabric darkens it fully), and at most.
const HIGHLIGHT: f32 = 0.35;
const MAX_LIGHTEN: f32 = 1.25;
/// Largest side of a visual on the photo, in pixels.
const MAX_PRINT_SIDE: i64 = 20_000;
/// Most product zones on one mockup.
pub const MAX_ZONES: usize = 16;
/// An object smaller than this share of the photo is not a zone of its own.
const MIN_OBJECT: f64 = 0.004;
/// Most visuals on one mockup.
const MAX_VISUALS: usize = 50;
/// Resolution of a visual whose file records none.
const FALLBACK_DPI: f64 = 300.0;

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: MOCKUP.into(), msg: msg.into() }
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

// ---------------------------------------------------------------- the photo

/// The garment's photo: its name, its pixels (RGBA bytes, opaque) and resolution.
#[derive(Clone)]
pub struct Photo {
    pub name: String,
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub dpi: f64,
}

/// Whether `file` can be a mockup's photo (PNG, JPG).
pub fn is_photo_file(file: &str) -> bool {
    matches!(file_cmds::extension(file).as_deref(), Some("png" | "jpg" | "jpeg"))
}

/// A PNG or JPG photo, flattened on white (a transparent PNG would show the checkerboard).
pub fn load_photo_bytes(file: &str, bytes: &[u8]) -> Result<Photo> {
    if !is_photo_file(file) {
        return Err(bad(format!("{}: the photo is a PNG or JPG file", file_cmds::file_name(file))));
    }
    crate::allocation::checkpoint("reading a mockup photo")?;
    let doc = file_cmds::import(&file_cmds::file_name(file), bytes)?;
    let (w, h) = (doc.size.width, doc.size.height);
    if w == 0 || h == 0 {
        return Err(bad(format!("{}: the image is empty", file_cmds::file_name(file))));
    }
    let surf = file_cmds::flattened(&doc, gang::format());
    let mut rgba = surf.to_interleaved(Rect::new(0, 0, w as i32, h as i32));
    for px in rgba.chunks_exact_mut(4) {
        let a = f32::from(px[3]) / 255.0;
        for c in &mut px[..3] {
            *c = (f32::from(*c) * a + 255.0 * (1.0 - a)).round() as u8;
        }
        px[3] = 255;
    }
    Ok(Photo { name: file_cmds::stem(file), rgba, width: w, height: h, dpi: f64::from(doc.resolution_dpi) })
}

/// A PNG or JPG photo, read from disk.
pub fn load_photo(path: &str) -> Result<Photo> {
    let bytes = file_cmds::read_file(path)?;
    load_photo_bytes(path, &bytes)
}

impl Photo {
    /// The photo at most `side` pixels on its longest side (box-averaged), and the factor applied.
    pub fn reduced(&self, side: u32) -> (Photo, f64) {
        let longest = self.width.max(self.height);
        if longest <= side || side == 0 {
            return (self.clone(), 1.0);
        }
        let k = f64::from(side) / f64::from(longest);
        let (w, h) = (((f64::from(self.width) * k).round() as u32).max(1), ((f64::from(self.height) * k).round() as u32).max(1));
        let (sw, sh) = (self.width as usize, self.height as usize);
        let mut out = vec![0u8; w as usize * h as usize * 4];
        for y in 0..h as usize {
            let y0 = y * sh / h as usize;
            let y1 = ((y + 1) * sh / h as usize).max(y0 + 1).min(sh);
            for x in 0..w as usize {
                let x0 = x * sw / w as usize;
                let x1 = ((x + 1) * sw / w as usize).max(x0 + 1).min(sw);
                let mut acc = [0u32; 4];
                for yy in y0..y1 {
                    for xx in x0..x1 {
                        let i = (yy * sw + xx) * 4;
                        for c in 0..4 {
                            acc[c] += u32::from(self.rgba[i + c]);
                        }
                    }
                }
                let n = ((y1 - y0) * (x1 - x0)) as u32;
                let o = (y * w as usize + x) * 4;
                for c in 0..4 {
                    out[o + c] = ((acc[c] + n / 2) / n) as u8;
                }
            }
        }
        (Photo { name: self.name.clone(), rgba: out, width: w, height: h, dpi: self.dpi * k }, k)
    }
}

/// The scale from a line drawn on the photo: pixels per cm (`None` for a line too short or a
/// length not positive).
pub fn pixels_per_cm(from: [f64; 2], to: [f64; 2], cm: f64) -> Option<f64> {
    let len = ((to[0] - from[0]).powi(2) + (to[1] - from[1]).powi(2)).sqrt();
    (len >= 2.0 && cm.is_finite() && cm > 0.0).then(|| len / cm)
}

/// A visual's own print size in cm (its pixels at its resolution).
pub fn natural_size_cm(v: &Visual) -> (f64, f64) {
    let dpi = if v.dpi.is_finite() && v.dpi >= 1.0 { v.dpi } else { FALLBACK_DPI };
    (f64::from(v.width) / dpi * 2.54, f64::from(v.height) / dpi * 2.54)
}

// ---------------------------------------------------------------- placing a visual

/// The warp styles offered on a garment (Edit › Transform › Warp's presets that suit a print).
pub const STYLES: [WarpStyle; 5] = [WarpStyle::Arc, WarpStyle::Arch, WarpStyle::Bulge, WarpStyle::Flag, WarpStyle::Inflate];

/// A visual's warp inside its own box: none, a preset with its bend, or a custom grid.
#[derive(Clone, Debug, PartialEq)]
pub struct Bend {
    /// `None`, `Custom` or one of [`STYLES`].
    pub style: WarpStyle,
    /// -100..100 (presets).
    pub bend: f64,
    /// Bent along the vertical axis (presets).
    pub vertical: bool,
    /// The 4 × 4 Bezier control points in box units (0..1, y down), row-major (`Custom`).
    pub points: Option<[[f64; 2]; 16]>,
}

impl Default for Bend {
    fn default() -> Self {
        Bend { style: WarpStyle::None, bend: 0.0, vertical: false, points: None }
    }
}

/// The unbent control grid.
pub fn rest_points() -> [[f64; 2]; 16] {
    std::array::from_fn(|k| [(k % 4) as f64 / 3.0, (k / 4) as f64 / 3.0])
}

impl Bend {
    /// The control grid in box units for a box of `w × h` (a preset's arcs are round in that box);
    /// `None` when nothing bends.
    pub fn grid(&self, w: f64, h: f64) -> Option<[[f64; 2]; 16]> {
        match self.style {
            WarpStyle::Custom => self.points.filter(|p| *p != rest_points()),
            st if st.is_preset() => {
                let sw = StyleWarp::new(st, self.bend, 0.0, 0.0, self.vertical, [0.0, 0.0, w.max(1e-6), h.max(1e-6)])?;
                let m = sw.mesh();
                let mut out = [[0.0; 2]; 16];
                for (o, p) in out.iter_mut().zip(&m.points) {
                    *o = [p[0] / w.max(1e-6), p[1] / h.max(1e-6)];
                }
                Some(out)
            }
            _ => None,
        }
    }

    /// The warp over the box `[0, 0, w, h]` (pixels).
    pub fn warp(&self, w: f64, h: f64) -> Option<Warp> {
        let g = self.grid(w, h)?;
        let points = g.iter().map(|p| [p[0] * w, p[1] * h]).collect();
        Some(Warp::custom(BezierMesh { us: vec![0.0, 1.0], vs: vec![0.0, 1.0], points }, [0.0, 0.0, w, h]))
    }

    /// The point `(u, v)` of the box (units) once bent.
    pub fn map(&self, grid: Option<&[[f64; 2]; 16]>, u: f64, v: f64) -> [f64; 2] {
        let Some(g) = grid else { return [u, v] };
        let b = |t: f64| [(1.0 - t).powi(3), 3.0 * t * (1.0 - t).powi(2), 3.0 * t * t * (1.0 - t), t.powi(3)];
        let (bu, bv) = (b(u), b(v));
        let mut p = [0.0; 2];
        for (j, wv) in bv.iter().enumerate() {
            for (i, wu) in bu.iter().enumerate() {
                p[0] += wu * wv * g[j * 4 + i][0];
                p[1] += wu * wv * g[j * 4 + i][1];
            }
        }
        p
    }

    fn to_json(&self) -> Value {
        let mut v = json!({"style": self.style.id(), "bend": self.bend, "vertical": self.vertical});
        if let Some(p) = self.points {
            v["points"] = json!(p);
        }
        v
    }

    /// From `{"style", "bend", "vertical", "points"}`.
    pub fn from_json(v: &Value) -> Result<Bend> {
        let style = match v.get("style").and_then(Value::as_str) {
            None => WarpStyle::None,
            Some(s) => WarpStyle::parse(s)
                .filter(|w| matches!(w, WarpStyle::None | WarpStyle::Custom) || STYLES.contains(w))
                .ok_or_else(|| bad(format!("warp style `{s}`: none, custom, arc, arch, bulge, flag or inflate")))?,
        };
        check_range(v, "bend", -100.0, 100.0)?;
        let points = match v.get("points").and_then(Value::as_array) {
            None => None,
            Some(a) => {
                let pts: Vec<[f64; 2]> = a.iter().filter_map(|p| point(Some(p))).collect();
                if pts.len() != 16 || pts.iter().flatten().any(|c| !c.is_finite() || c.abs() > 10.0) {
                    return Err(bad("warp `points`: 16 points [u, v] in the visual's box units"));
                }
                Some(std::array::from_fn(|k| pts[k]))
            }
        };
        if style == WarpStyle::Custom && points.is_none() {
            return Err(bad("a custom warp needs its 16 `points`"));
        }
        Ok(Bend { style, bend: num(v, "bend", 0.0), vertical: v.get("vertical").and_then(Value::as_bool).unwrap_or(false), points })
    }
}

/// Where a visual lies on the photo: its four corners (photo pixels, clockwise from the top
/// left) and its warp.
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub corners: [[f64; 2]; 4],
    pub bend: Bend,
}

/// The corners of a `w × h` box centred on `center`, turned `rotation` degrees clockwise.
pub fn rect_corners(center: [f64; 2], w: f64, h: f64, rotation: f64) -> [[f64; 2]; 4] {
    let (s, c) = rotation.to_radians().sin_cos();
    [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)].map(|(x, y)| [center[0] + x * c - y * s, center[1] + x * s + y * c])
}

fn dist(a: [f64; 2], b: [f64; 2]) -> f64 {
    (a[0] - b[0]).hypot(a[1] - b[1])
}

impl Placement {
    /// The pixel size to draw the visual at before it is placed: its longest top/bottom and
    /// left/right sides (at least 1).
    pub fn rendition_size(&self) -> (i64, i64) {
        let c = &self.corners;
        let w = dist(c[0], c[1]).max(dist(c[3], c[2])).round().max(1.0) as i64;
        let h = dist(c[0], c[3]).max(dist(c[1], c[2])).round().max(1.0) as i64;
        (w, h)
    }

    /// The point `(u, v)` of the visual (box units) on the photo: bent, then through the corners.
    pub fn map(&self, grid: Option<&[[f64; 2]; 16]>, u: f64, v: f64) -> Option<[f64; 2]> {
        let h = Homography::rect_to_quad([0.0, 0.0, 1.0, 1.0], self.corners)?;
        let [bu, bv] = self.bend.map(grid, u, v);
        let (x, y) = h.apply(bu, bv);
        Some([x, y])
    }

    fn to_json(&self) -> Value {
        json!({"corners": self.corners, "warp": self.bend.to_json()})
    }
}

/// A visual drawn on the photo: RGBA bytes, `width × height`, at `(x, y)` (photo pixels).
pub struct Placed {
    pub rgba: Vec<u8>,
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
}

/// The visual drawn at `w × h` pixels, unplaced (cache it while only its place changes).
pub fn rendition(v: &Visual, w: i64, h: i64) -> Result<Surface> {
    if w > MAX_PRINT_SIDE || h > MAX_PRINT_SIDE {
        return Err(bad(format!("{}: too large on this photo", v.name)));
    }
    gang::render_visual(v, w.max(1), h.max(1))
}

/// `src` (a rendition `w × h`) placed through `placement`, cut to the photo (`clip_w × clip_h`).
/// `None` when nothing lands on the photo.
pub fn place_rendition(src: &Surface, w: i64, h: i64, placement: &Placement, clip_w: u32, clip_h: u32) -> Result<Option<Placed>> {
    let (wf, hf) = (w as f64, h as f64);
    let Some(hm) = Homography::rect_to_quad([0.0, 0.0, wf, hf], placement.corners) else { return Ok(None) };
    let warp = placement.bend.warp(wf, hf);
    let out = photocraft_algo::warp::place_source_projective(src, Rect::new(0, 0, w as i32, h as i32), &hm, warp.as_ref()).map_err(|e| bad(e.to_string()))?;
    let r = out.tile_bounds().intersect(&Rect::new(0, 0, clip_w as i32, clip_h as i32));
    if r.is_empty() {
        return Ok(None);
    }
    Ok(Some(Placed { rgba: out.to_interleaved(r), x: r.x0, y: r.y0, width: r.width() as usize, height: r.height() as usize }))
}

/// The visual placed on a photo of `clip_w × clip_h` pixels.
pub fn place(v: &Visual, placement: &Placement, clip_w: u32, clip_h: u32) -> Result<Option<Placed>> {
    let (w, h) = placement.rendition_size();
    let src = rendition(v, w, h)?;
    place_rendition(&src, w, h, placement, clip_w, clip_h)
}

// ---------------------------------------------------------------- the render

/// The render settings, each 0..1.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Render {
    pub shadows: f32,
    pub folds: f32,
    pub opacity: f32,
}

impl Default for Render {
    fn default() -> Self {
        Render { shadows: (DEFAULT_SHADOWS / 100.0) as f32, folds: (DEFAULT_FOLDS / 100.0) as f32, opacity: (DEFAULT_OPACITY / 100.0) as f32 }
    }
}

/// The photo's light, computed once per photo: its luminance and the luminance blurred (the
/// folds' shape, without the weave).
pub struct Light {
    width: usize,
    height: usize,
    lum: Vec<f32>,
    blur: Vec<f32>,
    radius: f32,
}

/// One pass of a box blur of radius `r` along rows (`stride` 1) or columns.
fn box_pass(src: &[f32], dst: &mut [f32], len: usize, count: usize, step: usize, stride: usize, r: usize) {
    for k in 0..count {
        let base = k * stride;
        let at = |i: usize| src[base + i * step];
        let mut sum = 0.0f32;
        // Edges repeat outwards.
        for i in 0..=r {
            sum += at(i.min(len - 1));
        }
        sum += at(0) * r as f32;
        let n = (2 * r + 1) as f32;
        for i in 0..len {
            dst[base + i * step] = sum / n;
            let add = at((i + r + 1).min(len - 1));
            let sub = at(i.saturating_sub(r));
            sum += add - sub;
        }
    }
}

impl Light {
    pub fn new(rgba: &[u8], width: usize, height: usize) -> Light {
        let lum: Vec<f32> = rgba.chunks_exact(4).map(|p| (0.2126 * f32::from(p[0]) + 0.7152 * f32::from(p[1]) + 0.0722 * f32::from(p[2])) / 255.0).collect();
        let r = (width.max(height) / 250).max(1);
        let mut a = lum.clone();
        let mut b = vec![0.0f32; lum.len()];
        // Two box passes each way: close to a Gaussian.
        for _ in 0..2 {
            box_pass(&a, &mut b, width, height, 1, width, r);
            box_pass(&b, &mut a, height, width, width, 1, r);
        }
        Light { width, height, lum, blur: a, radius: r as f32 }
    }

    fn lum(&self, x: usize, y: usize) -> f32 {
        self.lum[y * self.width + x]
    }

    /// The blurred light's slope at `(x, y)`, over the blur radius (about −1..1).
    fn slope(&self, x: usize, y: usize) -> (f32, f32) {
        let b = |x: usize, y: usize| self.blur[y * self.width + x];
        let (xl, xr) = (x.saturating_sub(1), (x + 1).min(self.width - 1));
        let (yu, yd) = (y.saturating_sub(1), (y + 1).min(self.height - 1));
        let gx = (b(xr, y) - b(xl, y)) / (xr - xl).max(1) as f32;
        let gy = (b(x, yd) - b(x, yu)) / (yd - yu).max(1) as f32;
        ((gx * self.radius * 2.0).clamp(-1.0, 1.0), (gy * self.radius * 2.0).clamp(-1.0, 1.0))
    }
}

/// A placed visual as the render reads it: RGBA bytes, `width × height`, at `(x, y)` on the photo.
pub struct Print<'a> {
    pub rgba: &'a [u8],
    pub x: i32,
    pub y: i32,
    pub width: usize,
    pub height: usize,
    /// Its product zone: coverage 0..255 over the whole photo, row-major; the ink stops at its
    /// edge. `None`: not masked.
    pub zone: Option<&'a [u8]>,
}

impl<'a> From<&'a Placed> for Print<'a> {
    fn from(p: &'a Placed) -> Print<'a> {
        Print { rgba: &p.rgba, x: p.x, y: p.y, width: p.width, height: p.height, zone: None }
    }
}

impl<'a> Print<'a> {
    /// Masked by `zone` (photo-size coverage).
    pub fn in_zone(self, zone: Option<&'a [u8]>) -> Print<'a> {
        Print { zone, ..self }
    }
}

/// Premultiplied bilinear sample of `p` at `(u, v)` (its own pixels, centres at .5); transparent
/// outside.
fn sample(p: &Print, u: f32, v: f32) -> [f32; 4] {
    let (u, v) = (u - 0.5, v - 0.5);
    let (x0, y0) = (u.floor(), v.floor());
    let (fx, fy) = (u - x0, v - y0);
    let mut out = [0.0f32; 4];
    for (dy, wy) in [(0i64, 1.0 - fy), (1, fy)] {
        for (dx, wx) in [(0i64, 1.0 - fx), (1, fx)] {
            let (x, y) = (x0 as i64 + dx, y0 as i64 + dy);
            if x < 0 || y < 0 || x >= p.width as i64 || y >= p.height as i64 {
                continue;
            }
            let w = wx * wy;
            if w <= 0.0 {
                continue;
            }
            let i = (y as usize * p.width + x as usize) * 4;
            let a = f32::from(p.rgba[i + 3]) / 255.0;
            for c in 0..3 {
                out[c] += f32::from(p.rgba[i + c]) / 255.0 * a * w;
            }
            out[3] += a * w;
        }
    }
    out
}

/// The photo (`rgba`, `width × height`) with the placed prints on it, in order (the last on top),
/// at `ppcm` pixels per cm.
pub fn composite(rgba: &[u8], width: usize, height: usize, light: &Light, prints: &[Print], render: Render, ppcm: f64) -> Vec<u8> {
    let mut out = rgba.to_vec();
    if width == 0 || height == 0 || light.width != width || light.height != height {
        return out;
    }
    let shift = (f64::from(render.folds.clamp(0.0, 1.0)) * MAX_FOLD_CM * ppcm) as f32;
    let shadows = render.shadows.clamp(0.0, 1.0);
    let opacity = render.opacity.clamp(0.0, 1.0);
    for p in prints {
        if p.width == 0 || p.height == 0 || p.rgba.len() < p.width * p.height * 4 {
            continue;
        }
        // Its box on the photo, widened by the largest fold shift.
        let pad = shift.ceil() as i64 + 1;
        let x0 = (i64::from(p.x) - pad).max(0) as usize;
        let y0 = (i64::from(p.y) - pad).max(0) as usize;
        let x1 = ((i64::from(p.x) + p.width as i64 + pad).max(0) as usize).min(width);
        let y1 = ((i64::from(p.y) + p.height as i64 + pad).max(0) as usize).min(height);
        if x0 >= x1 || y0 >= y1 {
            continue;
        }
        // The print area's median light: the photo's light around it is the ink's own.
        let mut hist = [0u32; 256];
        for y in y0..y1 {
            for x in x0..x1 {
                hist[(light.lum(x, y) * 255.0).round().clamp(0.0, 255.0) as usize] += 1;
            }
        }
        let total: u32 = hist.iter().sum();
        let (mut seen, mut median) = (0u32, 128usize);
        for (k, n) in hist.iter().enumerate() {
            seen += n;
            if seen * 2 >= total {
                median = k;
                break;
            }
        }
        let reference = (median as f32 / 255.0).max(0.02);
        let (ox, oy) = (p.x as f32, p.y as f32);
        for y in y0..y1 {
            for x in x0..x1 {
                let (mut qx, mut qy) = (x as f32 + 0.5, y as f32 + 0.5);
                if shift > 0.0 {
                    let (gx, gy) = light.slope(x, y);
                    qx += gx * shift;
                    qy += gy * shift;
                }
                let px = sample(p, qx - ox, qy - oy);
                let inside = p.zone.and_then(|z| z.get(y * width + x)).map_or(1.0, |c| f32::from(*c) / 255.0);
                let a = px[3] * opacity * inside;
                if a <= 0.0 {
                    continue;
                }
                // Darker than the print area: the ink darkens as much; lighter: much less (a
                // highlight on ink stays ink, it does not wash out).
                let ratio = light.lum(x, y) / reference - 1.0;
                let shade = if ratio < 0.0 { 1.0 + shadows * ratio } else { (1.0 + shadows * HIGHLIGHT * ratio).min(MAX_LIGHTEN) }.max(0.0);
                let i = (y * width + x) * 4;
                for k in 0..3 {
                    let ink = (px[k] / px[3].max(1e-6) * shade).clamp(0.0, 1.0);
                    let under = f32::from(out[i + k]) / 255.0;
                    out[i + k] = ((under * (1.0 - a) + ink * a) * 255.0).round().clamp(0.0, 255.0) as u8;
                }
            }
        }
    }
    out
}

/// A screenshot (or any RGBA image) as a photo.
pub fn photo_from_rgba(name: &str, width: u32, height: u32, rgba: Vec<u8>) -> Result<Photo> {
    if width == 0 || height == 0 || rgba.len() != width as usize * height as usize * 4 {
        return Err(bad("the image is empty"));
    }
    let mut p = Photo { name: name.to_string(), rgba, width, height, dpi: 72.0 };
    for px in p.rgba.chunks_exact_mut(4) {
        let a = f32::from(px[3]) / 255.0;
        for c in &mut px[..3] {
            *c = (f32::from(*c) * a + 255.0 * (1.0 - a)).round() as u8;
        }
        px[3] = 255;
    }
    Ok(p)
}

/// Writes the photo as a PNG (a pasted screenshot gets a file, for the export to read).
pub fn write_photo_png(p: &Photo, path: &str) -> Result<()> {
    let mut doc = Document::new(&p.name, Size::new(p.width, p.height), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = p.dpi as f32;
    doc.layers.clear();
    let surf = Surface::from_interleaved(gang::format(), Rect::new(0, 0, p.width as i32, p.height as i32), &p.rgba);
    doc.layers.push(Layer::new(p.name.clone(), LayerContent::Raster(surf)));
    file_cmds::save_doc(&doc, path, file_cmds::SaveOpts::default())?;
    Ok(())
}

// ---------------------------------------------------------------- product zones

/// A polygon (photo pixels) as a zone: coverage 0..255 over a `w × h` photo, its edges smoothed
/// (four sub-rows a pixel, exact across).
pub fn zone_from_polygon(points: &[[f64; 2]], w: u32, h: u32) -> Vec<u8> {
    let (w, h) = (w as usize, h as usize);
    let mut out = vec![0u8; w * h];
    if points.len() < 3 || w == 0 || h == 0 {
        return out;
    }
    const SUB: usize = 4;
    let y_min = points.iter().map(|p| p[1]).fold(f64::MAX, f64::min).floor().max(0.0) as usize;
    let y_max = (points.iter().map(|p| p[1]).fold(f64::MIN, f64::max).ceil().max(0.0) as usize).min(h);
    let mut row = vec![0.0f32; w + 1];
    let mut xs = Vec::new();
    for y in y_min..y_max {
        row.iter_mut().for_each(|v| *v = 0.0);
        for s in 0..SUB {
            let sy = y as f64 + (s as f64 + 0.5) / SUB as f64;
            xs.clear();
            for k in 0..points.len() {
                let (a, b) = (points[k], points[(k + 1) % points.len()]);
                if (a[1] <= sy) != (b[1] <= sy) {
                    xs.push(a[0] + (sy - a[1]) / (b[1] - a[1]) * (b[0] - a[0]));
                }
            }
            xs.sort_by(f64::total_cmp);
            for pair in xs.chunks_exact(2) {
                let (x0, x1) = (pair[0].clamp(0.0, w as f64), pair[1].clamp(0.0, w as f64));
                if x1 <= x0 {
                    continue;
                }
                let (i0, i1) = (x0.floor() as usize, x1.floor() as usize);
                if i0 == i1 {
                    row[i0] += (x1 - x0) as f32;
                } else {
                    row[i0] += (i0 as f64 + 1.0 - x0) as f32;
                    for v in &mut row[i0 + 1..i1] {
                        *v += 1.0;
                    }
                    if i1 < w {
                        row[i1] += (x1 - i1 as f64) as f32;
                    }
                }
            }
        }
        for x in 0..w {
            out[y * w + x] = (row[x] / SUB as f32 * 255.0).round().clamp(0.0, 255.0) as u8;
        }
    }
    out
}

/// Adds `src` to `dst` (or takes it away).
pub fn combine_zone(dst: &mut [u8], src: &[u8], subtract: bool) {
    for (d, s) in dst.iter_mut().zip(src) {
        *d = if subtract { (u16::from(*d) * u16::from(255 - *s) / 255) as u8 } else { (*d).max(*s) };
    }
}

/// The separate objects of a zone, each a zone of its own, in reading order: row by row, left to
/// right in a row (8-connected; the
/// smoothed edge goes to the object it borders; specks under [`MIN_OBJECT`] of the photo are
/// dropped).
pub fn split_objects(mask: &[u8], w: u32, h: u32) -> Vec<Vec<u8>> {
    let (w, h) = (w as usize, h as usize);
    let mut label = vec![0u32; w * h];
    let mut sizes = vec![0usize];
    // Each object's box: left, top, right, bottom.
    let mut boxes = vec![[usize::MAX, usize::MAX, 0, 0]];
    let mut stack = Vec::new();
    for start in 0..w * h {
        if mask[start] < 128 || label[start] != 0 {
            continue;
        }
        let id = sizes.len() as u32;
        sizes.push(0);
        boxes.push([usize::MAX, usize::MAX, 0, 0]);
        label[start] = id;
        stack.push(start);
        while let Some(i) = stack.pop() {
            sizes[id as usize] += 1;
            let (x, y) = (i % w, i / w);
            let b = &mut boxes[id as usize];
            *b = [b[0].min(x), b[1].min(y), b[2].max(x), b[3].max(y)];
            for dy in -1i64..=1 {
                for dx in -1i64..=1 {
                    let (nx, ny) = (x as i64 + dx, y as i64 + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if mask[j] >= 128 && label[j] == 0 {
                        label[j] = id;
                        stack.push(j);
                    }
                }
            }
        }
    }
    let min = ((w * h) as f64 * MIN_OBJECT) as usize;
    let mut kept: Vec<u32> = (1..sizes.len() as u32).filter(|id| sizes[*id as usize] >= min.max(1)).collect();
    // Rows: by top; an object whose middle is above the row's lowest bottom joins it.
    kept.sort_by_key(|id| boxes[*id as usize][1]);
    let mut rows: Vec<Vec<u32>> = Vec::new();
    let mut row_bottom = 0usize;
    for id in kept {
        let b = boxes[id as usize];
        let middle = (b[1] + b[3]) / 2;
        match rows.last_mut() {
            Some(row) if middle < row_bottom => {
                row.push(id);
                row_bottom = row_bottom.max(b[3]);
            }
            _ => {
                rows.push(vec![id]);
                row_bottom = b[3];
            }
        }
    }
    let mut kept: Vec<u32> = rows
        .into_iter()
        .flat_map(|mut row| {
            row.sort_by_key(|id| boxes[*id as usize][0]);
            row
        })
        .collect();
    kept.truncate(MAX_ZONES);
    kept.iter()
        .map(|id| {
            let mut z = vec![0u8; w * h];
            for i in 0..w * h {
                if label[i] == *id {
                    z[i] = mask[i];
                } else if mask[i] > 0 && label[i] == 0 {
                    // The smoothed edge: kept when it touches this object.
                    let (x, y) = (i % w, i / w);
                    let near =
                        (y.saturating_sub(1)..=(y + 1).min(h - 1)).any(|yy| (x.saturating_sub(1)..=(x + 1).min(w - 1)).any(|xx| label[yy * w + xx] == *id));
                    if near {
                        z[i] = mask[i];
                    }
                }
            }
            z
        })
        .collect()
}

/// The photo's product(s), found automatically, one zone per object. A packshot's plain
/// backdrop (a uniform frame) is taken away from the edges inwards, so light garments on white
/// are found whole; on a busy background, PhotoCraft's Select Subject.
pub fn product_zones(photo: &Photo) -> Vec<Vec<u8>> {
    match backdrop_mask(photo) {
        Some(m) => split_objects(&m, photo.width, photo.height),
        None => subject_zones(photo),
    }
}

/// What isn't the plain backdrop: the colours of the photo's frame flood in from its edges (within
/// a tolerance set by how even the frame is); `None` when the frame isn't plain.
pub fn backdrop_mask(photo: &Photo) -> Option<Vec<u8>> {
    let (w, h) = (photo.width as usize, photo.height as usize);
    if w < 3 || h < 3 {
        return None;
    }
    let px = |i: usize| [photo.rgba[i * 4], photo.rgba[i * 4 + 1], photo.rgba[i * 4 + 2]];
    let border: Vec<usize> = (0..w).flat_map(|x| [x, (h - 1) * w + x]).chain((1..h - 1).flat_map(|y| [y * w, y * w + w - 1])).collect();
    let median = |c: usize| {
        let mut v: Vec<u8> = border.iter().map(|i| px(*i)[c]).collect();
        v.sort_unstable();
        v[v.len() / 2]
    };
    let bg = [median(0), median(1), median(2)];
    let dist = |p: [u8; 3]| (0..3).map(|c| (i16::from(p[c]) - i16::from(bg[c])).unsigned_abs()).max().unwrap_or(0);
    let mut devs: Vec<u16> = border.iter().map(|i| dist(px(*i))).collect();
    devs.sort_unstable();
    let p95 = devs[devs.len() * 95 / 100];
    if p95 > 25 {
        return None;
    }
    let tol = (p95 * 3 + 3).clamp(4, 30);
    let mut back = vec![false; w * h];
    let mut stack: Vec<usize> = border.iter().copied().filter(|i| dist(px(*i)) <= tol).collect();
    for i in &stack {
        back[*i] = true;
    }
    while let Some(i) = stack.pop() {
        let (x, y) = (i % w, i / w);
        let mut visit = |j: usize| {
            if !back[j] && dist(px(j)) <= tol {
                back[j] = true;
                stack.push(j);
            }
        };
        if x > 0 {
            visit(i - 1);
        }
        if x + 1 < w {
            visit(i + 1);
        }
        if y > 0 {
            visit(i - w);
        }
        if y + 1 < h {
            visit(i + w);
        }
    }
    // A 3 × 3 average smooths the cut edge.
    let mut out = vec![0u8; w * h];
    for y in 0..h {
        for x in 0..w {
            let (mut n, mut inside) = (0u32, 0u32);
            for yy in y.saturating_sub(1)..=(y + 1).min(h - 1) {
                for xx in x.saturating_sub(1)..=(x + 1).min(w - 1) {
                    n += 1;
                    inside += u32::from(!back[yy * w + xx]);
                }
            }
            out[y * w + x] = (inside * 255 / n) as u8;
        }
    }
    out.iter().any(|v| *v >= 128).then_some(out)
}

/// The photo's product(s) by PhotoCraft's Select Subject, one zone per object.
pub fn subject_zones(photo: &Photo) -> Vec<Vec<u8>> {
    let r = Rect::new(0, 0, photo.width as i32, photo.height as i32);
    let surf = Surface::from_interleaved(gang::format(), r, &photo.rgba);
    let Some(region) = photocraft_algo::segment::subject::select_subject(&photocraft_algo::segment::SurfaceSampler(&surf), r) else { return Vec::new() };
    split_objects(&region_mask(&region, photo.width, photo.height), photo.width, photo.height)
}

/// A selection region as a zone over a `w × h` photo.
pub fn region_mask(region: &photocraft_algo::selection::Region, w: u32, h: u32) -> Vec<u8> {
    let mut z = vec![0u8; w as usize * h as usize];
    let b = region.bbox.intersect(&Rect::new(0, 0, w as i32, h as i32));
    for y in b.y0..b.y1 {
        for x in b.x0..b.x1 {
            z[y as usize * w as usize + x as usize] = (region.at(x, y) * 255.0).round() as u8;
        }
    }
    z
}

/// A document's selection as a zone (the document the photo's size), `None` without one.
pub fn selection_zone(doc: &Document) -> Option<Vec<u8>> {
    let sel = doc.selection.as_ref()?;
    let r = Rect::new(0, 0, doc.size.width as i32, doc.size.height as i32);
    let m = photocraft_algo::selection::mask_from_surface(Some(sel), r);
    let z: Vec<u8> = m.iter().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8).collect();
    z.iter().any(|v| *v > 0).then_some(z)
}

/// A zone reduced to `tw × th` (box average), for the preview.
pub fn reduce_zone(z: &[u8], w: u32, h: u32, tw: u32, th: u32) -> Vec<u8> {
    let (w, h, tw, th) = (w as usize, h as usize, tw as usize, th as usize);
    let mut out = vec![0u8; tw * th];
    if w == 0 || h == 0 || tw == 0 || th == 0 {
        return out;
    }
    for y in 0..th {
        let (y0, y1) = (y * h / th, ((y + 1) * h / th).max(y * h / th + 1).min(h));
        for x in 0..tw {
            let (x0, x1) = (x * w / tw, ((x + 1) * w / tw).max(x * w / tw + 1).min(w));
            let mut acc = 0u32;
            for yy in y0..y1 {
                acc += z[yy * w + x0..yy * w + x1].iter().map(|v| u32::from(*v)).sum::<u32>();
            }
            let n = ((y1 - y0) * (x1 - x0)) as u32;
            out[y * tw + x] = ((acc + n / 2) / n) as u8;
        }
    }
    out
}

/// Writes a zone as a grey PNG (white inside), for the export to read.
pub fn write_zone_png(z: &[u8], w: u32, h: u32, path: &str) -> Result<()> {
    let rgba: Vec<u8> = z.iter().flat_map(|v| [*v, *v, *v, 255]).collect();
    write_photo_png(&Photo { name: "zone".into(), rgba, width: w, height: h, dpi: 72.0 }, path)
}

/// A zone from a PNG mask the photo's size (white inside).
pub fn load_zone(path: &str, w: u32, h: u32) -> Result<Vec<u8>> {
    let p = load_photo(path)?;
    if (p.width, p.height) != (w, h) {
        return Err(bad(format!("{}: the zone is {} × {} px, the photo {w} × {h}", file_cmds::file_name(path), p.width, p.height)));
    }
    Ok(p.rgba.chunks_exact(4).map(|c| c[0]).collect())
}

/// The zone whose coverage is highest at `q` (photo pixels), `None` outside them all.
pub fn zone_at(zones: &[&[u8]], w: u32, h: u32, q: [f64; 2]) -> Option<usize> {
    let (x, y) = (q[0].floor(), q[1].floor());
    if x < 0.0 || y < 0.0 || x >= f64::from(w) || y >= f64::from(h) {
        return None;
    }
    let i = y as usize * w as usize + x as usize;
    zones.iter().enumerate().filter(|(_, z)| z.get(i).is_some_and(|v| *v >= 128)).max_by_key(|(_, z)| z[i]).map(|(k, _)| k)
}

// ---------------------------------------------------------------- the command

fn load_visual(s: &Session, i: usize, v: &Value) -> Result<Visual> {
    if let Some(path) = v.get("path").and_then(Value::as_str) {
        gang::load_file(path).map_err(|e| bad(e.to_string()))
    } else if let Some(d) = v.get("document").and_then(Value::as_u64) {
        let st = s.documents().get(d as usize).ok_or_else(|| bad(format!("visual {i}: no document {d}")))?;
        gang::load_document(&st.doc, &st.doc.name).map_err(|e| bad(e.to_string()))
    } else {
        Err(bad(format!("visual {i}: give a \"path\" or a \"document\"")))
    }
}

fn point(v: Option<&Value>) -> Option<[f64; 2]> {
    let a = v?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
}

fn mockup_cmd(s: &mut Session, p: &Value) -> Result<Value> {
    for k in ["shadows", "folds", "opacity"] {
        check_range(p, k, 0.0, 100.0)?;
    }
    check_range(p, "pixelsPerCm", 0.01, 2000.0)?;
    let path = p.get("photo").and_then(Value::as_str).filter(|v| !v.trim().is_empty()).ok_or_else(|| bad("give the `photo` (a PNG or JPG path)"))?;
    let photo = load_photo(path)?;
    let ppcm = if let Some(v) = p.get("pixelsPerCm").and_then(Value::as_f64) {
        v
    } else {
        let sc = p.get("scale").ok_or_else(|| bad("give the photo's `scale` (a line on it and its length in cm) or `pixelsPerCm`"))?;
        check_range(sc, "cm", 0.1, 1000.0).map_err(|_| bad("`scale.cm` must be a number in 0.1..1000"))?;
        let (from, to) = (point(sc.get("from")), point(sc.get("to")));
        let (Some(from), Some(to)) = (from, to) else { return Err(bad("`scale` needs `from` and `to`: [x, y] in photo pixels")) };
        pixels_per_cm(from, to, num(sc, "cm", 0.0)).ok_or_else(|| bad("the scale line is too short or its length is missing"))?
    };
    let list = p.get("visuals").and_then(Value::as_array).cloned().unwrap_or_default();
    if list.len() > MAX_VISUALS {
        return Err(bad(format!("at most {MAX_VISUALS} visuals")));
    }
    // The product zones, each a coverage mask the size of the photo.
    let zone_paths = p.get("zones").and_then(Value::as_array).cloned().unwrap_or_default();
    if zone_paths.len() > MAX_ZONES {
        return Err(bad(format!("at most {MAX_ZONES} zones")));
    }
    let mut zones = Vec::new();
    for (i, z) in zone_paths.iter().enumerate() {
        let path = z.as_str().ok_or_else(|| bad(format!("zone {i}: a PNG path")))?;
        zones.push(load_zone(path, photo.width, photo.height)?);
    }
    let mut placed = Vec::new();
    let mut placements = Vec::new();
    for (i, v) in list.iter().enumerate() {
        check_range(v, "widthCm", 0.1, 500.0)?;
        check_range(v, "heightCm", 0.1, 500.0)?;
        check_range(v, "rotation", -360.0, 360.0)?;
        let visual = load_visual(s, i, v)?;
        let corners = match v.get("corners").and_then(Value::as_array) {
            Some(a) => {
                let pts: Vec<[f64; 2]> = a.iter().filter_map(|p| point(Some(p))).collect();
                if pts.len() != 4 || pts.iter().flatten().any(|c| !c.is_finite()) {
                    return Err(bad(format!("visual {i}: `corners` are 4 points [x, y] on the photo")));
                }
                [pts[0], pts[1], pts[2], pts[3]]
            }
            None => {
                let (nw, nh) = natural_size_cm(&visual);
                let (w, h) = match (v.get("widthCm").and_then(Value::as_f64), v.get("heightCm").and_then(Value::as_f64)) {
                    (Some(w), Some(h)) => (w, h),
                    (Some(w), None) => (w, w * nh / nw.max(1e-9)),
                    (None, Some(h)) => (h * nw / nh.max(1e-9), h),
                    (None, None) => (nw, nh),
                };
                let center = [num(v, "x", f64::from(photo.width) / 2.0), num(v, "y", f64::from(photo.height) / 2.0)];
                rect_corners(center, w * ppcm, h * ppcm, num(v, "rotation", 0.0))
            }
        };
        let bend = match v.get("warp") {
            Some(w) if !w.is_null() => Bend::from_json(w).map_err(|e| bad(format!("visual {i}: {e}")))?,
            _ => Bend::default(),
        };
        let zone = match v.get("zone") {
            None | Some(Value::Null) => None,
            Some(z) => Some(z.as_u64().map(|z| z as usize).filter(|z| *z < zones.len()).ok_or_else(|| bad(format!("visual {i}: no zone {z}")))?),
        };
        let placement = Placement { corners, bend };
        if let Some(pl) = place(&visual, &placement, photo.width, photo.height)? {
            placed.push((pl, zone));
        }
        let mut pj = placement.to_json();
        pj["zone"] = json!(zone);
        placements.push(pj);
    }
    let render = Render {
        shadows: (num(p, "shadows", DEFAULT_SHADOWS) / 100.0) as f32,
        folds: (num(p, "folds", DEFAULT_FOLDS) / 100.0) as f32,
        opacity: (num(p, "opacity", DEFAULT_OPACITY) / 100.0) as f32,
    };
    crate::allocation::checkpoint("rendering a mockup")?;
    let (w, h) = (photo.width as usize, photo.height as usize);
    let light = Light::new(&photo.rgba, w, h);
    let views: Vec<Print> = placed.iter().map(|(pl, z)| Print::from(pl).in_zone(z.map(|z| zones[z].as_slice()))).collect();
    let pixels = composite(&photo.rgba, w, h, &light, &views, render, ppcm);
    let name = p.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).map_or_else(|| format!("{} mockup", photo.name), str::to_string);
    let mut doc = Document::new(&name, Size::new(photo.width, photo.height), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = photo.dpi as f32;
    doc.layers.clear();
    let surf = Surface::from_interleaved(gang::format(), Rect::new(0, 0, photo.width as i32, photo.height as i32), &pixels);
    doc.layers.push(Layer::new(name.clone(), LayerContent::Raster(surf)));
    let mut out = json!({"pixels": [photo.width, photo.height], "pixelsPerCm": ppcm, "visuals": placements});
    if let Some(path) = p.get("output").and_then(Value::as_str).filter(|v| !v.trim().is_empty()) {
        if !is_photo_file(path) {
            return Err(bad("the mockup is written as a PNG or JPG: give a .png or .jpg `output`"));
        }
        let warnings = file_cmds::save_doc(&doc, path, file_cmds::SaveOpts::default())?;
        out["path"] = json!(path);
        out["warnings"] = json!(warnings);
    }
    if p.get("open").and_then(Value::as_bool).unwrap_or(true) {
        let index = s.add_document(doc, None);
        out["document"] = json!(index);
    }
    Ok(out)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: MOCKUP,
        label: "Mockup…",
        menu: &["DTF"],
        shortcut: None,
        params: r##"{"photo":str (the garment's photo, a PNG or JPG path),"scale":{"from":[x,y],"to":[x,y] (a line on the photo, in its pixels),"cm":0.1..1000 (its real length)}|"pixelsPerCm":0.01..2000,"visuals":[{"path":str (a PNG or SVG)|"document":int,"x":px,"y":px (its centre on the photo; default the photo's centre),"widthCm":0.1..500,"heightCm":0.1..500 (one alone keeps the proportions; default its own print size),"rotation":-360..360=0 (degrees, clockwise),"corners":[[x,y]×4] (in place of x/y/size/rotation: clockwise from the top left, photo pixels),"warp":{"style":"arc|arch|bulge|flag|inflate|custom","bend":-100..100,"vertical":bool,"points":[[u,v]×16] (custom: the 4×4 control grid in the visual's box units)},"zone":int (index in `zones`: the ink stops at its edge; none: not masked)}],"zones":[str] (product zones: PNG masks the photo's size, white inside),"shadows":0..100=60,"folds":0..100=35,"opacity":0..100=95,"name":str,"output":str (a .png or .jpg path),"open":bool=true}"##,
        enabled: crate::commands::always,
        run: mockup_cmd,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(w: usize, h: usize, rgb: [u8; 3]) -> Vec<u8> {
        (0..w * h).flat_map(|_| [rgb[0], rgb[1], rgb[2], 255]).collect()
    }

    fn square(n: usize, rgb: [u8; 3]) -> Vec<u8> {
        flat(n, n, rgb)
    }

    fn at(px: &[u8], w: usize, x: usize, y: usize) -> [u8; 4] {
        let i = (y * w + x) * 4;
        [px[i], px[i + 1], px[i + 2], px[i + 3]]
    }

    /// `w × h` RGBA placed through `placement` on a `pw × ph` photo.
    fn placed(rgba: &[u8], w: usize, h: usize, placement: &Placement, pw: u32, ph: u32) -> Placed {
        let src = Surface::from_interleaved(gang::format(), Rect::new(0, 0, w as i32, h as i32), rgba);
        place_rendition(&src, w as i64, h as i64, placement, pw, ph).unwrap().unwrap()
    }

    /// Without shadows, folds or transparency, the visual lands as it is, centred where asked,
    /// turned about its centre.
    #[test]
    fn a_print_lands_where_it_is_placed() {
        let (w, h) = (100, 80);
        let photo = flat(w, h, [200, 200, 200]);
        let light = Light::new(&photo, w, h);
        let red = square(20, [220, 30, 30]);
        let r = Render { shadows: 0.0, folds: 0.0, opacity: 1.0 };
        let at_rect = |rgba: &[u8], pw: usize, ph: usize, center: [f64; 2], rot: f64, r: Render| {
            let pl = placed(rgba, pw, ph, &Placement { corners: rect_corners(center, pw as f64, ph as f64, rot), bend: Bend::default() }, w as u32, h as u32);
            composite(&photo, w, h, &light, &[Print::from(&pl)], r, 10.0)
        };
        let out = at_rect(&red, 20, 20, [30.0, 40.0], 0.0, r);
        assert_eq!(at(&out, w, 30, 40), [220, 30, 30, 255]);
        assert_eq!(at(&out, w, 21, 31), [220, 30, 30, 255]);
        assert_eq!(at(&out, w, 18, 40), [200, 200, 200, 255], "left of it: the photo");
        assert_eq!(at(&out, w, 70, 40), [200, 200, 200, 255]);
        // A 40 × 4 bar turned 90°: upright.
        let bar = flat(40, 4, [0, 0, 0]);
        let out = at_rect(&bar, 40, 4, [70.0, 40.0], 90.0, r);
        assert!(at(&out, w, 70, 25)[0] < 10, "along the turned bar");
        assert_eq!(at(&out, w, 80, 40)[0], 200, "beside it");
        // Half the opacity: half the fabric shows.
        let out = at_rect(&red, 20, 20, [30.0, 40.0], 0.0, Render { opacity: 0.5, ..r });
        assert_eq!(at(&out, w, 30, 40)[1], 115);
    }

    /// Distort's corners and Warp's arc reach the print: a quad narrower at the top, an arc
    /// lifting the middle.
    #[test]
    fn corners_and_warp_shape_the_print() {
        let (w, h) = (120, 100);
        let red = square(40, [220, 30, 30]);
        // Narrower at the top (perspective).
        let pl = placed(&red, 40, 40, &Placement { corners: [[50.0, 20.0], [70.0, 20.0], [90.0, 80.0], [30.0, 80.0]], bend: Bend::default() }, w, h);
        let src = Print::from(&pl);
        let alpha = |x: i32, y: i32| {
            let (u, v) = (x - src.x, y - src.y);
            if u < 0 || v < 0 || u as usize >= src.width || v as usize >= src.height { 0 } else { src.rgba[(v as usize * src.width + u as usize) * 4 + 3] }
        };
        assert!(alpha(45, 22) < 20 && alpha(60, 22) > 200, "the top is narrow");
        assert!(alpha(35, 77) > 200, "the bottom wide");
        // An arc of 50 % lifts the middle of the top edge above the corners.
        let corners = rect_corners([60.0, 50.0], 40.0, 40.0, 0.0);
        let arc = Placement { corners, bend: Bend { style: WarpStyle::Arc, bend: 50.0, ..Bend::default() } };
        let grid = arc.bend.grid(40.0, 40.0);
        assert!(grid.is_some());
        let top_mid = arc.map(grid.as_ref(), 0.5, 0.0).unwrap();
        assert!(top_mid[1] < 29.0, "{top_mid:?}");
        let pl = placed(&red, 40, 40, &arc, w, h);
        let p = Print::from(&pl);
        let a = |x: i32, y: i32| p.rgba[((y - p.y) as usize * p.width + (x - p.x) as usize) * 4 + 3];
        assert!(a(60, 27) > 200, "the middle bows up");
        // No bend: no warp.
        assert!(Bend { style: WarpStyle::Arc, bend: 0.0, ..Bend::default() }.grid(40.0, 40.0).is_none());
        let custom = Bend { style: WarpStyle::Custom, points: Some(rest_points()), ..Bend::default() };
        assert!(custom.grid(40.0, 40.0).is_none(), "an unmoved grid is no warp");
        assert!(Bend::from_json(&json!({"style": "twist"})).is_err(), "not offered on a garment");
    }

    /// Shadows carry the photo's light onto the ink: darker on the shaded half.
    #[test]
    fn shadows_follow_the_fabric() {
        let (w, h) = (120, 60);
        let mut photo = flat(w, h, [210, 210, 210]);
        for y in 0..h {
            for x in 0..60 {
                let i = (y * w + x) * 4;
                photo[i..i + 3].copy_from_slice(&[110, 110, 110]);
            }
        }
        let light = Light::new(&photo, w, h);
        let ink = square(40, [60, 160, 230]);
        let print = Print { rgba: &ink, x: 40, y: 10, width: 40, height: 40, zone: None };
        let out = composite(&photo, w, h, &light, &[print], Render { shadows: 1.0, folds: 0.0, opacity: 1.0 }, 10.0);
        let (dark, lit) = (at(&out, w, 45, 30), at(&out, w, 75, 30));
        let sum = |p: [u8; 4]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
        assert!(sum(dark) + 40 < sum(lit), "{dark:?} {lit:?}");
        assert!(lit[0] < 120, "a highlight does not wash the ink out: {lit:?}");
        // Folds move the ink along the light: striped ink shifts on the light step.
        let stripes: Vec<u8> = (0..40 * 40).flat_map(|i| if (i % 40) % 4 < 2 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
        let at_step = |folds: f32| {
            let print = Print { rgba: &stripes, x: 40, y: 10, width: 40, height: 40, zone: None };
            composite(&photo, w, h, &light, &[print], Render { shadows: 0.0, folds, opacity: 1.0 }, 10.0)
        };
        let (flat, folded) = (at_step(0.0), at_step(1.0));
        assert!((57..63).any(|x| at(&flat, w, x, 30) != at(&folded, w, x, 30)), "moved on the step");
        assert_eq!(at(&flat, w, 45, 30), at(&folded, w, 45, 30), "not where the fabric is flat");
    }

    /// A lasso polygon makes a smoothed zone; two objects split into two zones, left to right;
    /// the ink stops at its zone's edge.
    #[test]
    fn zones_split_and_mask_the_ink() {
        let (w, h) = (100u32, 60u32);
        let z = zone_from_polygon(&[[10.0, 10.0], [40.5, 10.0], [40.5, 50.0], [10.0, 50.0]], w, h);
        assert_eq!(z[30 * 100 + 20], 255);
        assert_eq!(z[30 * 100 + 5], 0);
        assert_eq!(z[30 * 100 + 40], 128, "half the edge pixel");
        let mut both = zone_from_polygon(&[[60.0, 5.0], [90.0, 5.0], [90.0, 55.0], [60.0, 55.0]], w, h);
        combine_zone(&mut both, &z, false);
        let parts = split_objects(&both, w, h);
        assert_eq!(parts.len(), 2);
        assert_eq!(parts[0][30 * 100 + 20], 255, "the left one first");
        assert_eq!(parts[0][30 * 100 + 70], 0);
        assert_eq!(parts[0][30 * 100 + 40], 128, "with its smoothed edge");
        let mut cut = both.clone();
        combine_zone(&mut cut, &parts[1], true);
        assert_eq!(cut[30 * 100 + 70], 0);
        assert_eq!(zone_at(&[&parts[0], &parts[1]], w, h, [75.0, 30.0]), Some(1));
        assert_eq!(zone_at(&[&parts[0], &parts[1]], w, h, [50.0, 30.0]), None);
        // A print across the left zone's edge stops there.
        let photo = flat(100, 60, [200, 200, 200]);
        let light = Light::new(&photo, 100, 60);
        let ink = flat(40, 20, [0, 0, 0]);
        let print = Print { rgba: &ink, x: 20, y: 20, width: 40, height: 20, zone: Some(&parts[0]) };
        let out = composite(&photo, 100, 60, &light, &[print], Render { shadows: 0.0, folds: 0.0, opacity: 1.0 }, 10.0);
        assert_eq!(at(&out, 100, 30, 30)[0], 0, "inside");
        assert_eq!(at(&out, 100, 50, 30)[0], 200, "outside its zone");
        assert_eq!(reduce_zone(&parts[0], w, h, 50, 30)[15 * 50 + 10], 255);
    }

    /// Light garments on a white backdrop: one zone each, found from the backdrop.
    #[test]
    fn products_on_a_plain_backdrop_are_found() {
        let (w, h) = (120u32, 60u32);
        let mut rgba = flat(120, 60, [254, 254, 254]);
        for (x0, x1) in [(10, 50), (70, 110)] {
            for y in 10..50 {
                for x in x0..x1 {
                    let i = (y * 120 + x) * 4;
                    rgba[i..i + 3].copy_from_slice(&[238, 237, 242]);
                }
            }
        }
        let photo = Photo { name: "tee".into(), rgba, width: w, height: h, dpi: 72.0 };
        let zones = product_zones(&photo);
        assert_eq!(zones.len(), 2);
        assert_eq!(zones[0][30 * 120 + 30], 255);
        assert_eq!(zones[1][30 * 120 + 90], 255);
        assert_eq!(zones[0][30 * 120 + 60], 0);
    }

    #[test]
    fn the_scale_comes_from_the_line() {
        assert_eq!(pixels_per_cm([0.0, 0.0], [300.0, 400.0], 50.0), Some(10.0));
        assert_eq!(pixels_per_cm([5.0, 5.0], [5.0, 6.0], 10.0), None);
        assert_eq!(pixels_per_cm([0.0, 0.0], [100.0, 0.0], 0.0), None);
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dtf-mockup-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// `dtf.mockup` puts a visual at its print size on the photo, at the scale of the line, and
    /// writes the result.
    #[test]
    fn the_command_renders_the_mockup_at_scale() {
        let dir = temp_dir("cmd");
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 400, "height": 300, "resolution": 72, "background": "white"})).unwrap();
        let photo = dir.join("tee.png").to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": photo})).unwrap();
        // 2.54 cm square at 300 ppi: 300 px.
        s.execute("file.new", json!({"width": 300, "height": 300, "resolution": 300, "background": "transparent"})).unwrap();
        s.execute("edit.fill", json!({"color": "#204080"})).unwrap();
        let logo = dir.join("logo.png").to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": logo})).unwrap();
        let out = dir.join("mockup.jpg").to_string_lossy().into_owned();
        // 100 px line = 10 cm: 10 px/cm, the logo 25.4 px wide.
        let r = s
            .execute(
                MOCKUP,
                json!({"photo": photo, "scale": {"from": [0, 0], "to": [100, 0], "cm": 10}, "visuals": [{"path": logo, "x": 200, "y": 150}], "shadows": 0, "folds": 0, "opacity": 100, "output": out}),
            )
            .unwrap();
        assert_eq!(r["pixels"], json!([400, 300]));
        assert!((r["pixelsPerCm"].as_f64().unwrap() - 10.0).abs() < 1e-9);
        assert!(std::path::Path::new(&out).exists());
        let st = s.active().unwrap();
        assert_eq!((st.doc.size.width, st.doc.size.height), (400, 300));
        let surf = st.doc.layers[0].surface().unwrap();
        let inside = surf.read_region(Rect::new(200, 150, 201, 151));
        assert!(inside[2] > 0.4 && inside[0] < 0.2, "{inside:?}");
        let outside = surf.read_region(Rect::new(215, 150, 216, 151));
        assert!(outside[0] > 0.95, "13 px off the centre: past the 25 px print, {outside:?}");
        // Half the width asked: half the size.
        s.execute(MOCKUP, json!({"photo": photo, "pixelsPerCm": 10, "visuals": [{"path": logo, "x": 200, "y": 150, "widthCm": 1.27}], "shadows": 0, "folds": 0, "opacity": 100})).unwrap();
        let surf = s.active().unwrap().doc.layers[0].surface().unwrap().clone();
        assert!(surf.read_region(Rect::new(209, 150, 210, 151))[0] > 0.95);
        // By its corners, with a warp: the corners rule, the arc is accepted.
        let r = s
            .execute(
                MOCKUP,
                json!({"photo": photo, "pixelsPerCm": 10, "visuals": [{"path": logo, "corners": [[100, 100], [300, 100], [300, 200], [100, 200]], "warp": {"style": "arc", "bend": 30}}], "shadows": 0, "folds": 0, "opacity": 100}),
            )
            .unwrap();
        assert_eq!(r["visuals"][0]["warp"]["style"], "arc");
        let surf = s.active().unwrap().doc.layers[0].surface().unwrap().clone();
        assert!(surf.read_region(Rect::new(290, 190, 291, 191))[0] < 0.2, "the corners' box is covered");
        assert!(s.execute(MOCKUP, json!({"photo": photo, "pixelsPerCm": 10, "visuals": [{"path": logo, "corners": [[0, 0], [1, 1]]}]})).is_err());
        // A zone: the ink stops at its edge.
        let zone = dir.join("zone.png").to_string_lossy().into_owned();
        write_zone_png(&zone_from_polygon(&[[0.0, 0.0], [200.0, 0.0], [200.0, 300.0], [0.0, 300.0]], 400, 300), 400, 300, &zone).unwrap();
        let r = s
            .execute(
                MOCKUP,
                json!({"photo": photo, "pixelsPerCm": 10, "zones": [zone], "visuals": [{"path": logo, "corners": [[150, 100], [250, 100], [250, 200], [150, 200]], "zone": 0}], "shadows": 0, "folds": 0, "opacity": 100}),
            )
            .unwrap();
        assert_eq!(r["visuals"][0]["zone"], 0);
        let surf = s.active().unwrap().doc.layers[0].surface().unwrap().clone();
        assert!(surf.read_region(Rect::new(170, 150, 171, 151))[0] < 0.2, "inside the zone");
        assert!(surf.read_region(Rect::new(230, 150, 231, 151))[0] > 0.95, "outside it");
        assert!(s.execute(MOCKUP, json!({"photo": photo, "pixelsPerCm": 10, "zones": [zone], "visuals": [{"path": logo, "zone": 3}]})).is_err());
        // A pasted screenshot written as a PNG is a photo.
        let shot = photo_from_rgba("Screenshot", 4, 2, vec![0, 0, 0, 0].repeat(8)).unwrap();
        assert_eq!(&shot.rgba[..4], &[255, 255, 255, 255], "transparent pixels on white");
        let path = dir.join("shot.png").to_string_lossy().into_owned();
        write_photo_png(&shot, &path).unwrap();
        assert_eq!(load_photo(&path).unwrap().width, 4);
        assert!(s.execute(MOCKUP, json!({"photo": photo, "visuals": []})).is_err(), "no scale");
        assert!(s.execute(MOCKUP, json!({"photo": logo.replace(".png", ".svg"), "pixelsPerCm": 10})).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
