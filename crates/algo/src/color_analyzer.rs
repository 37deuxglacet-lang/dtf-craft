// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Color Analyzer (DTF Craft): the main colours of a visual and, around each one, a grid
//! of variants to print and compare with the expected colour.
//!
//! * [`extract`]: the main colours of 8-bit RGBA pixels. Transparent pixels (alpha below half)
//!   are left out; close shades and anti-aliasing merge (a 15-bit histogram, then the most
//!   frequent shades further apart than `merge`), and each colour is the mean of the pixels it
//!   gathers (one k-means pass), so a flat colour reads exactly.
//! * [`grid`]: an `n × n` block of variants around a colour, along two axes ([`Axis`]): the
//!   colour itself in the middle, each step one `step` apart. The horizontal axis grows to the
//!   right, the vertical one upwards.
//! * [`Board`]: the printed sheet's geometry, in millimetres: a header, then one block per colour.

use serde::{Deserialize, Serialize};

/// Fewest pixels (share of the visible ones) a shade needs to become a colour of its own,
/// unless no colour was found yet: below it are the leftovers of edges.
const MIN_SHARE: f64 = 0.003;
/// Pixels read at most: larger visuals are sampled on a regular grid.
const MAX_SAMPLES: usize = 400_000;

/// One colour of a visual: its sRGB value and its share of the visible pixels (0..1).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Swatch {
    pub rgb: [u8; 3],
    pub share: f64,
}

fn dist(a: [f64; 3], b: [f64; 3]) -> f64 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

fn to_u8(c: [f64; 3]) -> [u8; 3] {
    c.map(|v| v.round().clamp(0.0, 255.0) as u8)
}

/// The main colours of `rgba` (`width × height` interleaved 8-bit RGBA, straight alpha), at most
/// `max_colors`, the most frequent first. `merge` is the RGB distance (0..441) under which two
/// shades count as one colour: higher gives fewer colours.
pub fn extract(rgba: &[u8], width: usize, height: usize, max_colors: usize, merge: f64) -> Vec<Swatch> {
    let max_colors = max_colors.clamp(1, 64);
    let merge = if merge.is_finite() { merge.clamp(1.0, 442.0) } else { 40.0 };
    let pixels = width.saturating_mul(height).min(rgba.len() / 4);
    if pixels == 0 {
        return Vec::new();
    }
    // A regular grid of samples on large visuals.
    let stride = (pixels as f64 / MAX_SAMPLES as f64).sqrt().ceil().max(1.0) as usize;
    // Histogram on 5 bits per channel: anti-aliased shades fall together.
    let mut count = vec![0u64; 1 << 15];
    let mut sum = vec![[0u64; 3]; 1 << 15];
    let mut total = 0u64;
    for y in (0..height).step_by(stride) {
        for x in (0..width).step_by(stride) {
            let i = (y * width + x) * 4;
            let Some(px) = rgba.get(i..i + 4) else { continue };
            if px[3] < 128 {
                continue;
            }
            let key = (usize::from(px[0] >> 3) << 10) | (usize::from(px[1] >> 3) << 5) | usize::from(px[2] >> 3);
            if let (Some(c), Some(s)) = (count.get_mut(key), sum.get_mut(key)) {
                *c += 1;
                s[0] += u64::from(px[0]);
                s[1] += u64::from(px[1]);
                s[2] += u64::from(px[2]);
                total += 1;
            }
        }
    }
    if total == 0 {
        return Vec::new();
    }
    // Buckets: mean colour and weight, the most frequent first.
    let mut buckets: Vec<([f64; 3], u64)> = count
        .iter()
        .zip(&sum)
        .filter(|(c, _)| **c > 0)
        .map(|(c, s)| {
            let n = *c as f64;
            ([s[0] as f64 / n, s[1] as f64 / n, s[2] as f64 / n], *c)
        })
        .collect();
    buckets.sort_by(|a, b| b.1.cmp(&a.1));
    let floor = (total as f64 * MIN_SHARE).ceil() as u64;
    let mut palette: Vec<[f64; 3]> = Vec::new();
    for (c, n) in &buckets {
        if palette.len() >= max_colors {
            break;
        }
        if *n < floor && !palette.is_empty() {
            continue;
        }
        if palette.iter().all(|p| dist(*p, *c) > merge) {
            palette.push(*c);
        }
    }
    // Two k-means passes over the buckets: every pixel counts for its nearest colour (its
    // share), and the shades within `merge` of it set its value (their mean), so anti-aliased
    // edges further away do not pull a flat colour off.
    let mut weights = vec![0u64; palette.len()];
    for _ in 0..2 {
        let mut acc = vec![[0.0f64; 4]; palette.len()];
        weights = vec![0u64; palette.len()];
        for (c, n) in &buckets {
            let Some((nearest, d)) = palette.iter().enumerate().map(|(k, p)| (k, dist(*p, *c))).min_by(|a, b| a.1.total_cmp(&b.1)) else { continue };
            if let Some(w) = weights.get_mut(nearest) {
                *w += n;
            }
            if d <= merge
                && let Some(a) = acc.get_mut(nearest)
            {
                let f = *n as f64;
                a[0] += c[0] * f;
                a[1] += c[1] * f;
                a[2] += c[2] * f;
                a[3] += f;
            }
        }
        for (p, a) in palette.iter_mut().zip(&acc) {
            if a[3] > 0.0 {
                *p = [a[0] / a[3], a[1] / a[3], a[2] / a[3]];
            }
        }
    }
    let mut out: Vec<Swatch> =
        palette.iter().zip(&weights).filter(|(_, w)| **w > 0).map(|(p, w)| Swatch { rgb: to_u8(*p), share: *w as f64 / total as f64 }).collect();
    out.sort_by(|a, b| b.share.total_cmp(&a.share));
    out
}

