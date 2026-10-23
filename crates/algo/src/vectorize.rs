// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF Craft's Vectorisation: a raster visual (straight RGBA, read as sRGB) turned into an SVG of
//! cut-out shapes, one flat colour per shape and no shape over another.
//!
//! 1. The colours are reduced to a palette of at most N colours ([`dtf_color::analyze`], a
//!    k-means in L*a*b*) and every visible pixel takes the nearest one; transparent pixels
//!    (alpha < 0.5) stay out.
//! 2. Areas smaller than the detail level allows are given to the colour around them, so no gap
//!    opens where they were.
//! 3. The background is the colour on the visual's edge: on the image border when the visual
//!    reaches it, else on the border of its visible area when that area is a full rectangle (a
//!    visual with a transparent margin). Removing it drops the background areas joined to that
//!    edge; the same colour inside the visual stays.
//! 4. Each colour's areas are traced ([`visioncortex`], MIT OR Apache-2.0, the tracer of VTracer)
//!    into curves or straight segments, holes included, and written as one compound path per
//!    colour.
//!
//! Visuals larger than [`MAX_SIDE`] are traced on a box-averaged copy; the SVG keeps the
//! original's size.

use photocraft_color::convert::srgb_to_lab;
use serde::{Deserialize, Serialize};
use visioncortex::{BinaryImage, CompoundPathElement, PathSimplifyMode, PointF64};

use crate::dtf_color;

/// The longest side traced, in px; a larger visual is traced on a reduced copy.
pub const MAX_SIDE: usize = 2500;
/// The most colours in a palette.
pub const MAX_COLORS: usize = 32;
/// Index of a transparent pixel in [`Quantized::index`].
pub const TRANSPARENT: u8 = u8::MAX;

/// Flat colours (logo) or more nuances (illustration).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Kind {
    /// The colours are read inside the flat areas and those closer than ΔE 5 grouped, so the
    /// antialiased contours don't make colours of their own.
    #[default]
    Logo,
    /// Every colour of the palette is kept, as many as asked.
    Illustration,
}

/// The smallest area kept and how closely the curves follow the pixels.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Detail {
    Low,
    #[default]
    Medium,
    High,
}

/// Curves, or straight segments with sharp corners.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Smoothing {
    #[default]
    Curves,
    Corners,
}

/// What the palette is made of.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// At most this many colours (2–32).
    pub colors: usize,
    pub kind: Kind,
    pub detail: Detail,
}

impl Default for Settings {
    fn default() -> Self {
        Self { colors: 8, kind: Kind::Logo, detail: Detail::Medium }
    }
}

impl Detail {
    /// Areas under this many px are given to the colour around them, at 1000 px a side
    /// (the area grows with the square of the visual's size).
    fn min_area(self) -> f64 {
        match self {
            Detail::Low => 40.0,
            Detail::Medium => 12.0,
            Detail::High => 3.0,
        }
    }

    /// The tracer's segment length, in px (its range is 3.5–10).
    fn segment_length(self) -> f64 {
        match self {
            Detail::Low => 6.0,
            Detail::Medium => 4.0,
            Detail::High => 3.5,
        }
    }
}

/// One colour of the palette.
#[derive(Clone, Debug, PartialEq)]
pub struct PaletteColor {
    /// sRGB, 8 bits.
    pub rgb: [u8; 3],
    /// Share of the visible pixels, 0–1.
    pub share: f32,
    /// Pixels of this colour (before the background is removed).
    pub pixels: usize,
}

/// The visual reduced to its palette.
#[derive(Clone, Debug, PartialEq)]
pub struct Quantized {
    /// The size traced (the visual's, or its reduced copy's).
    pub width: usize,
    pub height: usize,
    /// One palette index per pixel, [`TRANSPARENT`] for the transparent ones.
    pub index: Vec<u8>,
    /// Largest share first.
    pub palette: Vec<PaletteColor>,
    /// The background colour (index in `palette`), when the visual has one.
    pub background: Option<usize>,
    /// The pixels the background search read, as indices in `index`: where its removal starts.
    edge: Vec<usize>,
}

/// One colour's shapes.
#[derive(Clone, Debug, PartialEq)]
pub struct ColorPath {
    /// Index in the palette.
    pub color: usize,
    /// SVG path data, absolute, in the traced pixels' units.
    pub d: String,
    /// Separate areas of this colour.
    pub shapes: usize,
    /// Anchor points (segment ends) of the path.
    pub points: usize,
}

