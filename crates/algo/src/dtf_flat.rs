// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! Flat Colour (DTF Craft): cleans the dark fringe an AI upscale leaves along the edges of flat
//! colour shapes (1 to 3 px darker than the shape, from the upscaler over-sharpening contrasted
//! transitions, worse from JPEG sources), without touching intended outlines.
//!
//! 1. Palette: the opaque pixels' colours grouped in N colours (k-means in L*a*b*; N chosen
//!    automatically, or given).
//! 2. Shapes: the 4-connected areas of one palette colour.
//! 3. Each shape's dominant colour: the median of its inner pixels (more than 4 px from its
//!    contour), else its palette colour.
//! 4. Edge band: the pixels within `band` px of a contour (another shape or transparency).
//! 5. Fringe: a band pixel darker than its shape's dominant by more than `darken` (ΔL*) and close
//!    to it in hue (ΔH* within `tolerance`, with some of its chroma) takes the dominant colour; one darker but
//!    only roughly as close (up to twice the tolerance) takes the colour most of its 8 neighbours'
//!    shapes have, so a contour is not thickened.
//! 6. Edges: sharp (no blend, best for the DTF white underbase), or a 1 px blend of the two
//!    dominant colours recomputed on the contour between two shapes.
//!
//! Colours picked as protected (an outline's black) are never replaced. Alpha never changes.

use std::collections::HashMap;

use photocraft_geom::Rect;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::Ctx;
use crate::fxutil::{rgba, set_rgba, xy};
use crate::image::Image;

/// Flat Colour settings (the filter's [`crate::FilterParams::DtfFlatColor`] fields).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct FlatColorSpec {
    /// Palette size; `None`: chosen from the image.
    pub colors: Option<u32>,
    /// Width of the edge band examined, in px (1–5).
    pub band: f32,
    /// How much darker than the shape (ΔL*) a band pixel must be to count as fringe (0–40).
    pub darken: f32,
    /// How close in hue (ΔH*) a fringe pixel is to its shape (0–50).
    pub tolerance: f32,
    /// A 1 px blend between two shapes instead of sharp edges.
    pub antialias: bool,
    /// Straight sRGB colours (0–1) never replaced.
    pub protected: Vec<[f32; 3]>,
    /// The pixels changed painted magenta (a preview view, not for applying).
    pub highlight: bool,
}

impl Default for FlatColorSpec {
    fn default() -> Self {
        FlatColorSpec { colors: None, band: 2.0, darken: 8.0, tolerance: 15.0, antialias: false, protected: Vec::new(), highlight: false }
    }
}

/// Most palette colours (auto or given).
pub const MAX_COLORS: u32 = 32;
/// Inner pixels: more than this from the contour (px).
const INNER: u8 = 4;
/// Distances are counted up to this (px).
const FAR: u8 = 8;
const TRANSPARENT: u8 = u8::MAX;
const MAGENTA: [f32; 3] = [1.0, 0.0, 1.0];
/// Pixels sampled for the palette.
const SAMPLES: usize = 60_000;

/// The colours Flat Colour gives the pixels it changes, prepared once over the whole area.
#[derive(Clone, Debug, Default)]
pub struct FlatColorPlan {
    rect: Rect,
    changed: HashMap<u32, [f32; 3]>,
    highlight: bool,
}

fn lab_of(ctx: &Ctx, px: &[f32]) -> ([f32; 3], f32) {
    let c = rgba(ctx, px);
    (photocraft_color::convert::srgb_to_lab([c[0], c[1], c[2]]), c[3])
}

fn d2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn nearest(centers: &[[f32; 3]], p: [f32; 3]) -> (usize, f32) {
    let mut best = (0, f32::MAX);
    for (i, c) in centers.iter().enumerate() {
        let d = d2(*c, p);
        if d < best.1 {
            best = (i, d);
        }
    }
    best
}

