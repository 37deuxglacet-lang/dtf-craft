// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF (direct-to-film) preparation filters (DTF Craft).
//!
//! DTF printers lay a white underbase under the colour inks and cannot print partial opacity:
//! a semi-transparent pixel becomes a full white dot under a faded colour, which shows as a halo
//! or a grey veil on the garment. These filters make every pixel either fully opaque or fully
//! transparent while keeping the colours:
//!
//! * [`alpha_threshold`] (Remove Semi-Transparent Pixels): alpha at or above the threshold
//!   becomes 1, below it 0.
//! * [`halftone`] (DTF Halftone): the coverage a pixel should have (its alpha, possibly scaled by
//!   its lightness for dark or light garments) is rendered as an opaque halftone dot pattern, so
//!   gradients, shadows and soft edges print as dots of solid ink instead of semi-transparency.
//!   The screen is laid out in document coordinates, so tiles and bands join seamlessly.

use photocraft_geom::Rect;
use serde::{Deserialize, Serialize};

use crate::Ctx;
use crate::fxutil::{MAXC, native, ncol, rgba, xy};
use crate::image::Image;
use crate::quantize::{HalftoneShape, spot};

/// What a DTF halftone turns into dots.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum HalftoneTarget {
    /// The transparency only: coverage = alpha (gradients, shadows and soft edges become dots).
    Alpha,
    /// Dark garment: dark areas let the garment show through (coverage = alpha × lightness).
    DarkGarment,
    /// Light garment: light areas let the garment show through (coverage = alpha × darkness).
    LightGarment,
}

impl HalftoneTarget {
    pub fn from_id(s: &str) -> Option<Self> {
        Some(match s {
            "alpha" | "semiTransparent" => HalftoneTarget::Alpha,
            "darkGarment" => HalftoneTarget::DarkGarment,
            "lightGarment" => HalftoneTarget::LightGarment,
            _ => return None,
        })
    }
}

/// DTF halftone settings (the filter's [`crate::FilterParams::DtfHalftone`] fields).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HalftoneSpec {
    /// Screen cell size in pixels (document resolution ÷ lines per inch).
    pub cell: f32,
    /// Screen angle in degrees.
    pub angle: f32,
    pub shape: HalftoneShape,
    pub target: HalftoneTarget,
    /// Coverage (0–1) below which no dot is printed: dots too small to hold on the film.
    pub min_dot: f32,
    /// Coverage (0–1) at or above which the area prints solid.
    pub max_dot: f32,
    /// Tone range of the coverage before screening: `low`..`high` (0–1) is stretched to 0..1.
    pub low: f32,
    pub high: f32,
    /// Gamma applied to the stretched coverage (1 = linear; < 1 darker dots, > 1 lighter).
    pub gamma: f32,
}

impl Default for HalftoneSpec {
    fn default() -> Self {
        HalftoneSpec {
            cell: 12.0,
            angle: 22.5,
            shape: HalftoneShape::Round,
            target: HalftoneTarget::Alpha,
            min_dot: 0.05,
            max_dot: 0.95,
            low: 0.0,
            high: 1.0,
            gamma: 1.0,
        }
    }
}

/// Sorted spot-function samples: the rank of a spot value is the share of the cell a dot covers
/// when it reaches that value, so coverage is proportional to the tone.
fn spot_table(shape: HalftoneShape) -> Vec<f32> {
    let mut table: Vec<f32> = (0..64 * 64).map(|k| spot(shape, ((k % 64) as f32 + 0.5) / 32.0 - 1.0, ((k / 64) as f32 + 0.5) / 32.0 - 1.0)).collect();
    table.sort_by(f32::total_cmp);
    table
}

/// Perceived lightness (Rec. 709 luma of straight sRGB), 0–1.
fn lightness(c: [f32; 4]) -> f32 {
    (0.2126 * c[0] + 0.7152 * c[1] + 0.0722 * c[2]).clamp(0.0, 1.0)
}