/// How the shapes are traced.
#[derive(Clone, Debug, PartialEq)]
pub struct TraceOptions {
    pub detail: Detail,
    pub smoothing: Smoothing,
    pub remove_background: bool,
    /// Palette colours left out of the SVG (their shapes are dropped, not merged): one flag per
    /// palette colour; missing flags count as kept.
    pub excluded: Vec<bool>,
}

/// The traced visual.
#[derive(Clone, Debug, PartialEq)]
pub struct Traced {
    pub width: usize,
    pub height: usize,
    /// One entry per colour that has shapes, largest share first.
    pub paths: Vec<ColorPath>,
}

impl Traced {
    pub fn shapes(&self) -> usize {
        self.paths.iter().map(|p| p.shapes).sum()
    }

    pub fn points(&self) -> usize {
        self.paths.iter().map(|p| p.points).sum()
    }
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

/// `#RRGGBB`.
pub fn hex(rgb: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", rgb[0], rgb[1], rgb[2])
}

/// A box-averaged copy of `px` (`w` × `h`, straight RGBA) whose longest side is at most `max`.
/// Colours are averaged by alpha, so transparent pixels don't darken the edges.
pub fn reduce(w: usize, h: usize, px: &[[f32; 4]], max: usize) -> (usize, usize, Vec<[f32; 4]>) {
    let f = w.max(h).div_ceil(max.max(1)).max(1);
    if f == 1 {
        return (w, h, px.to_vec());
    }
    let (rw, rh) = (w.div_ceil(f), h.div_ceil(f));
    let mut out = Vec::with_capacity(rw * rh);
    for ry in 0..rh {
        for rx in 0..rw {
            let (mut c, mut a, mut n) = ([0.0f64; 3], 0.0f64, 0.0f64);
            for y in ry * f..((ry + 1) * f).min(h) {
                for x in rx * f..((rx + 1) * f).min(w) {
                    let Some(q) = px.get(y * w + x) else { continue };
                    let qa = f64::from(q[3].clamp(0.0, 1.0));
                    for k in 0..3 {
                        c[k] += f64::from(q[k]) * qa;
                    }
                    a += qa;
                    n += 1.0;
                }
            }
            let rgb = if a > 0.0 { c.map(|v| (v / a) as f32) } else { [0.0; 3] };
            out.push([rgb[0], rgb[1], rgb[2], if n > 0.0 { (a / n) as f32 } else { 0.0 }]);
        }
    }
    (rw, rh, out)
}

/// Reduces the visual `px` (`w` × `h`, straight RGBA, sRGB) to its palette.
pub fn quantize(w: usize, h: usize, px: &[[f32; 4]], s: &Settings) -> Result<Quantized, String> {
    if w == 0 || h == 0 || w.checked_mul(h) != Some(px.len()) {
        return Err(format!("{w} × {h} px does not match {} pixels", px.len()));
    }
    let (w, h, px) = reduce(w, h, px, MAX_SIDE);
    let max = s.colors.clamp(2, MAX_COLORS);
    let main = match s.kind {
        // A logo's colours are read inside its flat areas, so the antialiased contours between
        // two colours don't become colours of their own (they go to the nearest one below).
        Kind::Logo => {
            let core = flat_pixels(w, h, &px);
            let visible = px.iter().filter(|q| q[3] >= 0.5).count();
            if core.len() * 10 >= visible { dtf_color::analyze(&core, max, 5.0, 0.002) } else { dtf_color::analyze(&px, max, 5.0, 0.005) }
        }
        Kind::Illustration => dtf_color::analyze(&px, max, 0.0, 0.0),
    };
    if main.is_empty() {
        return Err("no visible pixels to vectorize".into());
    }
    let centers: Vec<[f32; 3]> = main.iter().map(|c| c.lab).collect();
    // Every visible pixel takes the nearest colour (cached by 8-bit colour).
    let mut cache: std::collections::HashMap<u32, u8> = std::collections::HashMap::new();
    let mut index: Vec<u8> = px
        .iter()
        .map(|q| {
            if q[3] < 0.5 {
                return TRANSPARENT;
            }
            let rgb = [to8(q[0]), to8(q[1]), to8(q[2])];
            let key = (u32::from(rgb[0]) << 16) | (u32::from(rgb[1]) << 8) | u32::from(rgb[2]);
            *cache.entry(key).or_insert_with(|| {
                let lab = srgb_to_lab(rgb.map(|v| f32::from(v) / 255.0));
                (0..centers.len()).min_by(|a, b| dist2(lab, centers[*a]).total_cmp(&dist2(lab, centers[*b]))).unwrap_or(0) as u8
            })
        })
        .collect();
    let scale = (w.max(h) as f64 / 1000.0).max(1.0);
    let min_area = (s.detail.min_area() * scale * scale).round().max(1.0) as usize;
    for _ in 0..2 {
        if !absorb_small_areas(w, h, &mut index, min_area) {
            break;
        }
    }
    // The palette, largest share first.
    let mut counts = vec![0usize; main.len()];
    for i in &index {
        if let Some(c) = counts.get_mut(usize::from(*i)) {
            *c += 1;
        }
    }
    let visible: usize = counts.iter().sum();
    let mut order: Vec<usize> = (0..main.len()).filter(|i| counts[*i] > 0).collect();
    order.sort_by(|a, b| counts[*b].cmp(&counts[*a]).then(a.cmp(b)));
    let mut remap = vec![TRANSPARENT; main.len()];
    for (new, old) in order.iter().enumerate() {
        remap[*old] = new as u8;
    }
    for i in index.iter_mut() {
        if *i != TRANSPARENT {
            *i = remap.get(usize::from(*i)).copied().unwrap_or(TRANSPARENT);
        }
    }
    let palette: Vec<PaletteColor> = order
        .iter()
        .map(|old| PaletteColor { rgb: main[*old].rgb.map(to8), share: counts[*old] as f32 / visible.max(1) as f32, pixels: counts[*old] })
        .collect();
    let (background, edge) = background(w, h, &index, palette.len());
    Ok(Quantized { width: w, height: h, index, palette, background, edge })
}

/// The visible pixels whose 4 neighbours are visible and of the same colour (within 2 % a
/// channel): the inside of flat areas.
fn flat_pixels(w: usize, h: usize, px: &[[f32; 4]]) -> Vec<[f32; 4]> {
    let same = |a: &[f32; 4], b: &[f32; 4]| b[3] >= 0.5 && (0..3).all(|k| (a[k] - b[k]).abs() <= 0.02);
    let mut out = Vec::new();
    for y in 0..h {
        for x in 0..w {
            let q = &px[y * w + x];
            if q[3] >= 0.5 && neighbours(x, y, w, h).iter().all(|j| j.is_some_and(|j| same(q, &px[j]))) {
                out.push(*q);
            }
        }
    }
    out
}

/// Gives each 4-connected area of one colour smaller than `min_area` px to the colour most
/// around it (or to transparency). True when anything changed.
fn absorb_small_areas(w: usize, h: usize, index: &mut [u8], min_area: usize) -> bool {
    if min_area <= 1 {
        return false;
    }
    let n = w * h;
    let mut seen = vec![false; n];
    let mut area: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();
    let mut changed = false;
    for start in 0..n {
        let v = index[start];
        if seen[start] || v == TRANSPARENT {
            continue;
        }
        // The area of `start`'s colour, while it stays small enough to absorb.
        area.clear();
        stack.clear();
        seen[start] = true;
        stack.push(start);
        let mut big = false;
        while let Some(i) = stack.pop() {
            if !big {
                area.push(i);
                big = area.len() >= min_area;
            }
            let (x, y) = (i % w, i / w);
            for j in neighbours(x, y, w, h).into_iter().flatten() {
                if !seen[j] && index[j] == v {
                    seen[j] = true;
                    stack.push(j);
                }
            }
        }
        if big {
            continue;
        }
        // The colours around it (outside the image counts as transparent).
        let mut votes = [0usize; 256];
        for &i in &area {
            let (x, y) = (i % w, i / w);
            for j in neighbours(x, y, w, h) {
                match j {
                    Some(j) if index[j] != v => votes[usize::from(index[j])] += 1,
                    Some(_) => {}
                    None => votes[usize::from(TRANSPARENT)] += 1,
                }
            }
        }
        let Some((winner, _)) = votes.iter().enumerate().filter(|(_, n)| **n > 0).max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(&a.0))) else {
            continue;
        };
        for &i in &area {
            index[i] = winner as u8;
        }
        changed = true;
    }
    changed
}