/// k-means of `samples` in `k` colours (k-means++ start from a fixed seed: the same image gives
/// the same palette). Returns the centres and the sum of squared distances.
fn kmeans(samples: &[[f32; 3]], k: usize) -> (Vec<[f32; 3]>, f64) {
    let mut seed: u64 = 0x9E37_79B9_7F4A_7C15;
    let mut rand = move || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    let mut centers = vec![samples[0]];
    let mut dist: Vec<f32> = samples.iter().map(|s| d2(*s, samples[0])).collect();
    while centers.len() < k {
        let total: f64 = dist.iter().map(|d| f64::from(*d)).sum();
        if total <= 0.0 {
            break;
        }
        let mut target = rand() * total;
        let mut pick = samples.len() - 1;
        for (i, d) in dist.iter().enumerate() {
            target -= f64::from(*d);
            if target <= 0.0 {
                pick = i;
                break;
            }
        }
        let c = samples[pick];
        centers.push(c);
        for (d, s) in dist.iter_mut().zip(samples) {
            *d = d.min(d2(*s, c));
        }
    }
    let mut sse = 0.0;
    for _ in 0..15 {
        let mut sum = vec![[0.0f64; 3]; centers.len()];
        let mut count = vec![0usize; centers.len()];
        sse = 0.0;
        for s in samples {
            let (i, d) = nearest(&centers, *s);
            sse += f64::from(d);
            count[i] += 1;
            for c in 0..3 {
                sum[i][c] += f64::from(s[c]);
            }
        }
        let mut moved = false;
        for (i, c) in centers.iter_mut().enumerate() {
            if count[i] > 0 {
                let n = count[i] as f64;
                let next = [(sum[i][0] / n) as f32, (sum[i][1] / n) as f32, (sum[i][2] / n) as f32];
                moved |= d2(next, *c) > 1e-4;
                *c = next;
            }
        }
        if !moved {
            break;
        }
    }
    (centers, sse)
}

/// The palette: `colors` given, or the fewest colours past which one more barely helps; small
/// automatic clusters (a fringe of their own) join the nearest other colour.
fn palette(samples: &[[f32; 3]], colors: Option<u32>) -> Vec<[f32; 3]> {
    if let Some(n) = colors {
        let (centers, _) = kmeans(samples, (n.clamp(2, MAX_COLORS) as usize).min(samples.len()));
        return centers;
    }
    let mut prev = kmeans(samples, 1);
    let mut chosen = prev.0.clone();
    for k in 2..=16usize.min(samples.len()) {
        let next = kmeans(samples, k);
        let gain = if prev.1 > 0.0 { 1.0 - next.1 / prev.1 } else { 0.0 };
        // Mean distance under 3 ΔE: the colours are found.
        let mean = (next.1 / samples.len() as f64).sqrt();
        chosen = next.0.clone();
        if gain < 0.15 || mean < 3.0 {
            if gain < 0.15 {
                chosen = prev.0.clone();
            }
            break;
        }
        prev = next;
    }
    // Clusters under 0.3 % of the pixels: merged into the nearest other colour.
    let mut count = vec![0usize; chosen.len()];
    for s in samples {
        count[nearest(&chosen, *s).0] += 1;
    }
    let small = (samples.len() as f64 * 0.003).ceil() as usize;
    let kept: Vec<[f32; 3]> = chosen.iter().zip(&count).filter(|(_, n)| **n > small).map(|(c, _)| *c).collect();
    if kept.len() >= 2 { kept } else { chosen }
}

/// How far `lab` is in hue from a shape's colour `dom` (ΔH*). A fringe keeps some of the shape's
/// chroma: a pixel with under 35 % of it (black against red) is not of its hue. Against a grey or
/// white shape (chroma under 10), a pixel is of its hue as long as its own chroma is within
/// `tol`.
fn hue_distance(dom: [f32; 3], lab: [f32; 3], tol: f32) -> f32 {
    let chroma = |c: [f32; 3]| c[1].hypot(c[2]);
    let (cd, cp) = (chroma(dom), chroma(lab));
    if cd < 10.0 {
        return if cp <= tol { cp } else { f32::MAX };
    }
    if cp < 0.35 * cd {
        return f32::MAX;
    }
    let dab2 = (dom[1] - lab[1]).powi(2) + (dom[2] - lab[2]).powi(2);
    (dab2 - (cd - cp).powi(2)).max(0.0).sqrt()
}

