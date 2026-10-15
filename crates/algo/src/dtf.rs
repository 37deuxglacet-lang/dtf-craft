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
use crate::fxutil::{rgba, xy};
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

    #[test]
    fn hostile_values_do_not_panic() {
        let spec = HalftoneSpec { cell: f32::NAN, angle: f32::INFINITY, gamma: -3.0, low: 2.0, high: -1.0, min_dot: f32::NAN, ..HalftoneSpec::default() };
        let src = ramp([0.5, 0.5, 0.5]);
        let _ = halftone(&src, src.rect, &ctx(), &spec);
        let _ = alpha_threshold(&src, src.rect, &ctx(), f32::NAN);
        let empty = Image { rect: Rect::EMPTY, ch: 4, data: vec![] };
        assert!(halftone(&empty, Rect::EMPTY, &ctx(), &spec).is_empty());
    }
}