/// The 4 neighbours of (x, y); `None` outside the image.
fn neighbours(x: usize, y: usize, w: usize, h: usize) -> [Option<usize>; 4] {
    [
        (x > 0).then(|| y * w + x - 1),
        (x + 1 < w).then(|| y * w + x + 1),
        (y > 0).then(|| (y - 1) * w + x),
        (y + 1 < h).then(|| (y + 1) * w + x),
    ]
}

/// The background colour and the edge pixels it was read on. On the image border when at least
/// half of it is visible; else on the border of the visible area, when its four corners are
/// visible and of one colour (a rectangular background with a transparent margin). `None` for a
/// visual already cut out.
fn background(w: usize, h: usize, index: &[u8], colors: usize) -> (Option<usize>, Vec<usize>) {
    let ring = |x0: usize, y0: usize, x1: usize, y1: usize| -> Vec<usize> {
        let mut r = Vec::new();
        for x in x0..x1 {
            r.push(y0 * w + x);
            if y1 - y0 > 1 {
                r.push((y1 - 1) * w + x);
            }
        }
        for y in y0 + 1..y1.saturating_sub(1) {
            r.push(y * w + x0);
            if x1 - x0 > 1 {
                r.push(y * w + x1 - 1);
            }
        }
        r
    };
    let majority = |cells: &[usize]| -> Option<(usize, usize)> {
        let mut votes = vec![0usize; colors];
        for &i in cells {
            if let Some(v) = index.get(i).and_then(|c| votes.get_mut(usize::from(*c))) {
                *v += 1;
            }
        }
        votes.iter().enumerate().max_by(|a, b| a.1.cmp(b.1).then(b.0.cmp(&a.0))).filter(|(_, n)| **n > 0).map(|(c, n)| (c, *n))
    };
    let border = ring(0, 0, w, h);
    let visible = border.iter().filter(|i| index.get(**i).is_some_and(|c| *c != TRANSPARENT)).count();
    if visible * 2 >= border.len() {
        let colour = majority(&border).filter(|(_, n)| *n * 2 >= visible).map(|(c, _)| c);
        return (colour, if colour.is_some() { border } else { Vec::new() });
    }
    // The visible area's bounding box.
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0, 0);
    for y in 0..h {
        for x in 0..w {
            if index[y * w + x] != TRANSPARENT {
                (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x1 <= x0 || y1 <= y0 {
        return (None, Vec::new());
    }
    let corners = [y0 * w + x0, y0 * w + x1 - 1, (y1 - 1) * w + x0, (y1 - 1) * w + x1 - 1].map(|i| index[i]);
    if corners.contains(&TRANSPARENT) || corners.iter().any(|c| *c != corners[0]) {
        return (None, Vec::new());
    }
    let edge = ring(x0, y0, x1, y1);
    match majority(&edge) {
        Some((c, n)) if c == usize::from(corners[0]) && n * 10 >= edge.len() * 8 => (Some(c), edge),
        _ => (None, Vec::new()),
    }
}

impl Quantized {
    /// The palette index of every pixel, with the background areas joined to the visual's edge
    /// made transparent when `remove` is set.
    pub fn without_background(&self, remove: bool) -> Vec<u8> {
        let mut index = self.index.clone();
        let Some(bg) = self.background.filter(|_| remove) else { return index };
        let bg = bg as u8;
        let (w, h) = (self.width, self.height);
        let mut stack: Vec<usize> = Vec::new();
        for &i in &self.edge {
            if index.get(i) == Some(&bg) {
                index[i] = TRANSPARENT;
                stack.push(i);
            }
        }
        while let Some(i) = stack.pop() {
            for j in neighbours(i % w, i / w, w, h).into_iter().flatten() {
                if index[j] == bg {
                    index[j] = TRANSPARENT;
                    stack.push(j);
                }
            }
        }
        index
    }

    /// Pixels of each palette colour once the background is removed (when `remove`).
    pub fn kept_pixels(&self, remove: bool) -> Vec<usize> {
        let mut out = vec![0usize; self.palette.len()];
        for i in self.without_background(remove) {
            if let Some(c) = out.get_mut(usize::from(i)) {
                *c += 1;
            }
        }
        out
    }

    /// The traced shapes: one compound path per colour kept.
    pub fn trace(&self, o: &TraceOptions) -> Result<Traced, String> {
        let (w, h) = (self.width, self.height);
        let index = self.without_background(o.remove_background);
        let mode = match o.smoothing {
            Smoothing::Curves => PathSimplifyMode::Spline,
            Smoothing::Corners => PathSimplifyMode::Polygon,
        };
        let mut paths = Vec::new();
        for color in 0..self.palette.len() {
            if o.excluded.get(color).copied().unwrap_or(false) {
                continue;
            }
            let mut image = BinaryImage::new_w_h(w, h);
            let mut any = false;
            for (i, c) in index.iter().enumerate() {
                if usize::from(*c) == color {
                    image.set_pixel(i % w, i / w, true);
                    any = true;
                }
            }
            if !any {
                continue;
            }
            // The tracer is a dependency: a panic in it becomes an error, never a crash.
            let traced = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| trace_colour(&image, mode, o.detail)))
                .map_err(|_| format!("the tracer failed on colour {}", hex(self.palette[color].rgb)))?;
            if let Some((d, shapes, points)) = traced {
                paths.push(ColorPath { color, d, shapes, points });
            }
        }
        Ok(Traced { width: w, height: h, paths })
    }
}

/// One colour's areas as path data: (d, areas, points). `None` when nothing was traced.
fn trace_colour(image: &BinaryImage, mode: PathSimplifyMode, detail: Detail) -> Option<(String, usize, usize)> {
    let corner = 60f64.to_radians();
    let splice = 45f64.to_radians();
    let mut d = String::new();
    let (mut shapes, mut points) = (0, 0);
    for cluster in image.to_clusters(false).iter() {
        let compound = cluster.to_compound_path(mode, corner, detail.segment_length(), 10, splice);
        let before = points;
        for element in compound.iter() {
            points += write_element(&mut d, element);
        }
        if points > before {
            shapes += 1;
        }
    }
    (shapes > 0).then_some((d, shapes, points))
}

fn num(v: f64) -> String {
    let r = (v * 100.0).round() / 100.0;
    let s = format!("{r:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" { "0".into() } else { s.to_string() }
}

/// Appends one closed subpath (absolute commands); returns its anchor points.
fn write_element(d: &mut String, e: &CompoundPathElement) -> usize {
    let pts: Vec<PointF64> = match e {
        CompoundPathElement::PathI32(p) => p.path.iter().map(|q| PointF64::new(f64::from(q.x), f64::from(q.y))).collect(),
        CompoundPathElement::PathF64(p) => p.path.clone(),
        CompoundPathElement::Spline(s) => {
            let p = &s.points;
            if p.len() < 4 {
                return 0;
            }
            if !d.is_empty() {
                d.push(' ');
            }
            d.push_str(&format!("M{} {}", num(p[0].x), num(p[0].y)));
            let mut n = 0;
            for c in p[1..].chunks_exact(3) {
                d.push_str(&format!("C{} {} {} {} {} {}", num(c[0].x), num(c[0].y), num(c[1].x), num(c[1].y), num(c[2].x), num(c[2].y)));
                n += 1;
            }
            d.push('Z');
            return n;
        }
    };
    // A polygon: its last point repeats the first.
    let mut pts = pts;
    if pts.len() > 1 && pts.first() == pts.last() {
        pts.pop();
    }
    if pts.len() < 3 {
        return 0;
    }
    if !d.is_empty() {
        d.push(' ');
    }
    d.push_str(&format!("M{} {}", num(pts[0].x), num(pts[0].y)));
    for p in &pts[1..] {
        d.push_str(&format!("L{} {}", num(p.x), num(p.y)));
    }
    d.push('Z');
    pts.len()
}

/// How the SVG's size is given.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SvgSize {
    /// The print size, in mm (the exported file).
    Mm(f64, f64),
    /// A size in px (opening in DTF Craft at the document's size, or a preview).
    Px(f64, f64),
}

/// The SVG document: one `<path>` per colour, named by its colour. With `outline`, the shapes'
/// contours only (the preview's "Show contours"), `outline` px wide.
pub fn svg(t: &Traced, palette: &[PaletteColor], size: SvgSize, outline: Option<f64>) -> String {
    let (w, h) = match size {
        SvgSize::Mm(w, h) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => (format!("{}mm", num(w)), format!("{}mm", num(h))),
        SvgSize::Px(w, h) if w.is_finite() && h.is_finite() && w > 0.0 && h > 0.0 => (num(w), num(h)),
        _ => (t.width.to_string(), t.height.to_string()),
    };
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<!-- DTF Craft, vectorisation -->\n");
    out.push_str(&format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" version=\"1.1\" width=\"{w}\" height=\"{h}\" viewBox=\"0 0 {} {}\">\n",
        t.width, t.height
    ));
    for p in &t.paths {
        let Some(c) = palette.get(p.color) else { continue };
        let colour = hex(c.rgb);
        let id = format!("couleur-{}", colour.trim_start_matches('#'));
        match outline {
            Some(width) => out.push_str(&format!(
                "<path id=\"{id}\" d=\"{}\" fill=\"none\" stroke=\"#FF2FA0\" stroke-width=\"{}\" stroke-linejoin=\"round\"/>\n",
                p.d,
                num(width)
            )),
            None => out.push_str(&format!("<path id=\"{id}\" d=\"{}\" fill=\"{colour}\"/>\n", p.d)),
        }
    }
    out.push_str("</svg>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const BLUE: [f32; 4] = [31.0 / 255.0, 79.0 / 255.0, 181.0 / 255.0, 1.0];
    const ORANGE: [f32; 4] = [228.0 / 255.0, 87.0 / 255.0, 46.0 / 255.0, 1.0];
    const WHITE: [f32; 4] = [1.0, 1.0, 1.0, 1.0];

    /// 100 × 80 on white: a blue disc (r 30) with a white dot (r 6) at its centre, an orange
    /// bar, and one lone orange pixel.
    fn logo() -> Vec<[f32; 4]> {
        let mut px = vec![WHITE; 100 * 80];
        for y in 0..80 {
            for x in 0..100 {
                let (dx, dy) = (x as f32 - 35.5, y as f32 - 40.5);
                let r = (dx * dx + dy * dy).sqrt();
                if r < 30.0 {
                    px[y * 100 + x] = if r < 6.0 { WHITE } else { BLUE };
                }
                if (75..90).contains(&x) && (10..70).contains(&y) {
                    px[y * 100 + x] = ORANGE;
                }
            }
        }
        px[5 * 100 + 50] = ORANGE;
        px
    }

    fn colours(q: &Quantized) -> Vec<String> {
        q.palette.iter().map(|c| hex(c.rgb)).collect()
    }

    #[test]
    fn the_palette_keeps_the_main_colours_and_finds_the_white_background() {
        let q = quantize(100, 80, &logo(), &Settings::default()).unwrap();
        assert_eq!(colours(&q), ["#FFFFFF", "#1F4FB5", "#E4572E"]);
        assert_eq!(q.background, Some(0));
        assert!((q.palette.iter().map(|c| c.share).sum::<f32>() - 1.0).abs() < 1e-4);
        // The lone orange pixel went to the white around it.
        assert_eq!(q.index[5 * 100 + 50], 0);
        // Removing the background keeps the white dot inside the disc.
        let kept = q.kept_pixels(true);
        assert!(kept[0] > 50 && kept[0] < 200, "the inner white dot stays: {kept:?}");
        assert_eq!(q.kept_pixels(false)[0], q.palette[0].pixels);
    }

    #[test]
    fn cut_out_shapes_cover_each_colour_once() {
        let q = quantize(100, 80, &logo(), &Settings::default()).unwrap();
        let o = TraceOptions { detail: Detail::Medium, smoothing: Smoothing::Curves, remove_background: true, excluded: vec![] };
        let t = q.trace(&o).unwrap();
        // White: the inner dot only. Blue: the disc with its hole. Orange: the bar.
        assert_eq!(t.paths.iter().map(|p| (p.color, p.shapes)).collect::<Vec<_>>(), [(0, 1), (1, 1), (2, 1)]);
        let blue = &t.paths[1].d;
        assert_eq!(blue.matches('M').count(), 2, "outline and hole: {blue}");
        assert!(blue.contains('C') && !blue.contains('L'));
        // Straight segments with sharp corners.
        let p = q.trace(&TraceOptions { smoothing: Smoothing::Corners, ..o.clone() }).unwrap();
        assert!(p.paths[2].d.contains('L') && !p.paths[2].d.contains('C'));
        assert!(p.paths[2].points >= 4);
        // A colour left out has no path; with the background kept, white is one more area.
        let ex = q.trace(&TraceOptions { excluded: vec![false, false, true], ..o.clone() }).unwrap();
        assert_eq!(ex.paths.len(), 2);
        let kept = q.trace(&TraceOptions { remove_background: false, ..o }).unwrap();
        assert_eq!(kept.paths[0].shapes, 2);
    }

    #[test]
    fn the_svg_has_the_print_size_and_one_path_per_colour() {
        let q = quantize(100, 80, &logo(), &Settings::default()).unwrap();
        let o = TraceOptions { detail: Detail::High, smoothing: Smoothing::Curves, remove_background: true, excluded: vec![] };
        let t = q.trace(&o).unwrap();
        let s = svg(&t, &q.palette, SvgSize::Mm(16.93, 13.55), None);
        assert!(s.contains("width=\"16.93mm\" height=\"13.55mm\" viewBox=\"0 0 100 80\""), "{s}");
        assert_eq!(s.matches("<path ").count(), 3);
        assert!(s.contains("id=\"couleur-1F4FB5\"") && s.contains("fill=\"#E4572E\""));
        let outline = svg(&t, &q.palette, SvgSize::Px(200.0, 160.0), Some(1.5));
        assert!(outline.contains("width=\"200\"") && outline.contains("fill=\"none\"") && outline.contains("stroke-width=\"1.5\""));
    }

    #[test]
    fn a_transparent_visual_has_no_background_and_a_margin_is_seen_through() {
        // The disc logo cut out (white made transparent): nothing to remove.
        let cut: Vec<[f32; 4]> = logo().into_iter().map(|q| if q == WHITE { [1.0, 1.0, 1.0, 0.0] } else { q }).collect();
        let q = quantize(100, 80, &cut, &Settings::default()).unwrap();
        assert_eq!((colours(&q), q.background), (vec!["#1F4FB5".to_string(), "#E4572E".to_string()], None));
        // A white rectangle with a 2 px transparent margin: the background is still found.
        let mut margin = logo();
        for y in 0..80 {
            for x in 0..100 {
                if x < 2 || y < 2 || x >= 98 || y >= 78 {
                    margin[y * 100 + x] = [0.0; 4];
                }
            }
        }
        let q = quantize(100, 80, &margin, &Settings::default()).unwrap();
        assert_eq!(q.background, Some(0));
        assert!(q.kept_pixels(true)[0] < 200);
    }

    #[test]
    fn logos_group_their_contour_colours_and_illustrations_keep_them() {
        // Two flat colours with a blended column between them.
        let mut px = Vec::new();
        for _ in 0..40 {
            for x in 0..60 {
                px.push(match x {
                    0..=29 => BLUE,
                    30 => [0.5, 0.33, 0.45, 1.0],
                    _ => ORANGE,
                });
            }
        }
        let logo = quantize(60, 40, &px, &Settings { colors: 8, ..Settings::default() }).unwrap();
        assert_eq!(logo.palette.len(), 2);
        let illus = quantize(60, 40, &px, &Settings { colors: 8, kind: Kind::Illustration, detail: Detail::High }).unwrap();
        assert_eq!(illus.palette.len(), 3);
    }

    #[test]
    fn large_visuals_are_traced_on_a_reduced_copy() {
        let px = vec![BLUE; 6000 * 10];
        let (w, h, small) = reduce(6000, 10, &px, MAX_SIDE);
        assert_eq!((w, h), (2000, 4));
        assert!(small.iter().all(|q| (q[2] - BLUE[2]).abs() < 1e-6 && q[3] == 1.0));
        let q = quantize(6000, 10, &px, &Settings::default()).unwrap();
        assert_eq!((q.width, q.height), (2000, 4));
    }

    #[test]
    fn bad_input_is_an_error() {
        assert!(quantize(0, 0, &[], &Settings::default()).is_err());
        assert!(quantize(10, 10, &[WHITE; 5], &Settings::default()).is_err());
        assert!(quantize(4, 4, &[[0.0; 4]; 16], &Settings::default()).is_err(), "nothing visible");
        // A single visible pixel traces.
        let mut px = vec![[0.0; 4]; 9];
        px[4] = BLUE;
        let q = quantize(3, 3, &px, &Settings { detail: Detail::High, ..Settings::default() }).unwrap();
        let t = q.trace(&TraceOptions { detail: Detail::High, smoothing: Smoothing::Curves, remove_background: false, excluded: vec![] }).unwrap();
        assert!(t.paths.len() <= 1);
    }
}