/// What one axis of a variant block changes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Axis {
    /// HSL lightness, in percentage points.
    #[default]
    Lightness,
    /// HSL saturation, in percentage points.
    Saturation,
    /// Hue, in degrees.
    Hue,
    /// Red, in % of 255.
    Red,
    /// Green, in % of 255.
    Green,
    /// Blue, in % of 255.
    Blue,
}

impl Axis {
    pub const ALL: [Axis; 6] = [Axis::Lightness, Axis::Saturation, Axis::Hue, Axis::Red, Axis::Green, Axis::Blue];

    /// The id used in command params.
    pub fn id(self) -> &'static str {
        match self {
            Axis::Lightness => "lightness",
            Axis::Saturation => "saturation",
            Axis::Hue => "hue",
            Axis::Red => "red",
            Axis::Green => "green",
            Axis::Blue => "blue",
        }
    }

    pub fn from_id(id: &str) -> Option<Axis> {
        Axis::ALL.into_iter().find(|a| a.id() == id)
    }

    /// French name, as printed on the sheet.
    pub fn name_fr(self) -> &'static str {
        match self {
            Axis::Lightness => "Luminosité",
            Axis::Saturation => "Saturation",
            Axis::Hue => "Teinte",
            Axis::Red => "Rouge",
            Axis::Green => "Vert",
            Axis::Blue => "Bleu",
        }
    }

    /// One-letter French tag (L, S, T, R, V, B), for the labels under the patches.
    pub fn letter_fr(self) -> &'static str {
        match self {
            Axis::Lightness => "L",
            Axis::Saturation => "S",
            Axis::Hue => "T",
            Axis::Red => "R",
            Axis::Green => "V",
            Axis::Blue => "B",
        }
    }

    /// Unit of a step on this axis ("%" or "°").
    pub fn unit(self) -> &'static str {
        if self == Axis::Hue { "°" } else { "%" }
    }
}