/// The coverage (0–1) a pixel should print with, before the min/max dot limits.
pub fn coverage(spec: &HalftoneSpec, colour: [f32; 4], alpha: f32) -> f32 {
    let a = alpha.clamp(0.0, 1.0);
    let raw = match spec.target {
        HalftoneTarget::Alpha => a,
        HalftoneTarget::DarkGarment => a * lightness(colour),
        HalftoneTarget::LightGarment => a * (1.0 - lightness(colour)),
    };
    let (lo, hi) = (spec.low.clamp(0.0, 1.0), spec.high.clamp(0.0, 1.0));
    let stretched = if hi > lo { ((raw - lo) / (hi - lo)).clamp(0.0, 1.0) } else if raw >= lo { 1.0 } else { 0.0 };
    let g = if spec.gamma.is_finite() && spec.gamma > 0.0 { spec.gamma } else { 1.0 };
    if stretched <= 0.0 || stretched >= 1.0 { stretched } else { stretched.powf(g) }
}

/// Whether the pixel at document position `(x, y)` (its centre) is inside the dot for `cov`.
fn inside(spec: &HalftoneSpec, table: &[f32], sincos: (f32, f32), x: f32, y: f32, cov: f32) -> bool {
    let cell = if spec.cell.is_finite() { spec.cell.max(2.0) } else { 2.0 };
    let (s, c) = sincos;
    let (rx, ry) = (x * c + y * s, -x * s + y * c);
    let u = (rx / cell).rem_euclid(1.0) * 2.0 - 1.0;
    let v = (ry / cell).rem_euclid(1.0) * 2.0 - 1.0;
    // The share of the cell whose spot value is above this pixel's: the dot grows from the
    // highest spot values, so the pixel is inked once that share is below the coverage.
    let sv = spot(spec.shape, u, v);
    let above = 1.0 - table.partition_point(|t| *t < sv) as f32 / table.len().max(1) as f32;
    above < cov
}

/// The DTF Halftone kernel. Colour channels are kept; alpha becomes 0 or 1. Surfaces without
/// alpha (a channel or Quick Mask target) are screened on their single value instead.
pub(crate) fn halftone(src: &Image, out: Rect, ctx: &Ctx, spec: &HalftoneSpec) -> Vec<f32> {
    let n = src.ch;
    let mut res = src.crop(out);
    if n == 0 {
        return res;
    }
    let table = spot_table(spec.shape);
    let sincos = {
        let a = if spec.angle.is_finite() { spec.angle } else { 0.0 };
        let (s, c) = a.to_radians().sin_cos();
        (s, c)
    };
    let (min_dot, max_dot) = (spec.min_dot.clamp(0.0, 1.0), spec.max_dot.clamp(0.0, 1.0));
    for (i, px) in res.chunks_exact_mut(n).enumerate() {
        let (x, y) = xy(out, i);
        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
        if ctx.alpha && n >= 2 {
            let alpha = px.get(n - 1).copied().unwrap_or(0.0);
            if alpha <= 0.0 {
                continue;
            }
            let cov = coverage(spec, rgba(ctx, px), alpha);
            let on = if cov < min_dot || cov <= 0.0 {
                false
            } else if cov >= max_dot {
                true
            } else {
                inside(spec, &table, sincos, fx, fy, cov)
            };
            if let Some(a) = px.last_mut() {
                *a = if on { 1.0 } else { 0.0 };
            }
        } else {
            // A single-channel target (mask, alpha channel): screen its value directly.
            for v in px.iter_mut() {
                let cov = v.clamp(0.0, 1.0);
                let on = cov >= max_dot || (cov >= min_dot && cov > 0.0 && inside(spec, &table, sincos, fx, fy, cov));
                *v = if on { 1.0 } else { 0.0 };
            }
        }
    }
    res
}

/// The Remove Semi-Transparent Pixels kernel: alpha `>= threshold` → 1, else 0 (`threshold` 0–1,
/// at least one 8-bit step). Colour channels are unchanged.
pub(crate) fn alpha_threshold(src: &Image, out: Rect, ctx: &Ctx, threshold: f32) -> Vec<f32> {
    let n = src.ch;
    let mut res = src.crop(out);
    let t = if threshold.is_finite() { threshold.clamp(1.0 / 255.0, 1.0) } else { 0.5 };
    if ctx.alpha && n >= 2 {
        for px in res.chunks_exact_mut(n) {
            if let Some(a) = px.last_mut() {
                *a = if *a >= t { 1.0 } else { 0.0 };
            }
        }
    } else {
        for v in res.iter_mut() {
            *v = if *v >= t { 1.0 } else { 0.0 };
        }
    }
    res
}

