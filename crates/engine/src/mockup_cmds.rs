// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Mockup (DTF Craft, beta): visuals shown on a photo of the garment at their real size.
//!
//! The photo's scale comes from a line drawn on it whose real length is known (`scale`: its two
//! ends in photo pixels and its length in cm) or straight from `pixelsPerCm`. Each visual (a PNG
//! or SVG file, or an open document, flattened and cropped like a gang sheet's) is placed by its
//! centre on the photo, at its print size in cm (by default its own: pixels at its resolution),
//! turned by `rotation` degrees clockwise.
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

use photocraft_color::{ColorMode, SampleType};
use photocraft_doc::{Document, Layer, LayerContent, Size};
use photocraft_geom::Rect;
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

/// The visual drawn at `width_cm × height_cm` on a photo of `ppcm` pixels per cm: RGBA bytes and
/// their size.
pub fn print_pixels(v: &Visual, width_cm: f64, height_cm: f64, ppcm: f64) -> Result<(Vec<u8>, usize, usize)> {
    let tw = (width_cm * ppcm).round() as i64;
    let th = (height_cm * ppcm).round() as i64;
    if tw > MAX_PRINT_SIDE || th > MAX_PRINT_SIDE {
        return Err(bad(format!("{}: too large on this photo", v.name)));
    }
    let (tw, th) = (tw.max(1), th.max(1));
    let surf = gang::render_visual(v, tw, th)?;
    Ok((surf.to_interleaved(Rect::new(0, 0, tw as i32, th as i32)), tw as usize, th as usize))
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

/// One visual on the photo: its pixels (RGBA bytes, `width × height`, at the photo's scale), its
/// centre in photo pixels and its rotation (degrees, clockwise).
pub struct Print<'a> {
    pub rgba: &'a [u8],
    pub width: usize,
    pub height: usize,
    pub center: (f64, f64),
    pub rotation: f64,
}

/// Premultiplied bilinear sample of `p` at `(u, v)` (pixel centres at .5); transparent outside.
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