/// sRGB (0..255) to HSL: hue in degrees (0..360), saturation and lightness in 0..1.
pub fn rgb_to_hsl(rgb: [u8; 3]) -> [f64; 3] {
    let [r, g, b] = rgb.map(|v| f64::from(v) / 255.0);
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let l = (max + min) / 2.0;
    let d = max - min;
    if d <= f64::EPSILON {
        return [0.0, 0.0, l];
    }
    let s = d / (1.0 - (2.0 * l - 1.0).abs()).max(f64::EPSILON);
    let h = if max == r {
        60.0 * (((g - b) / d).rem_euclid(6.0))
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    [h.rem_euclid(360.0), s.clamp(0.0, 1.0), l.clamp(0.0, 1.0)]
}

/// HSL (see [`rgb_to_hsl`]) to sRGB.
pub fn hsl_to_rgb(hsl: [f64; 3]) -> [u8; 3] {
    let h = hsl[0].rem_euclid(360.0);
    let s = hsl[1].clamp(0.0, 1.0);
    let l = hsl[2].clamp(0.0, 1.0);
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let x = c * (1.0 - ((h / 60.0).rem_euclid(2.0) - 1.0).abs());
    let m = l - c / 2.0;
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    to_u8([(r + m) * 255.0, (g + m) * 255.0, (b + m) * 255.0])
}

/// `rgb` moved by `amount` along `axis` (percentage points of HSL lightness or saturation,
/// degrees of hue, or % of 255 on a channel), clamped to the sRGB gamut.
pub fn shift(rgb: [u8; 3], axis: Axis, amount: f64) -> [u8; 3] {
    if !amount.is_finite() || amount == 0.0 {
        return rgb;
    }
    let channel = |i: usize| {
        let mut out = rgb;
        if let Some(v) = out.get_mut(i) {
            *v = (f64::from(*v) + amount * 2.55).round().clamp(0.0, 255.0) as u8;
        }
        out
    };
    match axis {
        Axis::Red => channel(0),
        Axis::Green => channel(1),
        Axis::Blue => channel(2),
        Axis::Hue | Axis::Saturation | Axis::Lightness => {
            let mut hsl = rgb_to_hsl(rgb);
            match axis {
                Axis::Hue => hsl[0] += amount,
                Axis::Saturation => hsl[1] += amount / 100.0,
                _ => hsl[2] += amount / 100.0,
            }
            hsl_to_rgb(hsl)
        }
    }
}

/// One patch of a variant block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cell {
    /// Column (0 = left) and row (0 = top).
    pub col: usize,
    pub row: usize,
    /// Steps from the centre: right and up are positive.
    pub dx: i32,
    pub dy: i32,
    /// Number printed under the patch: 1 at the top left, row by row.
    pub number: usize,
    pub rgb: [u8; 3],
}

impl Cell {
    pub fn is_center(&self) -> bool {
        self.dx == 0 && self.dy == 0
    }
}

/// The `n × n` variants of `center` (`n` odd, 1..=9): horizontally along `x`, vertically along
/// `y`, `step` apart.
pub fn grid(center: [u8; 3], x: Axis, y: Axis, step: f64, n: usize) -> Vec<Cell> {
    let n = n.clamp(1, 9) | 1;
    let half = (n / 2) as i32;
    let step = if step.is_finite() { step.clamp(0.0, 100.0) } else { 0.0 };
    let mut out = Vec::with_capacity(n * n);
    for row in 0..n {
        for col in 0..n {
            let (dx, dy) = (col as i32 - half, half - row as i32);
            let rgb = variant(center, x, y, step, dx, dy);
            out.push(Cell { col, row, dx, dy, number: row * n + col + 1, rgb });
        }
    }
    out
}

/// The variant `dx` steps along `x` and `dy` along `y` from `center`.
pub fn variant(center: [u8; 3], x: Axis, y: Axis, step: f64, dx: i32, dy: i32) -> [u8; 3] {
    let hsl_axis = |a: Axis| matches!(a, Axis::Hue | Axis::Saturation | Axis::Lightness);
    if x != y && hsl_axis(x) && hsl_axis(y) {
        // Both HSL: one round trip, so neither axis disturbs the other.
        let mut hsl = rgb_to_hsl(center);
        for (a, k) in [(x, dx), (y, dy)] {
            let amount = step * f64::from(k);
            match a {
                Axis::Hue => hsl[0] += amount,
                Axis::Saturation => hsl[1] += amount / 100.0,
                _ => hsl[2] += amount / 100.0,
            }
        }
        return hsl_to_rgb(hsl);
    }
    shift(shift(center, x, step * f64::from(dx)), y, step * f64::from(dy))
}