// ---------------------------------------------------------------- Remove White Background

/// Remove White Background settings (the filter's [`crate::FilterParams::DtfCutout`] fields).
///
/// The background is the plain colour the visual sits on (white for most files). It is removed
/// from the image edges inwards, so the same colour inside the visual stays, and the contour
/// pixels mixed with it are unmixed (see [`crate::color_to_alpha`]) so no light halo is left.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CutoutSpec {
    /// The background colour (straight sRGB 0–1), or `None` to read it on the image edges.
    pub color: Option<[f32; 3]>,
    /// Largest channel difference (0–1) from the background colour still taken as background.
    pub tolerance: f32,
    /// Width in pixels of the contour band that is unmixed from the background (0 = none).
    pub edge: f32,
    /// Unmix the contour band from the background colour (otherwise it stays as it is).
    pub decontaminate: bool,
    /// Also remove background-coloured areas enclosed by the visual (inside an O, between
    /// shapes) of at least `min_hole` pixels.
    pub holes: bool,
    pub min_hole: u32,
}

impl Default for CutoutSpec {
    fn default() -> Self {
        CutoutSpec { color: None, tolerance: 0.1, edge: 2.0, decontaminate: true, holes: false, min_hole: 100 }
    }
}

/// What the cutout does to each pixel of the area it was prepared for.
const KEEP: u8 = 0;
const BACKGROUND: u8 = 1;
const CONTOUR: u8 = 2;

/// A cutout prepared once for the whole reference area (the flood fill needs every pixel), then
/// applied tile by tile by [`CutoutPlan::apply`].
#[derive(Clone, Debug)]
pub struct CutoutPlan {
    rect: Rect,
    /// The background colour in the samples' own colour model (colour channels only).
    key: Vec<f32>,
    class: Vec<u8>,
    /// For contour pixels: the colour of the nearest kept pixel of the visual (its own colour,
    /// which the contour pixel is a mix of with the background).
    inner: std::collections::HashMap<usize, [f32; MAXC]>,
    tolerance: f32,
    decontaminate: bool,
}

/// Largest channel difference between a pixel's colour channels and `key`.
fn distance(colour: &[f32], key: &[f32]) -> f32 {
    colour.iter().zip(key).map(|(p, k)| (p - k).abs()).fold(0.0, f32::max)
}