impl FlatColorPlan {
    /// Prepares Flat Colour over `src` (the whole reference area). `None` when cancelled.
    pub fn prepare(src: &Image, ctx: &Ctx, spec: &FlatColorSpec, ctl: &photocraft_raster::Interrupt) -> Option<FlatColorPlan> {
        let n = src.ch;
        let (w, h) = (src.rect.width() as usize, src.rect.height() as usize);
        let mut plan = FlatColorPlan { rect: src.rect, changed: HashMap::new(), highlight: spec.highlight };
        if n == 0 || w == 0 || h == 0 || src.data.len() < w * h * n {
            return Some(plan);
        }
        let opaque = |i: usize| !ctx.alpha || src.data[i * n + n - 1] >= 0.5;
        // 1. Palette.
        let total = w * h;
        let step = (total / SAMPLES).max(1);
        let samples: Vec<[f32; 3]> = (0..total).step_by(step).filter(|&i| opaque(i)).map(|i| lab_of(ctx, &src.data[i * n..(i + 1) * n]).0).collect();
        if samples.len() < 2 {
            return Some(plan);
        }
        let centers = palette(&samples, spec.colors);
        ctl.progress(0.15);
        if ctl.cancelled() {
            return None;
        }
        let mut label = vec![TRANSPARENT; total];
        label.par_chunks_mut(w).enumerate().for_each(|(y, row)| {
            for (x, l) in row.iter_mut().enumerate() {
                let i = y * w + x;
                if opaque(i) {
                    *l = nearest(&centers, lab_of(ctx, &src.data[i * n..(i + 1) * n]).0).0 as u8;
                }
            }
        });
        ctl.progress(0.35);
        if ctl.cancelled() {
            return None;
        }
        // 2. Shapes: 4-connected areas of one palette colour.
        let mut comp = vec![u32::MAX; total];
        let mut shapes = 0u32;
        let mut stack = Vec::new();
        for start in 0..total {
            if label[start] == TRANSPARENT || comp[start] != u32::MAX {
                continue;
            }
            let l = label[start];
            comp[start] = shapes;
            stack.push(start);
            while let Some(i) = stack.pop() {
                let (x, y) = (i % w, i / w);
                let mut visit = |j: usize| {
                    if label[j] == l && comp[j] == u32::MAX {
                        comp[j] = shapes;
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
            shapes += 1;
        }
        ctl.progress(0.5);
        if ctl.cancelled() {
            return None;
        }
        // 4. Distance to the contour (1 on it), up to FAR px (8-connected).
        let mut dist = vec![0u8; total];
        let mut queue = std::collections::VecDeque::new();
        for i in 0..total {
            if label[i] == TRANSPARENT {
                continue;
            }
            let (x, y) = (i % w, i / w);
            let other = |j: usize| comp[j] != comp[i];
            let edge = (x > 0 && other(i - 1)) || (x + 1 < w && other(i + 1)) || (y > 0 && other(i - w)) || (y + 1 < h && other(i + w));
            if edge {
                dist[i] = 1;
                queue.push_back(i);
            }
        }
        while let Some(i) = queue.pop_front() {
            let d = dist[i];
            if d >= FAR {
                continue;
            }
            let (x, y) = ((i % w) as i64, (i / w) as i64);
            for (dx, dy) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                let (nx, ny) = (x + dx, y + dy);
                if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                    continue;
                }
                let j = ny as usize * w + nx as usize;
                if label[j] != TRANSPARENT && dist[j] == 0 && comp[j] == comp[i] {
                    dist[j] = d + 1;
                    queue.push_back(j);
                }
            }
        }
        ctl.progress(0.65);
        if ctl.cancelled() {
            return None;
        }
        // 3. Dominant colour of each shape: the median of its inner pixels (8-bit histograms).
        let mut hist: HashMap<u32, Box<[[u32; 256]; 3]>> = HashMap::new();
        for i in 0..total {
            if label[i] == TRANSPARENT || (dist[i] != 0 && dist[i] <= INNER) {
                continue;
            }
            let c = rgba(ctx, &src.data[i * n..(i + 1) * n]);
            let hst = hist.entry(comp[i]).or_insert_with(|| Box::new([[0u32; 256]; 3]));
            for k in 0..3 {
                hst[k][(c[k].clamp(0.0, 1.0) * 255.0).round() as usize] += 1;
            }
        }
        let median = |hst: &[u32; 256]| {
            let total: u32 = hst.iter().sum();
            let mut acc = 0;
            for (v, n) in hst.iter().enumerate() {
                acc += n;
                if acc * 2 >= total {
                    return v as f32 / 255.0;
                }
            }
            1.0
        };
        let dominant: HashMap<u32, ([f32; 3], [f32; 3])> = hist
            .iter()
            .map(|(c, hst)| {
                let rgb = [median(&hst[0]), median(&hst[1]), median(&hst[2])];
                (*c, (rgb, photocraft_color::convert::srgb_to_lab(rgb)))
            })
            .collect();
        drop(hist);
        // Shapes without inner pixels: their palette colour.
        let shape_colour = |i: usize| -> ([f32; 3], [f32; 3]) {
            match dominant.get(&comp[i]) {
                Some(d) => *d,
                None => {
                    let lab = centers[label[i] as usize];
                    (photocraft_color::convert::lab_to_srgb(lab), lab)
                }
            }
        };
        ctl.progress(0.75);
        if ctl.cancelled() {
            return None;
        }
        // 5. The fringe in the band.
        let band = spec.band.clamp(1.0, 5.0).round() as u8;
        let darken = spec.darken.clamp(0.0, 40.0);
        let tol = spec.tolerance.clamp(0.0, 50.0);
        let protected: Vec<[f32; 3]> = spec.protected.iter().map(|c| photocraft_color::convert::srgb_to_lab(*c)).collect();
        let changes: Vec<(u32, [f32; 3])> = (0..total)
            .into_par_iter()
            .filter_map(|i| {
                let d = dist[i];
                if label[i] == TRANSPARENT || d == 0 || d > band {
                    return None;
                }
                let (lab, _) = lab_of(ctx, &src.data[i * n..(i + 1) * n]);
                if protected.iter().any(|p| d2(*p, lab).sqrt() <= tol.max(1.0)) {
                    return None;
                }
                let fringe = |dom: [f32; 3]| (dom[0] - lab[0] > darken, hue_distance(dom, lab, tol));
                // A shape too thin to have inner pixels (the fringe gathered in a colour of its
                // own): compared with the nearest wider shape around it.
                if !dominant.contains_key(&comp[i]) {
                    let r = i64::from(band) + 2;
                    let (x, y) = ((i % w) as i64, (i / w) as i64);
                    let mut best: Option<(i64, [f32; 3])> = None;
                    for ny in (y - r).max(0)..=(y + r).min(h as i64 - 1) {
                        for nx in (x - r).max(0)..=(x + r).min(w as i64 - 1) {
                            let j = ny as usize * w + nx as usize;
                            if label[j] == TRANSPARENT {
                                continue;
                            }
                            let Some((rgb, dom)) = dominant.get(&comp[j]) else { continue };
                            let (darker, dab) = fringe(*dom);
                            let k = (nx - x).abs().max((ny - y).abs());
                            if darker && dab <= tol && best.is_none_or(|b| k < b.0) {
                                best = Some((k, *rgb));
                            }
                        }
                    }
                    if let Some((_, rgb)) = best {
                        return Some((i as u32, rgb));
                    }
                }
                let (rgb, dom) = shape_colour(i);
                let (darker, dab) = fringe(dom);
                if !darker {
                    return None;
                }
                if dab <= tol {
                    return Some((i as u32, rgb));
                }
                if dab > 2.0 * tol {
                    return None;
                }
                // Ambiguous: the colour most of its neighbours' shapes have.
                let (x, y) = ((i % w) as i64, (i / w) as i64);
                let mut votes: Vec<(u8, usize, u32)> = Vec::new();
                for (dx, dy) in [(-1i64, -1i64), (0, -1), (1, -1), (-1, 0), (1, 0), (-1, 1), (0, 1), (1, 1)] {
                    let (nx, ny) = (x + dx, y + dy);
                    if nx < 0 || ny < 0 || nx >= w as i64 || ny >= h as i64 {
                        continue;
                    }
                    let j = ny as usize * w + nx as usize;
                    if label[j] == TRANSPARENT {
                        continue;
                    }
                    match votes.iter_mut().find(|v| v.0 == label[j]) {
                        Some(v) => v.2 += 1,
                        None => votes.push((label[j], j, 1)),
                    }
                }
                let best = votes.iter().max_by_key(|v| v.2)?;
                Some((i as u32, shape_colour(best.1).0))
            })
            .collect();
        let fixed: HashMap<u32, [f32; 3]> = changes.into_iter().collect();
        ctl.progress(0.85);
        if ctl.cancelled() {
            return None;
        }
        // 6. A 1 px blend of the two shapes' colours on the contour between them.
        if spec.antialias {
            let blends: Vec<(u32, [f32; 3])> = (0..total)
                .into_par_iter()
                .filter_map(|i| {
                    if label[i] == TRANSPARENT || dist[i] != 1 {
                        return None;
                    }
                    let (x, y) = (i % w, i / w);
                    let mut nbrs = Vec::with_capacity(4);
                    if x > 0 {
                        nbrs.push(i - 1);
                    }
                    if x + 1 < w {
                        nbrs.push(i + 1);
                    }
                    if y > 0 {
                        nbrs.push(i - w);
                    }
                    if y + 1 < h {
                        nbrs.push(i + w);
                    }
                    // One side of the contour only: the blend stays 1 px wide.
                    let j = nbrs.into_iter().find(|&j| label[j] != TRANSPARENT && comp[j] < comp[i])?;
                    let (lab, _) = lab_of(ctx, &src.data[i * n..(i + 1) * n]);
                    if protected.iter().any(|p| d2(*p, lab).sqrt() <= tol.max(1.0)) {
                        return None;
                    }
                    let colour = |k: usize| fixed.get(&(k as u32)).copied().unwrap_or_else(|| shape_colour(k).0);
                    let (a, b) = (colour(i), colour(j));
                    Some((i as u32, [0.5 * (a[0] + b[0]), 0.5 * (a[1] + b[1]), 0.5 * (a[2] + b[2])]))
                })
                .collect();
            plan.changed = fixed;
            plan.changed.extend(blends);
        } else {
            plan.changed = fixed;
        }
        ctl.progress(0.9);
        Some(plan)
    }

    /// The pixels the plan changes, in the area it was prepared on.
    pub fn changed(&self) -> usize {
        self.changed.len()
    }

    /// The tile `out` of the result (the changed pixels in their new colour, or magenta).
    pub fn apply(&self, src: &Image, out: Rect, ctx: &Ctx) -> Vec<f32> {
        let n = src.ch;
        let mut res = src.crop(out);
        if self.changed.is_empty() || n == 0 {
            return res;
        }
        let w = self.rect.width() as usize;
        for (i, px) in res.chunks_exact_mut(n).enumerate() {
            let (x, y) = xy(out, i);
            if !self.rect.contains(x, y) {
                continue;
            }
            let k = ((y - self.rect.y0) as usize * w + (x - self.rect.x0) as usize) as u32;
            if let Some(c) = self.changed.get(&k) {
                let a = rgba(ctx, px)[3];
                let c = if self.highlight { MAGENTA } else { *c };
                set_rgba(ctx, px, [c[0], c[1], c[2], a]);
            }
        }
        res
    }
}

/// Without a prepared plan (a single tile, as the generic kernel runs it): prepares over the tile.
pub fn flat_color(src: &Image, out: Rect, ctx: &Ctx, spec: &FlatColorSpec) -> Vec<f32> {
    let never = || false;
    let ignore = |_: f32| {};
    let ctl = photocraft_raster::Interrupt::new(&never, &ignore);
    match FlatColorPlan::prepare(src, ctx, spec, &ctl) {
        Some(plan) => plan.apply(src, out, ctx),
        None => src.crop(out),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use photocraft_color::ColorMode;

    fn image(w: i32, h: i32, f: impl Fn(i32, i32) -> [f32; 4]) -> Image {
        let mut data = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                data.extend_from_slice(&f(x, y));
            }
        }
        Image { rect: Rect::new(0, 0, w, h), ch: 4, data }
    }

