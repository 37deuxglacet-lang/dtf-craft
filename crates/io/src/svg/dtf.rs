// DTF Craft (Stéphane Glacet, 2026), MIT OR Apache-2.0. See MODIFICATIONS.md.
//! SVG helpers for DTF Craft's gang sheet: a drawing's print size, and part of it rendered at a
//! given size. A child module of `svg`, so PhotoCraft's `svg.rs` only declares it.

use resvg::tiny_skia;

use super::{inflate, parse};
use crate::IoError;

/// Largest render [`rasterize_region`] makes (DTF Craft's gang sheet: a 40 × 98 cm visual at
/// 300 ppi is 55 million pixels).
pub const MAX_REGION_PIXELS: u64 = 80_000_000;

/// A drawing's size in its own units (CSS px, 96 to the inch) and whether its root `<svg>` gives
/// its width in a physical unit (mm, cm, in, pt, pc): a size it is meant to print at. DTF Craft.
pub fn drawing_size(bytes: &[u8]) -> Result<(f32, f32, bool), IoError> {
    let tree = parse(bytes)?.tree;
    let xml = inflate(bytes)?;
    let physical = root_width(&xml).is_some_and(|w| {
        let w = w.trim().to_ascii_lowercase();
        ["mm", "cm", "in", "pt", "pc"].iter().any(|u| w.ends_with(u))
    });
    Ok((tree.size().width(), tree.size().height(), physical))
}

/// The `width` attribute of the first `<svg>` start tag, as written.
fn root_width(xml: &[u8]) -> Option<String> {
    let text = String::from_utf8_lossy(xml);
    let mut from = 0;
    let start = loop {
        let at = from + text.get(from..)?.find("<svg")?;
        if text.get(at + 4..at + 5).is_some_and(|c| c.chars().all(|c| c.is_whitespace() || c == '>' || c == '/')) {
            break at + 4;
        }
        from = at + 4;
    };
    // The tag runs to the first `>` outside quotes.
    let (mut quote, mut end) = (None, text.len());
    for (k, c) in text.get(start..)?.char_indices() {
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => {}
            None if c == '"' || c == '\'' => quote = Some(c),
            None if c == '>' => {
                end = start + k;
                break;
            }
            None => {}
        }
    }
    let tag = text.get(start..end)?;
    let mut rest = tag;
    while let Some(i) = rest.find("width") {
        let before = rest.get(..i).and_then(|b| b.chars().last());
        let after = rest.get(i + 5..).unwrap_or("").trim_start();
        // `width`, not `stroke-width` nor `data-width`, followed by `=`.
        if before.is_some_and(char::is_whitespace) && after.starts_with('=') {
            let value = after.get(1..).unwrap_or("").trim_start();
            let q = value.chars().next().filter(|c| *c == '"' || *c == '\'')?;
            let inner = value.get(1..)?;
            return inner.find(q).and_then(|e| inner.get(..e)).map(str::to_string);
        }
        rest = rest.get(i + 5..)?;
    }
    None
}

/// Part of the drawing, stretched to `full_w × full_h` px, as straight (unpremultiplied) RGBA
/// bytes: the `w × h` px box whose corner is at (`x0`, `y0`) of that full render. Refused past
/// [`MAX_REGION_PIXELS`]. DTF Craft's gang sheet renders each SVG this way at its print size.
pub fn rasterize_region(bytes: &[u8], full_w: f64, full_h: f64, x0: f64, y0: f64, w: u32, h: u32) -> Result<Vec<u8>, IoError> {
    if w == 0 || h == 0 || !(full_w.is_finite() && full_h.is_finite() && full_w > 0.0 && full_h > 0.0 && x0.is_finite() && y0.is_finite()) {
        return Err(IoError::Svg("nothing to render".to_string()));
    }
    if u64::from(w) * u64::from(h) > MAX_REGION_PIXELS {
        return Err(IoError::Svg(format!("{w} × {h} px is too large to rasterise")));
    }
    let tree = parse(bytes)?.tree;
    let (dw, dh) = (f64::from(tree.size().width().max(1e-3)), f64::from(tree.size().height().max(1e-3)));
    let mut pixmap = tiny_skia::Pixmap::new(w, h).ok_or_else(|| IoError::Svg(format!("{w} × {h} px is too large to rasterise")))?;
    let (sx, sy) = (full_w / dw, full_h / dh);
    resvg::render(&tree, tiny_skia::Transform::from_row(sx as f32, 0.0, 0.0, sy as f32, -x0 as f32, -y0 as f32), &mut pixmap.as_mut());
    let mut out = Vec::with_capacity(pixmap.pixels().len() * 4);
    for p in pixmap.pixels() {
        let c = p.demultiply();
        out.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drawing_size_tells_a_physical_size_and_regions_render_stretched() {
        let cm = br##"<svg xmlns="http://www.w3.org/2000/svg" stroke-width="3" width="10cm" height="5cm" viewBox="0 0 100 50"><rect x="0" y="0" width="50" height="50" fill="#ff0000"/></svg>"##;
        let (w, h, physical) = drawing_size(cm).unwrap();
        assert!(physical);
        assert!((w - 377.95).abs() < 0.1 && (h - 188.98).abs() < 0.1, "{w} {h}");
        let px = br##"<svg xmlns="http://www.w3.org/2000/svg" width="200" height="100"><rect width="100" height="100" fill="#00ff00"/></svg>"##;
        assert_eq!(drawing_size(px).unwrap(), (200.0, 100.0, false));
        assert!(!drawing_size(br##"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 10 10"/>"##).unwrap().2);
        // The full drawing at 40 × 20 px: the left half green, the right half transparent.
        let full = rasterize_region(px, 40.0, 20.0, 0.0, 0.0, 40, 20).unwrap();
        assert_eq!(full.len(), 40 * 20 * 4);
        assert_eq!(full.get(..4), Some(&[0u8, 255, 0, 255][..]));
        assert_eq!(full.get((10 * 40 + 30) * 4 + 3), Some(&0));
        // Only the right half of a 400 × 200 render.
        let right = rasterize_region(px, 400.0, 200.0, 200.0, 0.0, 200, 200).unwrap();
        assert!(right.chunks_exact(4).all(|p| p[3] == 0));
        assert!(rasterize_region(px, 0.0, 20.0, 0.0, 0.0, 4, 4).is_err());
        assert!(rasterize_region(px, 1e6, 1e6, 0.0, 0.0, 20_000, 20_000).is_err());
    }
}