/// The background colour read on the image edges: the per-channel median of the opaque pixels
/// of the outer ring. `None` when the ring has no opaque pixel (nothing to remove).
fn edge_colour(src: &Image, cc: usize) -> Option<Vec<f32>> {
    let n = src.ch;
    let (w, h) = (src.rect.width() as usize, src.rect.height() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    let mut samples: Vec<Vec<f32>> = vec![Vec::new(); cc];
    let mut take = |i: usize| {
        let Some(px) = src.data.get(i * n..i * n + n) else { return };
        if px.get(n - 1).copied().unwrap_or(0.0) < 0.5 {
            return;
        }
        for (c, s) in samples.iter_mut().enumerate() {
            if let Some(v) = px.get(c).filter(|v| v.is_finite()) {
                s.push(*v);
            }
        }
    };
    for x in 0..w {
        take(x);
        if h > 1 {
            take((h - 1) * w + x);
        }
    }
    for y in 1..h.saturating_sub(1) {
        take(y * w);
        if w > 1 {
            take(y * w + w - 1);
        }
    }
    let mut key = Vec::with_capacity(cc);
    for mut s in samples {
        if s.is_empty() {
            return None;
        }
        s.sort_by(f32::total_cmp);
        key.push(s[s.len() / 2]);
    }
    Some(key)
}

impl CutoutPlan {
    /// Prepares the cutout of `src` (the whole reference area). `None` when cancelled.
    pub fn prepare(src: &Image, ctx: &Ctx, spec: &CutoutSpec, ctl: &photocraft_raster::Interrupt) -> Option<CutoutPlan> {
        let n = src.ch;
        let (w, h) = (src.rect.width() as usize, src.rect.height() as usize);
        let tolerance = if spec.tolerance.is_finite() { spec.tolerance.clamp(0.0, 1.0) } else { 0.1 };
        let mut plan = CutoutPlan {
            rect: src.rect,
            key: Vec::new(),
            class: Vec::new(),
            inner: std::collections::HashMap::new(),
            tolerance,
            decontaminate: spec.decontaminate,
        };
        // Without alpha there is nowhere to put the transparency: the plan keeps every pixel.
        if !ctx.alpha || n < 2 || w == 0 || h == 0 || src.data.len() < w * h * n {
            return Some(plan);
        }
        let cc = ncol(ctx, n).min(MAXC);
        let key = match spec.color {
            Some(c) => native(ctx, [c[0], c[1], c[2], 1.0]).get(..cc).map(<[f32]>::to_vec),
            None => edge_colour(src, cc),
        };
        let Some(key) = key else { return Some(plan) };
        let is_background = |i: usize| -> bool {
            let px = &src.data[i * n..i * n + n];
            px[n - 1] <= 0.0 || distance(&px[..cc], &key) <= tolerance
        };
        let mut class = vec![KEEP; w * h];
        // 1. The background reached from the image edges (4-connected flood fill).
        let mut stack: Vec<usize> = Vec::new();
        let seed = |i: usize, class: &mut Vec<u8>, stack: &mut Vec<usize>| {
            if class[i] == KEEP && is_background(i) {
                class[i] = BACKGROUND;
                stack.push(i);
            }
        };
        for x in 0..w {
            seed(x, &mut class, &mut stack);
            seed((h - 1) * w + x, &mut class, &mut stack);
        }
        for y in 0..h {
            seed(y * w, &mut class, &mut stack);
            seed(y * w + w - 1, &mut class, &mut stack);
        }
        let mut steps = 0usize;
        while let Some(i) = stack.pop() {
            steps += 1;
            if steps % (1 << 20) == 0 && ctl.cancelled() {
                return None;
            }
            let (x, y) = (i % w, i / w);
            let mut visit = |j: usize| {
                if class[j] == KEEP && is_background(j) {
                    class[j] = BACKGROUND;
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
        // 2. Enclosed areas of the background colour, when asked, from a minimum size.
        if spec.holes {
            const SEEN: u8 = 3;
            let mut region: Vec<usize> = Vec::new();
            for start in 0..w * h {
                if class[start] != KEEP || !is_background(start) {
                    continue;
                }
                if ctl.cancelled() {
                    return None;
                }
                region.clear();
                class[start] = SEEN;
                stack.push(start);
                while let Some(i) = stack.pop() {
                    region.push(i);
                    let (x, y) = (i % w, i / w);
                    let mut visit = |j: usize| {
                        if class[j] == KEEP && is_background(j) {
                            class[j] = SEEN;
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
                let to = if region.len() as u64 >= u64::from(spec.min_hole.max(1)) { BACKGROUND } else { SEEN };
                for &i in &region {
                    class[i] = to;
                }
            }
            for c in class.iter_mut().filter(|c| **c == SEEN) {
                *c = KEEP;
            }
        }
        // 3. The contour: kept pixels within `edge` pixels of the removed background (8-connected
        // distance, grown one ring at a time).
        let rings = if spec.edge.is_finite() && spec.edge > 0.0 { spec.edge.ceil().min(64.0) as usize } else { 0 };
        if rings > 0 {
            let mut front: Vec<usize> = (0..w * h).filter(|&i| class[i] == BACKGROUND).collect();
            for _ in 0..rings {
                if ctl.cancelled() {
                    return None;
                }
                let mut next = Vec::new();
                for &i in &front {
                    let (x, y) = ((i % w) as i64, (i / w) as i64);
                    for dy in -1i64..=1 {
                        for dx in -1i64..=1 {
                            let (nx, ny) = (x + dx, y + dy);
                            if (dx, dy) == (0, 0) || nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                                continue;
                            }
                            let j = ny as usize * w + nx as usize;
                            if class[j] == KEEP {
                                class[j] = CONTOUR;
                                next.push(j);
                            }
                        }
                    }
                }
                front = next;
            }
        }
        // 4. Each contour pixel's visual colour: the nearest kept pixel's, carried outwards
        // through the contour. A contour pixel no kept pixel reaches (a stroke thinner than the
        // contour) has none and is unmixed from the background alone.
        let mut inner = std::collections::HashMap::new();
        if rings > 0 && spec.decontaminate {
            let colour_of = |i: usize| {
                let mut c = [0.0f32; MAXC];
                c[..cc].copy_from_slice(&src.data[i * n..i * n + cc]);
                c
            };
            let neighbours = |i: usize| {
                let (x, y) = ((i % w) as i64, (i / w) as i64);
                (-1i64..=1).flat_map(move |dy| (-1i64..=1).map(move |dx| (x + dx, y + dy))).filter_map(move |(nx, ny)| {
                    (nx >= 0 && ny >= 0 && nx < w as i64 && ny < h as i64 && (nx, ny) != (x, y)).then(|| ny as usize * w + nx as usize)
                })
            };
            let mut front: Vec<usize> = Vec::new();
            for i in (0..w * h).filter(|&i| class[i] == CONTOUR) {
                if let Some(j) = neighbours(i).find(|&j| class[j] == KEEP) {
                    inner.insert(i, colour_of(j));
                    front.push(i);
                }
            }
            while !front.is_empty() {
                if ctl.cancelled() {
                    return None;
                }
                let mut next = Vec::new();
                for &i in &front {
                    let Some(c) = inner.get(&i).copied() else { continue };
                    for j in neighbours(i) {
                        if class[j] == CONTOUR && !inner.contains_key(&j) {
                            inner.insert(j, c);
                            next.push(j);
                        }
                    }
                }
                front = next;
            }
        }
        plan.key = key;
        plan.class = class;
        plan.inner = inner;
        Some(plan)
    }

    /// The cutout of `out` (inside the prepared area; pixels outside it are kept).
    pub fn apply(&self, src: &Image, out: Rect, ctx: &Ctx) -> Vec<f32> {
        let n = src.ch;
        let mut res = src.crop(out);
        if self.class.is_empty() || n < 2 {
            return res;
        }
        let cc = ncol(ctx, n).min(MAXC).min(self.key.len());
        let w = self.rect.width() as usize;
        for (i, px) in res.chunks_exact_mut(n).enumerate() {
            let (x, y) = xy(out, i);
            if !self.rect.contains(x, y) {
                continue;
            }
            let k = (y - self.rect.y0) as usize * w + (x - self.rect.x0) as usize;
            match self.class.get(k).copied().unwrap_or(KEEP) {
                BACKGROUND => {
                    if let Some(a) = px.last_mut() {
                        *a = 0.0;
                    }
                }
                CONTOUR if self.decontaminate => {
                    let Some((alpha, colour)) = px.split_last_mut() else { continue };
                    let colour = &mut colour[..cc];
                    let key = &self.key[..cc];
                    *alpha *= match self.inner.get(&k) {
                        Some(f) => unmix_towards(colour, key, &f[..cc], self.tolerance),
                        None => crate::color_to_alpha::unmix(colour, key, self.tolerance, 1.0),
                    };
                }
                _ => {}
            }
        }
        res
    }
}

/// Unmixes a contour pixel `p` taken as `a·F + (1 − a)·K` (`F` the visual's colour next to it,
/// `K` the background): `a` is the projection of `p − K` on `F − K`, the colour becomes
/// `K + (p − K) / a`. A pixel within `tolerance` of the background becomes transparent; one as
/// far as `F` (or a visual of the background's own colour) stays as it is.
fn unmix_towards(colour: &mut [f32], key: &[f32], inner: &[f32], tolerance: f32) -> f32 {
    if distance(colour, key) <= tolerance {
        colour.copy_from_slice(key);
        return 0.0;
    }
    let (mut dot, mut len) = (0.0f32, 0.0f32);
    for ((p, k), f) in colour.iter().zip(key).zip(inner) {
        dot += (p - k) * (f - k);
        len += (f - k) * (f - k);
    }
    let a = if len > 1e-6 { (dot / len).clamp(0.0, 1.0) } else { 1.0 };
    if !a.is_finite() || a >= 1.0 {
        return 1.0;
    }
    if a <= 0.0 {
        colour.copy_from_slice(key);
        return 0.0;
    }
    for (p, &k) in colour.iter_mut().zip(key) {
        *p = (k + (*p - k) / a).clamp(0.0, 1.0);
    }
    a
}

/// The Remove White Background kernel for one call (prepares the cutout from `src`, which covers
/// the reference area). [`crate::apply`] prepares it once for all its tiles instead.
pub(crate) fn cutout(src: &Image, out: Rect, ctx: &Ctx, spec: &CutoutSpec) -> Vec<f32> {
    match CutoutPlan::prepare(src, ctx, spec, &photocraft_raster::Interrupt::NONE) {
        Some(plan) => plan.apply(src, out, ctx),
        None => src.crop(out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::ColorMode;

    fn ctx() -> Ctx {
        Ctx { bounds: Rect::new(0, 0, 256, 64), mode: ColorMode::Rgb, alpha: true }
    }

    /// A horizontal alpha ramp 0 → 1 over 256 px, 64 px high, colour `rgb`.
    fn ramp(rgb: [f32; 3]) -> Image {
        let r = Rect::new(0, 0, 256, 64);
        let mut data = Vec::new();
        for _y in 0..64 {
            for x in 0..256 {
                data.extend_from_slice(&[rgb[0], rgb[1], rgb[2], x as f32 / 255.0]);
            }
        }
        Image { rect: r, ch: 4, data }
    }

    fn alphas(v: &[f32]) -> Vec<f32> {
        v.chunks_exact(4).map(|p| p[3]).collect()
    }

    #[test]
    fn threshold_leaves_only_opaque_or_transparent_and_keeps_colours() {
        let src = ramp([0.2, 0.4, 0.6]);
        let out = alpha_threshold(&src, src.rect, &ctx(), 0.5);
        assert!(alphas(&out).iter().all(|a| *a == 0.0 || *a == 1.0));
        // 127/255 < 0.5 → 0, 128/255 ≥ 0.5 → 1 (the 8-bit 50 % split).
        assert_eq!(out[127 * 4 + 3], 0.0);
        assert_eq!(out[128 * 4 + 3], 1.0);
        assert!(out.chunks_exact(4).all(|p| p[0] == 0.2 && p[1] == 0.4 && p[2] == 0.6));
    }

    #[test]
    fn halftone_is_binary_and_coverage_follows_the_tone() {
        let spec = HalftoneSpec { cell: 8.0, angle: 0.0, min_dot: 0.0, max_dot: 1.0, ..HalftoneSpec::default() };
        let src = ramp([1.0, 0.0, 0.0]);
        let out = halftone(&src, src.rect, &ctx(), &spec);
        let a = alphas(&out);
        assert!(a.iter().all(|v| *v == 0.0 || *v == 1.0), "only opaque or transparent pixels");
        // Average coverage over 32-px wide bands tracks the ramp within a few percent.
        for band in 0..8 {
            let (x0, x1) = (band * 32, band * 32 + 32);
            let mut sum = 0.0;
            for y in 0..64 {
                for x in x0..x1 {
                    sum += a[y * 256 + x];
                }
            }
            let got = sum / (32.0 * 64.0);
            let want = (x0 as f32 + 15.5) / 255.0;
            assert!((got - want).abs() < 0.08, "band {band}: {got} vs {want}");
        }
        // Colours never change.
        assert!(out.chunks_exact(4).all(|p| p[0] == 1.0 && p[1] == 0.0 && p[2] == 0.0));
    }

    #[test]
    fn min_and_max_dot_drop_specks_and_fill_solids() {
        let spec = HalftoneSpec { cell: 8.0, min_dot: 0.2, max_dot: 0.8, ..HalftoneSpec::default() };
        let src = ramp([0.0, 0.0, 0.0]);
        let a = alphas(&halftone(&src, src.rect, &ctx(), &spec));
        for y in 0..64 {
            for x in 0..40 {
                assert_eq!(a[y * 256 + x], 0.0, "below min dot: empty");
            }
            for x in 210..256 {
                assert_eq!(a[y * 256 + x], 1.0, "above max dot: solid");
            }
        }
    }

    #[test]
    fn garment_modes_use_lightness() {
        let dark = HalftoneSpec { target: HalftoneTarget::DarkGarment, ..HalftoneSpec::default() };
        let light = HalftoneSpec { target: HalftoneTarget::LightGarment, ..HalftoneSpec::default() };
        // Opaque black: nothing on a dark garment, solid on a light one.
        assert_eq!(coverage(&dark, [0.0, 0.0, 0.0, 1.0], 1.0), 0.0);
        assert_eq!(coverage(&light, [0.0, 0.0, 0.0, 1.0], 1.0), 1.0);
        // Opaque white: the reverse.
        assert_eq!(coverage(&dark, [1.0, 1.0, 1.0, 1.0], 1.0), 1.0);
        assert_eq!(coverage(&light, [1.0, 1.0, 1.0, 1.0], 1.0), 0.0);
    }

    #[test]
    fn screen_is_seamless_across_bands() {
        let spec = HalftoneSpec { cell: 7.3, angle: 37.0, ..HalftoneSpec::default() };
        let src = ramp([0.5, 0.5, 0.5]);
        let whole = halftone(&src, src.rect, &ctx(), &spec);
        let top = halftone(&src, Rect::new(0, 0, 256, 20), &ctx(), &spec);
        let bottom = halftone(&src, Rect::new(0, 20, 256, 64), &ctx(), &spec);
        let mut joined = top;
        joined.extend(bottom);
        assert_eq!(whole, joined);
    }

    /// 40 × 40 on a slightly noisy white (0.97–1): a black square ring from 10 to 30 enclosing a
    /// white 10 × 10 hole, and one grey (0.7) contour pixel at (9, 20) against the background.
    fn logo(bg: [f32; 3]) -> Image {
        let r = Rect::new(0, 0, 40, 40);
        let mut data = Vec::new();
        for y in 0..40 {
            for x in 0..40 {
                let ring = (10..30).contains(&x) && (10..30).contains(&y) && !((15..25).contains(&x) && (15..25).contains(&y));
                let hole = (15..25).contains(&x) && (15..25).contains(&y);
                let c = if ring {
                    [0.0; 3]
                } else if hole {
                    [1.0; 3]
                } else if (x, y) == (9, 20) {
                    [0.7; 3]
                } else {
                    let n = if (x + y) % 3 == 0 { 0.03 } else { 0.0 };
                    [bg[0] - n, bg[1] - n, bg[2] - n]
                };
                data.extend_from_slice(&[c[0], c[1], c[2], 1.0]);
            }
        }
        Image { rect: r, ch: 4, data }
    }

    fn cut(img: &Image, spec: CutoutSpec) -> Vec<f32> {
        let ctx = Ctx { bounds: img.rect, mode: ColorMode::Rgb, alpha: true };
        let plan = CutoutPlan::prepare(img, &ctx, &spec, &photocraft_raster::Interrupt::NONE).unwrap();
        plan.apply(img, img.rect, &ctx)
    }

    fn alpha_at(v: &[f32], x: usize, y: usize) -> f32 {
        v[(y * 40 + x) * 4 + 3]
    }

    #[test]
    fn cutout_removes_the_outer_white_and_keeps_the_white_inside() {
        let out = cut(&logo([1.0; 3]), CutoutSpec::default());
        // The background, JPEG-like noise included, is gone.
        for (x, y) in [(0, 0), (3, 7), (39, 39), (5, 35)] {
            assert_eq!(alpha_at(&out, x, y), 0.0, "({x}, {y})");
        }
        // The visual stays, and so does the white enclosed by it (interior holes are off).
        assert_eq!(alpha_at(&out, 12, 12), 1.0);
        assert_eq!(alpha_at(&out, 20, 20), 1.0);
        assert_eq!(&out[(20 * 40 + 20) * 4..(20 * 40 + 20) * 4 + 3], &[1.0, 1.0, 1.0]);
    }

    #[test]
    fn cutout_unmixes_the_contour_from_the_background() {
        let out = cut(&logo([1.0; 3]), CutoutSpec::default());
        // 0.7 grey next to black on white = black at 30 % coverage.
        let a = alpha_at(&out, 9, 20);
        assert!((a - 0.3).abs() < 1e-4, "{a}");
        let i = (20 * 40 + 9) * 4;
        assert!(out[i..i + 3].iter().all(|v| *v < 1e-4), "the halo is black, not grey: {:?}", &out[i..i + 3]);
        // Without decontamination the contour pixel is kept as it was.
        let raw = cut(&logo([1.0; 3]), CutoutSpec { decontaminate: false, ..CutoutSpec::default() });
        assert_eq!(alpha_at(&raw, 9, 20), 1.0);
        assert_eq!(raw[i], 0.7);
    }

    #[test]
    fn a_light_visual_on_white_keeps_its_opaque_edge() {
        // Pale yellow (1, 1, 0.6) touching the white: its edge is no mix with the background.
        let r = Rect::new(0, 0, 40, 40);
        let mut data = Vec::new();
        for y in 0..40 {
            for x in 0..40 {
                let inside = (10..30).contains(&x) && (10..30).contains(&y);
                let blue = if inside { 0.6 } else if (x, y) == (9, 20) { 0.8 } else { 1.0 };
                data.extend_from_slice(&[1.0, 1.0, blue, 1.0]);
            }
        }
        let out = cut(&Image { rect: r, ch: 4, data }, CutoutSpec::default());
        assert_eq!(alpha_at(&out, 10, 20), 1.0, "the yellow edge stays opaque");
        assert_eq!(alpha_at(&out, 12, 20), 1.0);
        // The antialiased pixel halfway to white is half yellow.
        let a = alpha_at(&out, 9, 20);
        assert!((a - 0.5).abs() < 1e-4, "{a}");
        let i = (20 * 40 + 9) * 4;
        assert!((out[i + 2] - 0.6).abs() < 1e-4, "its colour is the yellow's: {:?}", &out[i..i + 3]);
        assert_eq!(alpha_at(&out, 0, 0), 0.0);
    }

    #[test]
    fn cutout_holes_follow_their_minimum_size() {
        let open = cut(&logo([1.0; 3]), CutoutSpec { holes: true, min_hole: 50, ..CutoutSpec::default() });
        assert_eq!(alpha_at(&open, 20, 20), 0.0, "the 100 px hole is removed");
        assert_eq!(alpha_at(&open, 12, 12), 1.0);
        let kept = cut(&logo([1.0; 3]), CutoutSpec { holes: true, min_hole: 200, ..CutoutSpec::default() });
        assert_eq!(alpha_at(&kept, 20, 20), 1.0, "smaller than the minimum: kept");
    }

    #[test]
    fn cutout_reads_a_plain_background_of_any_colour_or_the_one_given() {
        let green = logo([0.2, 0.8, 0.3]);
        let out = cut(&green, CutoutSpec::default());
        assert_eq!(alpha_at(&out, 0, 0), 0.0);
        assert_eq!(alpha_at(&out, 12, 12), 1.0);
        // Given as white, the green is no background: nothing is removed.
        let white = cut(&green, CutoutSpec { color: Some([1.0; 3]), ..CutoutSpec::default() });
        assert_eq!(alpha_at(&white, 0, 0), 1.0);
    }

    #[test]
    fn cutout_is_the_same_whatever_the_tiling() {
        use photocraft_color::{PixelFormat, SampleType};
        let img = logo([1.0; 3]);
        let mut s = photocraft_raster::Surface::new(PixelFormat::new(ColorMode::Rgb, SampleType::U8, true));
        s.write_region(img.rect, &img.data);
        let p = crate::FilterParams::DtfCutout { spec: CutoutSpec::default() };
        let a = crate::apply_tiled(&s, &p, img.rect, img.rect, None, 256, None);
        let b = crate::apply_tiled(&s, &p, img.rect, img.rect, None, 7, None);
        assert_eq!(a.read_region(img.rect), b.read_region(img.rect));
        assert_eq!(a.pixel(0, 0)[3], 0.0);
        assert_eq!(a.pixel(12, 12)[3], 1.0);
    }

    #[test]
    fn hostile_values_do_not_panic() {
        let img = logo([1.0; 3]);
        let _ = cut(&img, CutoutSpec { tolerance: f32::NAN, edge: f32::INFINITY, min_hole: 0, holes: true, ..CutoutSpec::default() });
        let empty = Image { rect: Rect::EMPTY, ch: 4, data: vec![] };
        let _ = cut(&empty, CutoutSpec::default());
        let spec = HalftoneSpec { cell: f32::NAN, angle: f32::INFINITY, gamma: -3.0, low: 2.0, high: -1.0, min_dot: f32::NAN, ..HalftoneSpec::default() };
        let src = ramp([0.5, 0.5, 0.5]);
        let _ = halftone(&src, src.rect, &ctx(), &spec);
        let _ = alpha_threshold(&src, src.rect, &ctx(), f32::NAN);
        let empty = Image { rect: Rect::EMPTY, ch: 4, data: vec![] };
        assert!(halftone(&empty, Rect::EMPTY, &ctx(), &spec).is_empty());
    }
}