    fn ctx(w: i32, h: i32) -> Ctx {
        Ctx { bounds: Rect::new(0, 0, w, h), mode: ColorMode::Rgb, alpha: true }
    }

    fn run(img: &Image, spec: &FlatColorSpec) -> Vec<f32> {
        flat_color(img, img.rect, &ctx(img.rect.width() as i32, img.rect.height() as i32), spec)
    }

    const RED: [f32; 4] = [0.85, 0.1, 0.1, 1.0];
    const DARK_RED: [f32; 4] = [0.55, 0.05, 0.05, 1.0];
    const BLUE: [f32; 4] = [0.1, 0.2, 0.85, 1.0];
    const BLACK: [f32; 4] = [0.0, 0.0, 0.0, 1.0];

    /// A red square (x 10..30) on a blue one, with a 2 px dark red fringe along its edges.
    fn fringed() -> Image {
        image(40, 40, |x, y| {
            let inside = (10..30).contains(&x) && (10..30).contains(&y);
            let edge = inside && (x < 12 || x >= 28 || y < 12 || y >= 28);
            if edge {
                DARK_RED
            } else if inside {
                RED
            } else {
                BLUE
            }
        })
    }

    fn px(data: &[f32], w: i32, x: i32, y: i32) -> [f32; 4] {
        let i = ((y * w + x) * 4) as usize;
        [data[i], data[i + 1], data[i + 2], data[i + 3]]
    }