/// The photo (`rgba`, `width × height`) with the prints on it, in order (the last on top), at
/// `ppcm` pixels per cm.
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
        let (s, c) = p.rotation.to_radians().sin_cos();
        let (hw, hh) = (p.width as f64 / 2.0, p.height as f64 / 2.0);
        // The turned box on the photo, widened by the largest fold shift.
        let corners = [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].map(|(x, y)| (p.center.0 + x * c - y * s, p.center.1 + x * s + y * c));
        let pad = f64::from(shift) + 1.0;
        let x0 = (corners.iter().map(|q| q.0).fold(f64::MAX, f64::min) - pad).floor().max(0.0) as usize;
        let y0 = (corners.iter().map(|q| q.1).fold(f64::MAX, f64::min) - pad).floor().max(0.0) as usize;
        let x1 = ((corners.iter().map(|q| q.0).fold(f64::MIN, f64::max) + pad).ceil().max(0.0) as usize).min(width);
        let y1 = ((corners.iter().map(|q| q.1).fold(f64::MIN, f64::max) + pad).ceil().max(0.0) as usize).min(height);
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
        let (s, c) = (s as f32, c as f32);
        let (cx, cy) = (p.center.0 as f32, p.center.1 as f32);
        let (hw, hh) = (hw as f32, hh as f32);
        for y in y0..y1 {
            for x in x0..x1 {
                let (mut qx, mut qy) = (x as f32 + 0.5, y as f32 + 0.5);
                if shift > 0.0 {
                    let (gx, gy) = light.slope(x, y);
                    qx += gx * shift;
                    qy += gy * shift;
                }
                // Back into the visual: turned the other way about its centre.
                let (dx, dy) = (qx - cx, qy - cy);
                let u = dx * c + dy * s + hw;
                let v = -dx * s + dy * c + hh;
                if u < -1.0 || v < -1.0 || u > p.width as f32 + 1.0 || v > p.height as f32 + 1.0 {
                    continue;
                }
                let px = sample(p, u, v);
                let a = px[3] * opacity;
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
    let mut prints = Vec::new();
    for (i, v) in list.iter().enumerate() {
        check_range(v, "widthCm", 0.1, 500.0)?;
        check_range(v, "heightCm", 0.1, 500.0)?;
        check_range(v, "rotation", -360.0, 360.0)?;
        let visual = load_visual(s, i, v)?;
        let (nw, nh) = natural_size_cm(&visual);
        let (w, h) = match (v.get("widthCm").and_then(Value::as_f64), v.get("heightCm").and_then(Value::as_f64)) {
            (Some(w), Some(h)) => (w, h),
            (Some(w), None) => (w, w * nh / nw.max(1e-9)),
            (None, Some(h)) => (h * nw / nh.max(1e-9), h),
            (None, None) => (nw, nh),
        };
        let (px, pw, ph) = print_pixels(&visual, w, h, ppcm)?;
        let center = (num(v, "x", f64::from(photo.width) / 2.0), num(v, "y", f64::from(photo.height) / 2.0));
        prints.push((px, pw, ph, center, num(v, "rotation", 0.0)));
    }
    let render = Render {
        shadows: (num(p, "shadows", DEFAULT_SHADOWS) / 100.0) as f32,
        folds: (num(p, "folds", DEFAULT_FOLDS) / 100.0) as f32,
        opacity: (num(p, "opacity", DEFAULT_OPACITY) / 100.0) as f32,
    };
    crate::allocation::checkpoint("rendering a mockup")?;
    let (w, h) = (photo.width as usize, photo.height as usize);
    let light = Light::new(&photo.rgba, w, h);
    let views: Vec<Print> =
        prints.iter().map(|(px, pw, ph, center, rotation)| Print { rgba: px, width: *pw, height: *ph, center: *center, rotation: *rotation }).collect();
    let pixels = composite(&photo.rgba, w, h, &light, &views, render, ppcm);
    let name = p.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).map_or_else(|| format!("{} mockup", photo.name), str::to_string);
    let mut doc = Document::new(&name, Size::new(photo.width, photo.height), ColorMode::Rgb, SampleType::U8);
    doc.resolution_dpi = photo.dpi as f32;
    doc.layers.clear();
    let surf = Surface::from_interleaved(gang::format(), Rect::new(0, 0, photo.width as i32, photo.height as i32), &pixels);
    doc.layers.push(Layer::new(name.clone(), LayerContent::Raster(surf)));
    let mut out = json!({"pixels": [photo.width, photo.height], "pixelsPerCm": ppcm, "visuals": prints.len()});
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
        params: r##"{"photo":str (the garment's photo, a PNG or JPG path),"scale":{"from":[x,y],"to":[x,y] (a line on the photo, in its pixels),"cm":0.1..1000 (its real length)}|"pixelsPerCm":0.01..2000,"visuals":[{"path":str (a PNG or SVG)|"document":int,"x":px,"y":px (its centre on the photo; default the photo's centre),"widthCm":0.1..500,"heightCm":0.1..500 (one alone keeps the proportions; default its own print size),"rotation":-360..360=0 (degrees, clockwise)}],"shadows":0..100=60,"folds":0..100=35,"opacity":0..100=95,"name":str,"output":str (a .png or .jpg path),"open":bool=true}"##,
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

    /// Without shadows, folds or transparency, the visual lands as it is, centred where asked,
    /// turned about its centre.
    #[test]
    fn a_print_lands_where_it_is_placed() {
        let (w, h) = (100, 80);
        let photo = flat(w, h, [200, 200, 200]);
        let light = Light::new(&photo, w, h);
        let red = square(20, [220, 30, 30]);
        let print = Print { rgba: &red, width: 20, height: 20, center: (30.0, 40.0), rotation: 0.0 };
        let r = Render { shadows: 0.0, folds: 0.0, opacity: 1.0 };
        let out = composite(&photo, w, h, &light, &[print], r, 10.0);
        assert_eq!(at(&out, w, 30, 40), [220, 30, 30, 255]);
        assert_eq!(at(&out, w, 21, 31), [220, 30, 30, 255]);
        assert_eq!(at(&out, w, 18, 40), [200, 200, 200, 255], "left of it: the photo");
        assert_eq!(at(&out, w, 70, 40), [200, 200, 200, 255]);
        // A 40 × 4 bar turned 90°: upright.
        let bar = flat(40, 4, [0, 0, 0]);
        let print = Print { rgba: &bar, width: 40, height: 4, center: (70.0, 40.0), rotation: 90.0 };
        let out = composite(&photo, w, h, &light, &[print], r, 10.0);
        assert_eq!(at(&out, w, 70, 25)[0], 0, "along the turned bar");
        assert_eq!(at(&out, w, 80, 40)[0], 200, "beside it");
        // Half the opacity: half the fabric shows.
        let r = Render { opacity: 0.5, ..r };
        let out = composite(&photo, w, h, &light, &[Print { rgba: &red, width: 20, height: 20, center: (30.0, 40.0), rotation: 0.0 }], r, 10.0);
        assert_eq!(at(&out, w, 30, 40)[1], 115);
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
        let print = Print { rgba: &ink, width: 40, height: 40, center: (60.0, 30.0), rotation: 0.0 };
        let out = composite(&photo, w, h, &light, &[print], Render { shadows: 1.0, folds: 0.0, opacity: 1.0 }, 10.0);
        let (dark, lit) = (at(&out, w, 45, 30), at(&out, w, 75, 30));
        let sum = |p: [u8; 4]| u32::from(p[0]) + u32::from(p[1]) + u32::from(p[2]);
        assert!(sum(dark) + 40 < sum(lit), "{dark:?} {lit:?}");
        assert!(lit[0] < 120, "a highlight does not wash the ink out: {lit:?}");
        // Folds move the ink along the light: striped ink shifts on the light step.
        let stripes: Vec<u8> = (0..40 * 40).flat_map(|i| if (i % 40) % 4 < 2 { [0, 0, 0, 255] } else { [255, 255, 255, 255] }).collect();
        let at_step = |folds: f32| {
            let print = Print { rgba: &stripes, width: 40, height: 40, center: (60.0, 30.0), rotation: 0.0 };
            composite(&photo, w, h, &light, &[print], Render { shadows: 0.0, folds, opacity: 1.0 }, 10.0)
        };
        let (flat, folded) = (at_step(0.0), at_step(1.0));
        assert!((57..63).any(|x| at(&flat, w, x, 30) != at(&folded, w, x, 30)), "moved on the step");
        assert_eq!(at(&flat, w, 45, 30), at(&folded, w, 45, 30), "not where the fabric is flat");
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
        assert!(s.execute(MOCKUP, json!({"photo": photo, "visuals": []})).is_err(), "no scale");
        assert!(s.execute(MOCKUP, json!({"photo": logo.replace(".png", ".svg"), "pixelsPerCm": 10})).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }
}