/// "L+10 S−5"-style label of a variant's offsets (empty for the centre).
pub fn offset_label(x: Axis, y: Axis, step: f64, dx: i32, dy: i32) -> String {
    let part = |a: Axis, k: i32| {
        let v = step * f64::from(k);
        let sign = if v < 0.0 { "−" } else { "+" };
        let n = v.abs();
        let num = if (n - n.round()).abs() < 1e-6 { format!("{n:.0}") } else { format!("{n:.1}") };
        format!("{}{sign}{num}", a.letter_fr())
    };
    let mut parts = Vec::new();
    if dx != 0 {
        parts.push(part(x, dx));
    }
    if dy != 0 {
        parts.push(part(y, dy));
    }
    parts.join(" ")
}

/// "#A43B2F".
pub fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// Reads "#a43b2f", "A43B2F" or "#a3b" (spaces ignored).
pub fn parse_hex(text: &str) -> Option<[u8; 3]> {
    let t: String = text.chars().filter(|c| !c.is_whitespace()).collect();
    let t = t.strip_prefix('#').unwrap_or(&t);
    if !t.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let digits: Vec<u8> = t.chars().filter_map(|c| c.to_digit(16)).map(|d| d as u8).collect();
    match digits.as_slice() {
        [r, g, b] => Some([r * 17, g * 17, b * 17]),
        [r1, r0, g1, g0, b1, b0] => Some([r1 * 16 + r0, g1 * 16 + g0, b1 * 16 + b0]),
        _ => None,
    }
}

/// The printed sheet, in millimetres.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Board {
    /// Sheet width.
    pub width: f64,
    /// Side of a patch.
    pub patch: f64,
    /// Distance from one patch to the next (patch plus gap).
    pub pitch: f64,
    /// Patches per side of a block (odd).
    pub grid: usize,
}

/// Margin all around the sheet.
pub const BOARD_MARGIN: f64 = 10.0;
/// Header: title, settings, fields to fill in, the visual.
pub const BOARD_HEADER: f64 = 32.0;
/// Above a block's patches: its title and axes.
pub const BLOCK_TITLE: f64 = 11.0;
/// Under each row of patches: number, code and offsets.
pub const CELL_LABEL: f64 = 7.0;
/// Under a block: the line where the chosen patch is written.
pub const BLOCK_FOOT: f64 = 8.0;
/// Between two blocks.
pub const BLOCK_GAP: f64 = 8.0;

impl Default for Board {
    fn default() -> Self {
        Board { width: 560.0, patch: 15.0, pitch: 16.5, grid: 5 }
    }
}

impl Board {
    /// A block's width and height.
    pub fn block_size(&self) -> (f64, f64) {
        let n = self.grid as f64;
        (n * self.pitch, BLOCK_TITLE + n * (self.pitch + CELL_LABEL) + BLOCK_FOOT)
    }

    /// Blocks per row (at least one).
    pub fn columns(&self) -> usize {
        let (bw, _) = self.block_size();
        let room = self.width - 2.0 * BOARD_MARGIN + BLOCK_GAP;
        ((room / (bw + BLOCK_GAP)).floor() as usize).max(1)
    }

    /// Does one block fit across the sheet?
    pub fn fits(&self) -> bool {
        self.block_size().0 <= self.width - 2.0 * BOARD_MARGIN + 1e-9
    }

    /// Top-left corner of block `i`.
    pub fn block_origin(&self, i: usize) -> (f64, f64) {
        let (bw, bh) = self.block_size();
        let cols = self.columns();
        let (c, r) = (i % cols, i / cols);
        (BOARD_MARGIN + c as f64 * (bw + BLOCK_GAP), BOARD_MARGIN + BOARD_HEADER + r as f64 * (bh + BLOCK_GAP))
    }