    #[test]
    fn the_dark_fringe_takes_the_shape_colour() {
        let out = run(&fringed(), &FlatColorSpec { colors: Some(2), ..Default::default() });
        for (x, y) in [(10, 10), (11, 20), (29, 29), (20, 28)] {
            let p = px(&out, 40, x, y);
            assert!((p[0] - RED[0]).abs() < 0.02 && (p[1] - RED[1]).abs() < 0.02, "({x}, {y}) {p:?}");
        }
        // The blue and the red's inside don't change.
        assert_eq!(px(&out, 40, 2, 2), BLUE);
        assert_eq!(px(&out, 40, 20, 20), RED);
    }

    #[test]
    fn an_outline_of_another_hue_or_protected_stays() {
        // A red square with a black 2 px outline, on transparency.
        let img = image(40, 40, |x, y| {
            let inside = (10..30).contains(&x) && (10..30).contains(&y);
            let edge = inside && (x < 12 || x >= 28 || y < 12 || y >= 28);
            if edge {
                BLACK
            } else if inside {
                RED
            } else {
                [0.0; 4]
            }
        });
        let out = run(&img, &FlatColorSpec::default());
        assert_eq!(px(&out, 40, 10, 10), BLACK, "black is not a darker red");
        assert_eq!(out, img.data);
        // A dark red outline protected at the eyedropper stays too.
        let out = run(&fringed(), &FlatColorSpec { colors: Some(2), protected: vec![[DARK_RED[0], DARK_RED[1], DARK_RED[2]]], ..Default::default() });
        assert_eq!(px(&out, 40, 10, 10), DARK_RED);
    }

