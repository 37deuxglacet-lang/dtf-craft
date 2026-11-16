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

/// The fringe width Remove Semi-Transparent Pixels treats when none is given, in px.
pub fn default_fringe_width() -> f32 {
    2.0
}

/// The Remove Semi-Transparent Pixels kernel, on the fringe only: a semi-transparent pixel at
/// most `width` px from a fully transparent one (around the visual, or in a hole) becomes opaque
/// when its alpha is `>= threshold` (0–1, at least one 8-bit step), transparent below it. Pixels
/// further inside (soft shadows, gradients, antialiasing between two colours) are left alone, and
/// so is a layer without transparency. Colour channels are unchanged.
pub(crate) fn alpha_threshold(src: &Image, out: Rect, ctx: &Ctx, threshold: f32, width: f32) -> Vec<f32> {
    let n = src.ch;
    let mut res = src.crop(out);
    if !ctx.alpha || n < 2 {
        return res;
    }
    let t = if threshold.is_finite() { threshold.clamp(1.0 / 255.0, 1.0) } else { 0.5 };
    let width = if width.is_finite() { width.clamp(1.0, 64.0) } else { 2.0 };
    // Distances to the transparency, over the source (it reaches `width` px around `out`).
    let (w, h) = (src.rect.width() as usize, src.rect.height() as usize);
    if src.data.len() < w * h * n {
        return res;
    }
    let clear: Vec<bool> = (0..w * h).map(|i| src.data[i * n + n - 1] < 0.5 / 255.0).collect();
    if !clear.iter().any(|c| *c) {
        return res;
    }
    let dist = crate::selection::edt(&clear, w, h);
    for (i, px) in res.chunks_exact_mut(n).enumerate() {
        let (x, y) = crate::fxutil::xy(out, i);
        if !src.rect.contains(x, y) {
            continue;
        }
        let k = (y - src.rect.y0) as usize * w + (x - src.rect.x0) as usize;
        let Some(a) = px.last_mut() else { continue };
        if *a > 0.0 && *a < 1.0 && dist.get(k).is_some_and(|d| *d <= width + 1e-3) {
            *a = if *a >= t { 1.0 } else { 0.0 };
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
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
    /// Holes clicked by hand (document coordinates): each removes the area of the clicked
    /// pixel's colour reached from it, its contour unmixed from that colour, along with the
    /// background (at most [`MAX_HOLES`]).
    #[serde(default)]
    pub seeds: Vec<[i32; 2]>,
}

/// The most holes one cut-out takes by hand.
pub const MAX_HOLES: usize = 250;

impl Default for CutoutSpec {
    fn default() -> Self {
        CutoutSpec { color: None, tolerance: 0.1, edge: 2.0, decontaminate: true, holes: false, min_hole: 100, seeds: Vec::new() }
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
    /// The colours removed, in the samples' own colour model (colour channels only): the
    /// background's, then each hole's.
    keys: Vec<Vec<f32>>,
    /// Which of `keys` each removed or contour pixel was removed from or is unmixed against
    /// (empty: all the first).
    owner: Vec<u8>,
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

/// The plain background read on the image edges: the per-channel median of the outer ring, when
/// that ring is a plain colour. A transparent margin around the image (Print Conformity leaves
/// 2 px) has no opaque pixel there: the ring of the opaque pixels' bounds is read instead, again
/// only when it is plain. `None` when neither is (an already transparent file, whose bounds run
/// through the visual's own outline): nothing is removed.
pub fn plain_background(src: &Image, ctx: &Ctx, tolerance: f32) -> Option<Vec<f32>> {
    if !ctx.alpha || src.ch < 2 {
        return None;
    }
    edge_colour(src, ncol(ctx, src.ch).min(MAXC), tolerance)
}

fn edge_colour(src: &Image, cc: usize, tolerance: f32) -> Option<Vec<f32>> {
    let n = src.ch;
    let (w, h) = (src.rect.width() as usize, src.rect.height() as usize);
    if w == 0 || h == 0 {
        return None;
    }
    if let Some(key) = ring_colour(src, cc, (0, 0, w, h), tolerance) {
        return Some(key);
    }
    let opaque = |i: usize| src.data.get(i * n + n - 1).is_some_and(|a| *a >= 0.5);
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if opaque(y * w + x) {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return None;
    }
    ring_colour(src, cc, (x0, y0, x1, y1), tolerance)
}

/// The share of a ring's pixels that must be opaque, and of those that must be within the
/// tolerance of its median colour, for it to be a plain background.
const PLAIN_OPAQUE: f32 = 0.9;
const PLAIN_SAME: f32 = 0.85;

/// The per-channel median of the opaque pixels on the border of `(x0, y0, x1, y1)`, when that
/// border is a plain colour: nearly all opaque, nearly all within `tolerance` of the median.
fn ring_colour(src: &Image, cc: usize, (x0, y0, x1, y1): (usize, usize, usize, usize), tolerance: f32) -> Option<Vec<f32>> {
    let n = src.ch;
    let w = src.rect.width() as usize;
    let mut samples: Vec<Vec<f32>> = vec![Vec::new(); cc];
    let mut pixels: Vec<usize> = Vec::new();
    let mut total = 0usize;
    let mut take = |i: usize| {
        total += 1;
        let Some(px) = src.data.get(i * n..i * n + n) else { return };
        if px.get(n - 1).copied().unwrap_or(0.0) < 0.5 {
            return;
        }
        pixels.push(i);
        for (c, s) in samples.iter_mut().enumerate() {
            if let Some(v) = px.get(c).filter(|v| v.is_finite()) {
                s.push(*v);
            }
        }
    };
    for x in x0..x1 {
        take(y0 * w + x);
        if y1 - y0 > 1 {
            take((y1 - 1) * w + x);
        }
    }
    for y in y0 + 1..y1.saturating_sub(1) {
        take(y * w + x0);
        if x1 - x0 > 1 {
            take(y * w + x1 - 1);
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
    if (pixels.len() as f32) < PLAIN_OPAQUE * total as f32 {
        return None;
    }
    // JPEG noise and a faint gradient pass; a ring through the visual's colours doesn't.
    let tol = if tolerance.is_finite() { tolerance.clamp(0.04, 1.0) } else { 0.1 };
    let same = pixels.iter().filter(|&&i| distance(&src.data[i * n..i * n + cc], &key) <= tol).count();
    if (same as f32) < PLAIN_SAME * pixels.len() as f32 {
        return None;
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
            keys: Vec::new(),
            owner: Vec::new(),
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
        let main = match spec.color {
            Some(c) => native(ctx, [c[0], c[1], c[2], 1.0]).get(..cc).map(<[f32]>::to_vec),
            None => edge_colour(src, cc, tolerance),
        };
        // The holes clicked by hand: on a visible pixel of the image, its own colour.
        let seeds: Vec<usize> = spec
            .seeds
            .iter()
            .filter(|[x, y]| src.rect.contains(*x, *y))
            .map(|[x, y]| (*y - src.rect.y0) as usize * w + (*x - src.rect.x0) as usize)
            .filter(|&i| src.data[i * n + n - 1] >= 0.5)
            .take(MAX_HOLES)
            .collect();
        if main.is_none() && seeds.is_empty() {
            return Some(plan);
        }
        let mut keys: Vec<Vec<f32>> = Vec::new();
        let mut owner: Vec<u8> = if seeds.is_empty() { Vec::new() } else { vec![0; w * h] };
        let mut class = vec![KEEP; w * h];
        let mut stack: Vec<usize> = Vec::new();
        // A 4-connected flood fill of `region` from the pixels on `stack`, owned by key `k`.
        let flood = |class: &mut Vec<u8>, owner: &mut Vec<u8>, stack: &mut Vec<usize>, k: u8, region: &dyn Fn(usize) -> bool| -> bool {
            let mut steps = 0usize;
            while let Some(i) = stack.pop() {
                steps += 1;
                if steps % (1 << 20) == 0 && ctl.cancelled() {
                    return false;
                }
                let (x, y) = (i % w, i / w);
                let mut visit = |j: usize| {
                    if class[j] == KEEP && region(j) {
                        class[j] = BACKGROUND;
                        if let Some(o) = owner.get_mut(j) {
                            *o = k;
                        }
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
            true
        };
        // 1. The background reached from the image edges.
        let background = main.clone();
        let is_background = |i: usize| -> bool {
            let px = &src.data[i * n..i * n + n];
            background.as_ref().is_some_and(|key| px[n - 1] <= 0.0 || distance(&px[..cc], key) <= tolerance)
        };
        if let Some(key) = main {
            keys.push(key);
            let start = |i: usize, class: &mut Vec<u8>, stack: &mut Vec<usize>| {
                if class[i] == KEEP && is_background(i) {
                    class[i] = BACKGROUND;
                    stack.push(i);
                }
            };
            for x in 0..w {
                start(x, &mut class, &mut stack);
                start((h - 1) * w + x, &mut class, &mut stack);
            }
            for y in 0..h {
                start(y * w, &mut class, &mut stack);
                start(y * w + w - 1, &mut class, &mut stack);
            }
            if !flood(&mut class, &mut owner, &mut stack, 0, &is_background) {
                return None;
            }
        }
        // 1b. Each hole clicked by hand: its colour reached from the click, up to the
        // transparency already there.
        for &i in &seeds {
            if class[i] != KEEP {
                continue;
            }
            let key = src.data[i * n..i * n + cc].to_vec();
            let k = keys.len() as u8;
            let in_hole = |j: usize| {
                let px = &src.data[j * n..j * n + n];
                px[n - 1] > 0.0 && distance(&px[..cc], &key) <= tolerance
            };
            class[i] = BACKGROUND;
            owner[i] = k;
            stack.push(i);
            if !flood(&mut class, &mut owner, &mut stack, k, &in_hole) {
                return None;
            }
            keys.push(key);
        }
        // 2. Enclosed areas of the background colour, when asked, from a minimum size.
        if spec.holes && background.is_some() {
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
                                if !owner.is_empty() {
                                    owner[j] = owner[i];
                                }
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
        plan.keys = keys;
        plan.owner = owner;
        plan.class = class;
        plan.inner = inner;
        Some(plan)
    }

    /// The cutout of `out` (inside the prepared area; pixels outside it are kept).
    pub fn apply(&self, src: &Image, out: Rect, ctx: &Ctx) -> Vec<f32> {
        let n = src.ch;
        let mut res = src.crop(out);
        if self.class.is_empty() || self.keys.is_empty() || n < 2 {
            return res;
        }
        let cc = ncol(ctx, n).min(MAXC).min(self.keys[0].len());
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
                    let owner = self.owner.get(k).map_or(0, |o| usize::from(*o));
                    let key = &self.keys.get(owner).unwrap_or(&self.keys[0])[..cc];
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

    /// 40 × 40: transparent outside a square (10..30), a 1 px semi-transparent edge (alpha 0.7)
    /// and a 1 px semi-transparent band (alpha 0.3) on the inside of the edge; a 2 × 2 hole
    /// (19..21) with a 0.4 ring; a semi-transparent shadow (0.5) deep inside at (14, 24).
    fn square() -> Image {
        let r = Rect::new(0, 0, 40, 40);
        let mut data = Vec::new();
        for y in 0..40i32 {
            for x in 0..40i32 {
                let d = (x - 10).min(29 - x).min(y - 10).min(29 - y);
                let hole = (x as f32 - 19.5).abs().max((y as f32 - 19.5).abs());
                let a = if d < 0 {
                    0.0
                } else if d == 0 {
                    0.7
                } else if d == 1 {
                    0.3
                } else if (19..21).contains(&x) && (19..21).contains(&y) {
                    0.0
                } else if hole < 1.6 {
                    0.4
                } else if (x, y) == (14, 24) {
                    0.5
                } else {
                    1.0
                };
                data.extend_from_slice(&[0.2, 0.4, 0.6, a]);
            }
        }
        Image { rect: r, ch: 4, data }
    }

    #[test]
    fn threshold_works_on_the_fringe_only_and_keeps_colours() {
        let src = square();
        let out = alpha_threshold(&src, src.rect, &ctx(), 0.62, 2.0);
        let a = |x: usize, y: usize| out[(y * 40 + x) * 4 + 3];
        // The edge (1 px from the transparency) at 0.7 → opaque, the band (2 px) at 0.3 → clear.
        assert_eq!((a(10, 20), a(11, 20)), (1.0, 0.0));
        // The hole's ring is fringe too: 0.4 → clear.
        assert_eq!(a(18, 20), 0.0);
        // Deep inside, the shadow stays semi-transparent.
        assert_eq!(a(14, 24), 0.5);
        assert!(out.chunks_exact(4).all(|p| p[0] == 0.2 && p[1] == 0.4 && p[2] == 0.6));
        // 1 px wide: the inner band is no longer fringe.
        let out = alpha_threshold(&src, src.rect, &ctx(), 0.62, 1.0);
        assert_eq!((out[(20 * 40 + 10) * 4 + 3], out[(20 * 40 + 11) * 4 + 3]), (1.0, 0.3));
    }

    #[test]
    fn threshold_leaves_a_layer_without_transparency_alone() {
        // RGB without alpha: the antialiasing greys stay (they used to be pushed to 0 or 1).
        let r = Rect::new(0, 0, 4, 1);
        let src = Image { rect: r, ch: 3, data: vec![0.2, 0.5, 0.8, 0.1, 0.9, 0.4, 0.6, 0.6, 0.6, 0.3, 0.3, 0.3] };
        let no_alpha = Ctx { alpha: false, ..ctx() };
        assert_eq!(alpha_threshold(&src, r, &no_alpha, 0.5, 2.0), src.data);
        // The 8-bit 50 % split on the fringe: 127/255 < 0.5 → 0, 128/255 ≥ 0.5 → 1.
        let r = Rect::new(0, 0, 3, 1);
        let src = Image { rect: r, ch: 2, data: vec![0.0, 0.0, 0.5, 127.0 / 255.0, 0.5, 128.0 / 255.0] };
        let one = Ctx { bounds: r, ..ctx() };
        let out = alpha_threshold(&src, r, &one, 0.5, 2.0);
        assert_eq!((out[3], out[5]), (0.0, 1.0));
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
    fn clicked_holes_are_removed_with_the_background_and_their_contour_unmixed() {
        let mut img = logo([1.0; 3]);
        // An antialiased inner edge: a 50 % grey (black mixed with white) at (14, 20).
        let at = (20 * 40 + 14) * 4;
        img.data[at..at + 3].copy_from_slice(&[0.5; 3]);
        let out = cut(&img, CutoutSpec { seeds: vec![[20, 20]], ..CutoutSpec::default() });
        assert_eq!((alpha_at(&out, 20, 20), alpha_at(&out, 0, 0)), (0.0, 0.0), "the hole and the background");
        assert_eq!(alpha_at(&out, 12, 12), 1.0, "the ring stays");
        // The grey contour pixel is unmixed: black at half opacity, no light line left.
        assert!((alpha_at(&out, 14, 20) - 0.5).abs() < 0.02, "{}", alpha_at(&out, 14, 20));
        assert!(out[at..at + 3].iter().all(|v| *v < 0.02), "{:?}", &out[at..at + 3]);
        // A click on the background, or outside the image, adds nothing.
        let plain = cut(&img, CutoutSpec::default());
        assert_eq!(cut(&img, CutoutSpec { seeds: vec![[0, 0], [-5, 99]], ..CutoutSpec::default() }), plain);
    }

    /// An already transparent logo: its bounds run through its own outline, not a background.
    fn rounded_logo() -> Image {
        let r = Rect::new(0, 0, 40, 40);
        let mut data = Vec::new();
        for y in 0..40 {
            for x in 0..40 {
                let d = ((x as f32 - 19.5).powi(2) + (y as f32 - 19.5).powi(2)).sqrt();
                // A brown disc with a dark outline (14–18 px) and a white centre (< 6 px).
                let c = if d < 6.0 {
                    [1.0, 1.0, 1.0, 1.0]
                } else if d < 14.0 {
                    [0.7, 0.5, 0.3, 1.0]
                } else if d < 18.0 {
                    [0.3, 0.2, 0.1, 1.0]
                } else {
                    [0.0, 0.0, 0.0, 0.0]
                };
                data.extend_from_slice(&c);
            }
        }
        Image { rect: r, ch: 4, data }
    }

    #[test]
    fn an_already_transparent_file_has_no_background_to_detect() {
        let img = rounded_logo();
        let ctx = Ctx { bounds: img.rect, mode: ColorMode::Rgb, alpha: true };
        assert_eq!(plain_background(&img, &ctx, 0.1), None);
        // Nothing is removed: the dark outline stays.
        assert_eq!(cut(&img, CutoutSpec::default()), img.data);
        // A hole clicked in the white centre is still cut.
        let out = cut(&img, CutoutSpec { seeds: vec![[20, 20]], ..CutoutSpec::default() });
        assert_eq!((alpha_at(&out, 20, 20), alpha_at(&out, 20, 3)), (0.0, 1.0));
        // A white file is detected, a photo-like edge is not.
        assert_eq!(plain_background(&logo([1.0; 3]), &ctx, 0.1).map(|k| k.iter().all(|v| *v > 0.95)), Some(true));
        let mut mixed = logo([1.0; 3]);
        for x in 0..40 {
            let i = x * 4;
            mixed.data[i..i + 3].copy_from_slice(&[(x as f32) / 40.0, 0.2, 0.8]);
            let i = (39 * 40 + x) * 4;
            mixed.data[i..i + 3].copy_from_slice(&[0.1, (x as f32) / 40.0, 0.3]);
        }
        for y in 0..40 {
            let i = y * 40 * 4;
            mixed.data[i..i + 3].copy_from_slice(&[0.9, 0.1, (y as f32) / 40.0]);
        }
        assert_eq!(plain_background(&mixed, &ctx, 0.1), None);
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
        let _ = alpha_threshold(&src, src.rect, &ctx(), f32::NAN, f32::NAN);
        let empty = Image { rect: Rect::EMPTY, ch: 4, data: vec![] };
        assert!(halftone(&empty, Rect::EMPTY, &ctx(), &spec).is_empty());
    }
}