    /// Sheet height for `blocks` blocks.
    pub fn height(&self, blocks: usize) -> f64 {
        let (_, bh) = self.block_size();
        let rows = blocks.max(1).div_ceil(self.columns()) as f64;
        2.0 * BOARD_MARGIN + BOARD_HEADER + rows * bh + (rows - 1.0) * BLOCK_GAP
    }

    /// A patch: left, top and side, in a block whose corner is `origin`.
    pub fn patch_rect(&self, origin: (f64, f64), col: usize, row: usize) -> (f64, f64, f64) {
        let gap = (self.pitch - self.patch).max(0.0) / 2.0;
        (origin.0 + col as f64 * self.pitch + gap, origin.1 + BLOCK_TITLE + row as f64 * (self.pitch + CELL_LABEL), self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `w × h` RGBA filled by `f(x, y)`.
    fn image(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        let mut v = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                v.extend_from_slice(&f(x, y));
            }
        }
        v
    }

    #[test]
    fn flat_colours_read_exactly_and_transparency_is_ignored() {
        // Left third red, middle third blue, right third transparent green.
        let px = image(90, 30, |x, _| match x {
            0..30 => [200, 30, 40, 255],
            30..60 => [20, 60, 180, 255],
            _ => [0, 255, 0, 10],
        });
        let s = extract(&px, 90, 30, 6, 40.0);
        assert_eq!(s.len(), 2);
        assert!(s.iter().any(|c| c.rgb == [200, 30, 40]));
        assert!(s.iter().any(|c| c.rgb == [20, 60, 180]));
        assert!((s[0].share - 0.5).abs() < 1e-9);
        assert!(extract(&[], 0, 0, 6, 40.0).is_empty());
        assert!(extract(&image(4, 4, |_, _| [0, 0, 0, 0]), 4, 4, 6, 40.0).is_empty());
    }

    #[test]
    fn anti_aliasing_merges_and_the_count_is_capped() {
        // A red area with a thin edge of shades between red and white.
        let px = image(1000, 20, |x, _| match x {
            0..700 => [210, 20, 30, 255],
            700..702 => [230, 120, 125, 255],
            702..704 => [240, 200, 200, 255],
            _ => [255, 255, 255, 255],
        });
        let s = extract(&px, 1000, 20, 6, 40.0);
        assert_eq!(s.len(), 2, "{s:?}");
        assert_eq!(s[0].rgb, [210, 20, 30], "the main colour stays exact: {s:?}");
        assert_eq!(s[1].rgb, [255, 255, 255]);
        // Every visible pixel counts for one colour.
        assert!((s[0].share + s[1].share - 1.0).abs() < 1e-9);
        // At most `max_colors`, the most frequent first.
        let bands = image(80, 10, |x, _| {
            let k = (x / 10) as u32;
            [(k * 30) as u8, (255 - k * 30) as u8, ((k * 70) % 255) as u8, 255]
        });
        let s = extract(&bands, 80, 10, 3, 10.0);
        assert_eq!(s.len(), 3);
        assert!(s.windows(2).all(|w| w[0].share >= w[1].share));
    }

    #[test]
    fn hsl_round_trips() {
        for rgb in [[0, 0, 0], [255, 255, 255], [164, 59, 47], [12, 200, 90], [30, 60, 250], [128, 128, 128], [255, 0, 255]] {
            assert_eq!(hsl_to_rgb(rgb_to_hsl(rgb)), rgb, "{rgb:?}");
        }
        let hsl = rgb_to_hsl([255, 0, 0]);
        assert!((hsl[0] - 0.0).abs() < 1e-9 && (hsl[1] - 1.0).abs() < 1e-9 && (hsl[2] - 0.5).abs() < 1e-9);
    }