    #[test]
    fn highlight_shows_the_changes_and_alpha_stays() {
        let out = run(&fringed(), &FlatColorSpec { colors: Some(2), highlight: true, ..Default::default() });
        assert_eq!(px(&out, 40, 10, 10), [1.0, 0.0, 1.0, 1.0]);
        assert_eq!(px(&out, 40, 20, 20), RED);
        // A band of 1 px leaves the second row of the fringe.
        let out = run(&fringed(), &FlatColorSpec { colors: Some(2), band: 1.0, highlight: true, ..Default::default() });
        assert_eq!(px(&out, 40, 10, 20), [1.0, 0.0, 1.0, 1.0]);
        assert_eq!(px(&out, 40, 11, 20), DARK_RED);
    }

    #[test]
    fn auto_palette_finds_the_colours_and_a_blend_softens_the_edge() {
        let mut spec = FlatColorSpec::default();
        let out = run(&fringed(), &spec);
        assert!((px(&out, 40, 10, 20)[0] - RED[0]).abs() < 0.02, "auto palette");
        spec.antialias = true;
        let out = run(&fringed(), &spec);
        // On the contour between red and blue: half of each.
        let p = px(&out, 40, 10, 20);
        assert!((p[0] - 0.475).abs() < 0.03 && (p[2] - 0.475).abs() < 0.03, "{p:?}");
    }
}