    #[test]
    fn grid_is_centred_and_oriented() {
        let c = [164, 59, 47];
        let g = grid(c, Axis::Lightness, Axis::Saturation, 5.0, 5);
        assert_eq!(g.len(), 25);
        let mid = g[12];
        assert!(mid.is_center());
        assert_eq!((mid.number, mid.rgb), (13, c));
        // Right = lighter, top = more saturated.
        let l = |rgb: [u8; 3]| rgb_to_hsl(rgb)[2];
        let s = |rgb: [u8; 3]| rgb_to_hsl(rgb)[1];
        assert!(l(g[14].rgb) > l(g[13].rgb) && l(g[13].rgb) > l(mid.rgb) && l(g[10].rgb) < l(mid.rgb));
        assert!(s(g[2].rgb) > s(g[7].rgb) && s(g[7].rgb) > s(mid.rgb) && s(g[22].rgb) < s(mid.rgb));
        assert_eq!((g[0].dx, g[0].dy, g[0].number), (-2, 2, 1));
        assert_eq!((g[24].dx, g[24].dy, g[24].number), (2, -2, 25));
        // Two steps of 5 % lightness: about 10 points.
        assert!((l(g[14].rgb) - l(mid.rgb) - 0.10).abs() < 0.01);
        // An even size is made odd; a zero step repeats the centre.
        assert_eq!(grid(c, Axis::Red, Axis::Blue, 5.0, 4).len(), 25);
        assert!(grid(c, Axis::Hue, Axis::Lightness, 0.0, 3).iter().all(|k| k.rgb == c));
    }

    #[test]
    fn channel_and_hue_shifts() {
        assert_eq!(shift([100, 100, 100], Axis::Red, 10.0), [126, 100, 100]);
        assert_eq!(shift([250, 100, 100], Axis::Red, 10.0), [255, 100, 100]);
        assert_eq!(shift([100, 100, 100], Axis::Blue, -100.0), [100, 100, 0]);
        // Red turned 120° is green.
        assert_eq!(shift([255, 0, 0], Axis::Hue, 120.0), [0, 255, 0]);
        assert_eq!(variant([255, 0, 0], Axis::Hue, Axis::Red, 60.0, 2, 0), [0, 255, 0]);
    }

    #[test]
    fn labels_and_hex() {
        assert_eq!(offset_label(Axis::Lightness, Axis::Saturation, 5.0, 2, -1), "L+10 S−5");
        assert_eq!(offset_label(Axis::Hue, Axis::Red, 2.5, -1, 0), "T−2.5");
        assert_eq!(offset_label(Axis::Hue, Axis::Red, 5.0, 0, 0), "");
        assert_eq!(hex([164, 59, 47]), "#A43B2F");
        assert_eq!(parse_hex("#a43b2f"), Some([164, 59, 47]));
        assert_eq!(parse_hex(" A43B2F "), Some([164, 59, 47]));
        assert_eq!(parse_hex("#a3b"), Some([170, 51, 187]));
        for bad in ["", "#", "a43b2", "#zz0000", "#a43b2f00", "é"] {
            assert_eq!(parse_hex(bad), None, "{bad}");
        }
        for a in Axis::ALL {
            assert_eq!(Axis::from_id(a.id()), Some(a));
        }
    }

    #[test]
    fn board_geometry() {
        let b = Board::default();
        // 5 × 16.5 mm = 82.5 mm wide; six blocks across 56 cm.
        assert!((b.block_size().0 - 82.5).abs() < 1e-9);
        assert_eq!(b.columns(), 6);
        assert!(b.fits());
        let (_, bh) = b.block_size();
        assert!((b.height(6) - (2.0 * BOARD_MARGIN + BOARD_HEADER + bh)).abs() < 1e-9);
        assert!((b.height(7) - (2.0 * BOARD_MARGIN + BOARD_HEADER + 2.0 * bh + BLOCK_GAP)).abs() < 1e-9);
        assert_eq!(b.block_origin(6), (BOARD_MARGIN, BOARD_MARGIN + BOARD_HEADER + bh + BLOCK_GAP));
        // Patches are centred in their pitch.
        let (x, y, s) = b.patch_rect((10.0, 42.0), 1, 0);
        assert!((x - (10.0 + 16.5 + 0.75)).abs() < 1e-9 && (y - (42.0 + BLOCK_TITLE)).abs() < 1e-9 && (s - 15.0).abs() < 1e-9);
        assert!(!Board { width: 50.0, ..b }.fits());
        assert_eq!(Board { width: 50.0, ..b }.columns(), 1);
    }
}
