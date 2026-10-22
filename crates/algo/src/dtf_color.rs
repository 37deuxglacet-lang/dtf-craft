// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF Craft's Colour Analyser: the colours of a visual (sRGB, transparency ignored) grouped into
//! a few main colours, matched to the nearest colours of a swatch book the user loads (.ase,
//! .aco, .csv) by CIEDE2000, and replaced by them for export.
//!
//! Colours are compared in CIE L*a*b* D50 (as swatch books store them). The replacement shifts
//! each pixel by the offset of the main colours near it (weighted by closeness), so a flat area
//! takes the swatch colour exactly while its antialiased contour and its shading stay smooth.

use photocraft_color::convert::{lab_to_srgb, srgb_to_lab};

/// A swatch book colour: its reference and its L*a*b* (D50).
#[derive(Clone, Debug, PartialEq)]
pub struct Swatch {
    pub name: String,
    pub lab: [f32; 3],
}

/// One of the visual's main colours.
#[derive(Clone, Debug, PartialEq)]
pub struct MainColor {
    /// Encoded sRGB, 0–1, on 8-bit steps.
    pub rgb: [f32; 3],
    pub lab: [f32; 3],
    /// Share of the analysed (visible) pixels, 0–1.
    pub share: f32,
}

/// CIEDE2000 colour difference between two L*a*b* colours.
pub fn delta_e2000(a: [f32; 3], b: [f32; 3]) -> f32 {
    let (l1, a1, b1) = (f64::from(a[0]), f64::from(a[1]), f64::from(a[2]));
    let (l2, a2, b2) = (f64::from(b[0]), f64::from(b[1]), f64::from(b[2]));
    let c1 = (a1 * a1 + b1 * b1).sqrt();
    let c2 = (a2 * a2 + b2 * b2).sqrt();
    let cm = (c1 + c2) / 2.0;
    let g = 0.5 * (1.0 - (cm.powi(7) / (cm.powi(7) + 25f64.powi(7))).sqrt());
    let (a1p, a2p) = ((1.0 + g) * a1, (1.0 + g) * a2);
    let (c1p, c2p) = ((a1p * a1p + b1 * b1).sqrt(), (a2p * a2p + b2 * b2).sqrt());
    let hue = |b: f64, a: f64| {
        if a == 0.0 && b == 0.0 {
            0.0
        } else {
            let h = b.atan2(a).to_degrees();
            if h < 0.0 { h + 360.0 } else { h }
        }
    };
    let (h1p, h2p) = (hue(b1, a1p), hue(b2, a2p));
    let dl = l2 - l1;
    let dc = c2p - c1p;
    let dh = if c1p * c2p == 0.0 {
        0.0
    } else if (h2p - h1p).abs() <= 180.0 {
        h2p - h1p
    } else if h2p - h1p > 180.0 {
        h2p - h1p - 360.0
    } else {
        h2p - h1p + 360.0
    };
    let dhh = 2.0 * (c1p * c2p).sqrt() * (dh.to_radians() / 2.0).sin();
    let lm = (l1 + l2) / 2.0;
    let cmp = (c1p + c2p) / 2.0;
    let hm = if c1p * c2p == 0.0 {
        h1p + h2p
    } else if (h1p - h2p).abs() <= 180.0 {
        (h1p + h2p) / 2.0
    } else if h1p + h2p < 360.0 {
        (h1p + h2p + 360.0) / 2.0
    } else {
        (h1p + h2p - 360.0) / 2.0
    };
    let t = 1.0 - 0.17 * (hm - 30.0).to_radians().cos() + 0.24 * (2.0 * hm).to_radians().cos() + 0.32 * (3.0 * hm + 6.0).to_radians().cos()
        - 0.20 * (4.0 * hm - 63.0).to_radians().cos();
    let dtheta = 30.0 * (-((hm - 275.0) / 25.0).powi(2)).exp();
    let rc = 2.0 * (cmp.powi(7) / (cmp.powi(7) + 25f64.powi(7))).sqrt();
    let sl = 1.0 + 0.015 * (lm - 50.0).powi(2) / (20.0 + (lm - 50.0).powi(2)).sqrt();
    let sc = 1.0 + 0.045 * cmp;
    let sh = 1.0 + 0.015 * cmp * t;
    let rt = -(2.0 * dtheta).to_radians().sin() * rc;
    let (x, y, z) = (dl / sl, dc / sc, dhh / sh);
    (x * x + y * y + z * z + rt * y * z).sqrt() as f32
}

fn dist2(a: [f32; 3], b: [f32; 3]) -> f32 {
    (a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)
}

/// 8-bit RGB as one key.
fn key(rgb: [f32; 3]) -> u32 {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u32;
    (q(rgb[0]) << 16) | (q(rgb[1]) << 8) | q(rgb[2])
}

fn unkey(k: u32) -> [f32; 3] {
    [((k >> 16) & 255) as f32 / 255.0, ((k >> 8) & 255) as f32 / 255.0, (k & 255) as f32 / 255.0]
}

/// The visible colours of straight-RGBA pixels (alpha ≥ 0.5) as weighted 5-bit-per-channel bins:
/// (mean L*a*b*, mean sRGB, pixel count). Fast to cluster, whatever the image size.
pub fn histogram(px: &[[f32; 4]]) -> Vec<([f32; 3], [f32; 3], f32)> {
    let mut bins: std::collections::HashMap<u32, ([f64; 3], f64)> = std::collections::HashMap::new();
    for q in px.iter().filter(|q| q[3] >= 0.5) {
        let b = |v: f32| (v.clamp(0.0, 1.0) * 31.0).round() as u32;
        let k = (b(q[0]) << 10) | (b(q[1]) << 5) | b(q[2]);
        let e = bins.entry(k).or_insert(([0.0; 3], 0.0));
        for (sum, v) in e.0.iter_mut().zip(q) {
            *sum += f64::from(*v);
        }
        e.1 += 1.0;
    }
    let mut out: Vec<([f32; 3], [f32; 3], f32)> = bins
        .into_values()
        .map(|(sum, n)| {
            let rgb = [(sum[0] / n) as f32, (sum[1] / n) as f32, (sum[2] / n) as f32];
            (srgb_to_lab(rgb), rgb, n as f32)
        })
        .collect();
    // A stable order, whatever the hash map's.
    out.sort_by(|a, b| b.2.total_cmp(&a.2).then(key(a.1).cmp(&key(b.1))));
    out
}

/// The main colours of `px` (straight RGBA): at most `max_colors`, those closer than
/// `merge_delta_e` merged, those under `min_share` (0–1) of the visible pixels left out. Sorted by
/// share, largest first.
pub fn analyze(px: &[[f32; 4]], max_colors: usize, merge_delta_e: f32, min_share: f32) -> Vec<MainColor> {
    let bins = histogram(px);
    let total: f32 = bins.iter().map(|b| b.2).sum();
    if bins.is_empty() || total <= 0.0 {
        return Vec::new();
    }
    let k = max_colors.clamp(1, 32).min(bins.len());
    // Seeds: the most frequent bin, then the bin that adds most (weight × distance²).
    let mut centers: Vec<[f32; 3]> = vec![bins[0].0];
    while centers.len() < k {
        let next = bins.iter().map(|b| (b.2.sqrt() * centers.iter().map(|c| dist2(b.0, *c)).fold(f32::MAX, f32::min), b.0)).max_by(|a, b| a.0.total_cmp(&b.0));
        match next {
            Some((score, lab)) if score > 0.0 => centers.push(lab),
            _ => break,
        }
    }
    // Weighted k-means in L*a*b*.
    let nearest = |lab: [f32; 3], centers: &[[f32; 3]]| -> usize {
        let mut best = (0, f32::MAX);
        for (i, c) in centers.iter().enumerate() {
            let d = dist2(lab, *c);
            if d < best.1 {
                best = (i, d);
            }
        }
        best.0
    };
    for _ in 0..20 {
        let mut sums = vec![([0.0f64; 3], 0.0f64); centers.len()];
        for b in &bins {
            let i = nearest(b.0, &centers);
            for c in 0..3 {
                sums[i].0[c] += f64::from(b.0[c]) * f64::from(b.2);
            }
            sums[i].1 += f64::from(b.2);
        }
        let mut moved = false;
        for (i, (s, n)) in sums.iter().enumerate() {
            if *n > 0.0 {
                let c = [(s[0] / n) as f32, (s[1] / n) as f32, (s[2] / n) as f32];
                moved |= dist2(c, centers[i]) > 1e-4;
                centers[i] = c;
            }
        }
        if !moved {
            break;
        }
    }
    // Members of each centre: weight and sRGB sum (the colour shown is the members' mean).
    let mut groups: Vec<([f32; 3], [f64; 3], f64)> = centers.iter().map(|c| (*c, [0.0; 3], 0.0)).collect();
    for b in &bins {
        let i = nearest(b.0, &centers);
        for c in 0..3 {
            groups[i].1[c] += f64::from(b.1[c]) * f64::from(b.2);
        }
        groups[i].2 += f64::from(b.2);
    }
    groups.retain(|g| g.2 > 0.0);
    // Merge colours closer than the grouping difference, the smaller into the larger.
    loop {
        let mut pair = None;
        for i in 0..groups.len() {
            for j in i + 1..groups.len() {
                let de = delta_e2000(groups[i].0, groups[j].0);
                if de < merge_delta_e && pair.is_none_or(|(_, _, d)| de < d) {
                    pair = Some((i, j, de));
                }
            }
        }
        let Some((i, j, _)) = pair else { break };
        let (gj, gi) = (groups.remove(j), groups[i]);
        let n = gi.2 + gj.2;
        let lab = [0, 1, 2].map(|c| ((f64::from(gi.0[c]) * gi.2 + f64::from(gj.0[c]) * gj.2) / n) as f32);
        groups[i] = (lab, [0, 1, 2].map(|c| gi.1[c] + gj.1[c]), n);
    }
    let mut out: Vec<MainColor> = groups
        .into_iter()
        .map(|(_, sum, n)| {
            // The members' mean colour: a flat area's own colour, so replacing it is exact.
            let rgb = unkey(key([0, 1, 2].map(|c| (sum[c] / n) as f32)));
            MainColor { rgb, lab: srgb_to_lab(rgb), share: (n / f64::from(total)) as f32 }
        })
        .filter(|c| c.share >= min_share)
        .collect();
    out.sort_by(|a, b| b.share.total_cmp(&a.share));
    out
}

/// The share of the visible pixels nearest each of `centers` (L*a*b*): the shares after a colour
/// is added by hand.
pub fn shares(px: &[[f32; 4]], centers: &[[f32; 3]]) -> Vec<f32> {
    let bins = histogram(px);
    let total: f32 = bins.iter().map(|b| b.2).sum();
    let mut out = vec![0.0f32; centers.len()];
    if centers.is_empty() || total <= 0.0 {
        return out;
    }
    for b in &bins {
        let i = (0..centers.len()).min_by(|x, y| dist2(b.0, centers[*x]).total_cmp(&dist2(b.0, centers[*y]))).unwrap_or(0);
        out[i] += b.2 / total;
    }
    out
}

/// The swatch nearest `lab` by CIEDE2000, and that difference.
pub fn nearest_swatch(swatches: &[Swatch], lab: [f32; 3]) -> Option<(usize, f32)> {
    swatches.iter().enumerate().map(|(i, s)| (i, delta_e2000(lab, s.lab))).min_by(|a, b| a.1.total_cmp(&b.1))
}

/// Replace the main colours in straight-RGBA pixels: `centers` are the main colours (L*a*b*) with
/// the colour each becomes (`None`: kept). Each pixel moves by the closeness-weighted offsets of
/// the centres, so a pixel of a main colour takes its target exactly and the contours stay smooth.
/// Alpha is kept.
pub fn replace(px: &mut [[f32; 4]], centers: &[([f32; 3], Option<[f32; 3]>)]) {
    if !centers.iter().any(|c| c.1.is_some()) {
        return;
    }
    let shifts: Vec<([f32; 3], [f32; 3])> = centers.iter().map(|(c, t)| (*c, t.map_or([0.0; 3], |t| [t[0] - c[0], t[1] - c[1], t[2] - c[2]]))).collect();
    let mut cache: std::collections::HashMap<u32, [f32; 3]> = std::collections::HashMap::new();
    for q in px.iter_mut() {
        if q[3] <= 0.0 {
            continue;
        }
        let k = key([q[0], q[1], q[2]]);
        let out = *cache.entry(k).or_insert_with(|| {
            let lab = srgb_to_lab(unkey(k));
            let (mut wsum, mut shift) = (0.0f32, [0.0f32; 3]);
            for (c, s) in &shifts {
                let d2 = dist2(lab, *c);
                let w = 1.0 / (d2 * d2 + 1e-6);
                wsum += w;
                for i in 0..3 {
                    shift[i] += w * s[i];
                }
            }
            if wsum <= 0.0 || shift.iter().all(|v| *v == 0.0) {
                return unkey(k);
            }
            let moved = [lab[0] + shift[0] / wsum, lab[1] + shift[1] / wsum, lab[2] + shift[2] / wsum];
            lab_to_srgb([moved[0].clamp(0.0, 100.0), moved[1], moved[2]])
        });
        q[0] = out[0];
        q[1] = out[1];
        q[2] = out[2];
    }
}

/// `#rrggbb` of encoded sRGB 0–1.
pub fn hex(rgb: [f32; 3]) -> String {
    let q = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{:02X}{:02X}{:02X}", q(rgb[0]), q(rgb[1]), q(rgb[2]))
}

/// The sRGB of a swatch (clipped to the sRGB gamut).
pub fn swatch_rgb(s: &Swatch) -> [f32; 3] {
    lab_to_srgb(s.lab).map(|v| v.clamp(0.0, 1.0))
}

// ------------------------------------------------------------------ swatch books

/// Read a swatch book by its file name's extension: Adobe Swatch Exchange (.ase), Photoshop
/// swatches (.aco), or text (.csv: `reference;R;G;B` 0–255, `reference;#rrggbb`, or with a
/// first line naming L, a, b: `reference;L;a;b`).
pub fn parse_swatch_book(file_name: &str, bytes: &[u8]) -> Result<Vec<Swatch>, String> {
    let ext = std::path::Path::new(file_name).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).unwrap_or_default();
    let swatches = match ext.as_str() {
        "ase" => parse_ase(bytes)?,
        "aco" => parse_aco(bytes)?,
        "csv" | "txt" => parse_csv(&String::from_utf8_lossy(bytes))?,
        _ => return Err(format!("{file_name}: swatch books are .ase, .aco or .csv files")),
    };
    if swatches.is_empty() {
        return Err(format!("{file_name}: no colour found"));
    }
    Ok(swatches)
}

fn cmyk_lab(c: [f32; 4]) -> [f32; 3] {
    srgb_to_lab(photocraft_color::convert::cmyk_to_rgb_naive(c))
}

/// Adobe Swatch Exchange: "ASEF", version, block count, then blocks (group start / end, colour
/// entry: UTF-16 name, model "RGB " | "CMYK" | "LAB " | "Gray", big-endian f32 values, type).
pub fn parse_ase(b: &[u8]) -> Result<Vec<Swatch>, String> {
    if b.len() < 12 || &b[0..4] != b"ASEF" {
        return Err("not an Adobe Swatch Exchange file".into());
    }
    let u16_at = |i: usize| b.get(i..i + 2).map(|s| u16::from_be_bytes([s[0], s[1]]));
    let u32_at = |i: usize| b.get(i..i + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    let f32_at = |i: usize| b.get(i..i + 4).map(|s| f32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    let count = u32_at(8).ok_or("truncated file")? as usize;
    let mut i = 12;
    let mut out = Vec::new();
    for _ in 0..count {
        let (Some(kind), Some(len)) = (u16_at(i), u32_at(i + 2)) else { break };
        let start = i + 6;
        let end = start + len as usize;
        if end > b.len() {
            break;
        }
        if kind == 0x0001 {
            let n = u16_at(start).unwrap_or(0) as usize;
            let units: Vec<u16> = (0..n).filter_map(|k| u16_at(start + 2 + 2 * k)).collect();
            let name = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
            let m = start + 2 + 2 * n;
            let model = b.get(m..m + 4).unwrap_or(b"    ");
            let v = |k: usize| f32_at(m + 4 + 4 * k).unwrap_or(0.0);
            let lab = match model {
                b"RGB " => Some(srgb_to_lab([v(0), v(1), v(2)])),
                b"CMYK" => Some(cmyk_lab([v(0), v(1), v(2), v(3)])),
                // L is stored 0–1, a and b as they are.
                b"LAB " => Some([if v(0) <= 1.0 { v(0) * 100.0 } else { v(0) }, v(1), v(2)]),
                b"Gray" => Some(srgb_to_lab([v(0); 3])),
                _ => None,
            };
            if let Some(lab) = lab {
                out.push(Swatch { name: if name.is_empty() { format!("{}", out.len() + 1) } else { name }, lab });
            }
        }
        i = end;
    }
    Ok(out)
}

/// Photoshop swatches: version 1 (no names), then usually version 2 (with names). Colour spaces:
/// 0 RGB, 1 HSB, 2 CMYK, 7 Lab, 8 Grayscale.
pub fn parse_aco(b: &[u8]) -> Result<Vec<Swatch>, String> {
    let u16_at = |i: usize| b.get(i..i + 2).map(|s| u16::from_be_bytes([s[0], s[1]]));
    let u32_at = |i: usize| b.get(i..i + 4).map(|s| u32::from_be_bytes([s[0], s[1], s[2], s[3]]));
    let section = |at: usize| -> Option<(u16, Vec<Swatch>, usize)> {
        let version = u16_at(at)?;
        let count = u16_at(at + 2)? as usize;
        let mut i = at + 4;
        let mut out = Vec::new();
        for n in 0..count {
            let space = u16_at(i)?;
            let w = [u16_at(i + 2)?, u16_at(i + 4)?, u16_at(i + 6)?, u16_at(i + 8)?];
            i += 10;
            let mut name = format!("{}", n + 1);
            if version == 2 {
                let len = u32_at(i)? as usize;
                let units: Vec<u16> = (0..len).filter_map(|k| u16_at(i + 4 + 2 * k)).collect();
                name = String::from_utf16_lossy(&units).trim_end_matches('\0').to_string();
                i += 4 + 2 * len;
            }
            let f = |v: u16| f32::from(v) / 65535.0;
            let lab = match space {
                0 => Some(srgb_to_lab([f(w[0]), f(w[1]), f(w[2])])),
                1 => Some(srgb_to_lab(hsb_to_rgb(f(w[0]) * 360.0, f(w[1]), f(w[2])))),
                // CMYK is stored inverted: 0 = 100 % ink.
                2 => Some(cmyk_lab([1.0 - f(w[0]), 1.0 - f(w[1]), 1.0 - f(w[2]), 1.0 - f(w[3])])),
                7 => Some([f32::from(w[0]) / 100.0, f32::from(w[1] as i16) / 100.0, f32::from(w[2] as i16) / 100.0]),
                8 => Some(srgb_to_lab([1.0 - f32::from(w[0]) / 10000.0; 3])),
                _ => None,
            };
            if let Some(lab) = lab {
                out.push(Swatch { name, lab });
            }
        }
        Some((version, out, i))
    };
    let (version, first, end) = section(0).ok_or("not a Photoshop swatches file")?;
    if version == 1
        && let Some((2, named, _)) = section(end)
        && !named.is_empty()
    {
        return Ok(named);
    }
    if version != 1 && version != 2 {
        return Err("not a Photoshop swatches file".into());
    }
    Ok(first)
}

fn hsb_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let hp = (h / 60.0).rem_euclid(6.0);
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// Text swatches: one colour a line, `reference;R;G;B` (0–255), `reference;#rrggbb`, or
/// `reference;L;a;b` when the first line names L, a and b. `;`, `,` or tab separate the fields;
/// a header line without numbers is skipped.
pub fn parse_csv(text: &str) -> Result<Vec<Swatch>, String> {
    let mut lab_mode = false;
    let mut out = Vec::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim().trim_start_matches('\u{feff}');
        if line.is_empty() {
            continue;
        }
        let sep = if line.contains(';') {
            ';'
        } else if line.contains('\t') {
            '\t'
        } else {
            ','
        };
        let f: Vec<&str> = line.split(sep).map(|s| s.trim().trim_matches('"')).collect();
        let nums: Vec<f32> = f.iter().skip(1).filter_map(|s| s.replace(',', ".").parse::<f32>().ok()).collect();
        if n == 0 && nums.is_empty() && !f.iter().any(|s| s.starts_with('#')) {
            let lower: Vec<String> = f.iter().map(|s| s.to_ascii_lowercase()).collect();
            lab_mode = lower.iter().any(|s| s == "l" || s == "l*") && lower.iter().any(|s| s == "a" || s == "a*");
            continue;
        }
        let name = f.first().copied().unwrap_or("").to_string();
        let lab = if let Some(h) = f.get(1).filter(|s| s.starts_with('#') && s.len() == 7) {
            let c = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|v| f32::from(v) / 255.0);
            match (c(1), c(3), c(5)) {
                (Some(r), Some(g), Some(b)) => srgb_to_lab([r, g, b]),
                _ => return Err(format!("line {}: bad colour {h}", n + 1)),
            }
        } else if nums.len() >= 3 {
            if lab_mode { [nums[0], nums[1], nums[2]] } else { srgb_to_lab([nums[0] / 255.0, nums[1] / 255.0, nums[2] / 255.0]) }
        } else {
            return Err(format!("line {}: expected a reference and a colour", n + 1));
        };
        out.push(Swatch { name: if name.is_empty() { format!("{}", out.len() + 1) } else { name }, lab });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ciede2000_matches_the_published_pairs() {
        // Sharma, Wu & Dalal (2005), test data pairs 1, 7 and 17.
        let cases = [
            ([50.0, 2.6772, -79.7751], [50.0, 0.0, -82.7485], 2.0425),
            ([50.0, 0.0, 0.0], [50.0, -1.0, 2.0], 2.3669),
            ([50.0, 2.5, 0.0], [73.0, 25.0, -18.0], 27.1492),
        ];
        for (a, b, want) in cases {
            assert!((delta_e2000(a, b) - want).abs() < 1e-3, "{a:?} {b:?}: {} ≠ {want}", delta_e2000(a, b));
        }
        assert_eq!(delta_e2000([40.0, 10.0, -5.0], [40.0, 10.0, -5.0]), 0.0);
    }

    fn logo() -> Vec<[f32; 4]> {
        // 100 × 100: 60 % blue, 30 % orange, 10 % transparent; one antialiased column between.
        let mut px = Vec::new();
        for _y in 0..100 {
            for x in 0..100 {
                px.push(match x {
                    0..=9 => [0.0, 0.0, 0.0, 0.0],
                    10..=69 => [0.12, 0.23, 0.58, 1.0],
                    70 => [0.5, 0.3, 0.4, 1.0],
                    _ => [0.89, 0.34, 0.18, 1.0],
                });
            }
        }
        px
    }

    #[test]
    fn analysis_finds_the_main_colours_and_their_shares() {
        // The one-pixel contour column (1.1 %) is left out at 2 %.
        let colors = analyze(&logo(), 8, 5.0, 0.02);
        assert_eq!(colors.len(), 2, "{colors:?}");
        assert_eq!(hex(colors[0].rgb), "#1F3B94");
        assert!((colors[0].share - 60.0 / 90.0).abs() < 0.02 && (colors[1].share - 29.0 / 90.0).abs() < 0.02, "{colors:?}");
        // At 0.5 % it is a colour of its own; transparent pixels never count.
        let all = analyze(&logo(), 8, 5.0, 0.005);
        assert_eq!(all.len(), 3);
        assert!((all.iter().map(|c| c.share).sum::<f32>() - 1.0).abs() < 1e-4);
    }

    #[test]
    fn replacing_takes_flat_areas_to_the_swatch_exactly_and_keeps_alpha() {
        let mut px = logo();
        let colors = analyze(&px, 8, 5.0, 0.02);
        let target = srgb_to_lab([0.0, 0.2, 0.6]);
        let centers: Vec<_> = colors.iter().enumerate().map(|(i, c)| (c.lab, (i == 0).then_some(target))).collect();
        replace(&mut px, &centers);
        let at = |x: usize| px[50 * 100 + x];
        assert_eq!(hex([at(30)[0], at(30)[1], at(30)[2]]), "#003399");
        assert_eq!(hex([at(90)[0], at(90)[1], at(90)[2]]), "#E3572E", "kept colours stay (within rounding): {:?}", at(90));
        assert_eq!(at(5)[3], 0.0, "transparency stays");
        // The contour pixel moves, but less than the area it borders.
        let moved = dist2(srgb_to_lab([0.5, 0.3, 0.4]), srgb_to_lab([at(70)[0], at(70)[1], at(70)[2]])).sqrt();
        let full = dist2(colors[0].lab, target).sqrt();
        assert!(moved > 0.1 && moved < full, "{moved} / {full}");
    }

    #[test]
    fn swatch_books_read_ase_aco_and_csv() {
        // ASE: one RGB entry "Rouge" and one LAB entry "Bleu".
        let mut ase = b"ASEF".to_vec();
        ase.extend_from_slice(&[0, 1, 0, 0]);
        ase.extend_from_slice(&2u32.to_be_bytes());
        let entry = |name: &str, model: &[u8; 4], vals: &[f32]| {
            let units: Vec<u16> = name.encode_utf16().chain([0]).collect();
            let mut body = (units.len() as u16).to_be_bytes().to_vec();
            for u in &units {
                body.extend_from_slice(&u.to_be_bytes());
            }
            body.extend_from_slice(model);
            for v in vals {
                body.extend_from_slice(&v.to_be_bytes());
            }
            body.extend_from_slice(&2u16.to_be_bytes());
            let mut b = 1u16.to_be_bytes().to_vec();
            b.extend_from_slice(&(body.len() as u32).to_be_bytes());
            b.extend_from_slice(&body);
            b
        };
        ase.extend(entry("Rouge", b"RGB ", &[1.0, 0.0, 0.0]));
        ase.extend(entry("Bleu", b"LAB ", &[0.3, 20.0, -60.0]));
        let s = parse_swatch_book("lib.ase", &ase).unwrap();
        assert_eq!(s.len(), 2);
        assert_eq!((s[0].name.as_str(), hex(swatch_rgb(&s[0]))), ("Rouge", "#FF0000".to_string()));
        assert!(s[1].lab.iter().zip([30.0, 20.0, -60.0]).all(|(a, b)| (a - b).abs() < 1e-4), "{:?}", s[1].lab);
        // ACO version 1 + 2: one RGB green named "Vert".
        let mut aco = vec![0, 1, 0, 1, 0, 0, 0, 0, 0xff, 0xff, 0, 0, 0, 0];
        aco.extend_from_slice(&[0, 2, 0, 1, 0, 0, 0, 0, 0xff, 0xff, 0, 0, 0, 0]);
        aco.extend_from_slice(&5u32.to_be_bytes());
        for u in "Vert".encode_utf16().chain([0]) {
            aco.extend_from_slice(&u.to_be_bytes());
        }
        let s = parse_swatch_book("lib.aco", &aco).unwrap();
        assert_eq!((s[0].name.as_str(), hex(swatch_rgb(&s[0]))), ("Vert", "#00FF00".to_string()));
        // CSV: RGB, hex and a Lab file.
        let s = parse_swatch_book("lib.csv", b"Ref;R;V;B\nOrange 01;228;87;46\nNoir;#000000\n").unwrap();
        assert_eq!((s.len(), hex(swatch_rgb(&s[0])), hex(swatch_rgb(&s[1]))), (2, "#E4572E".to_string(), "#000000".to_string()));
        let s = parse_swatch_book("lab.csv", b"Ref;L;a;b\nGris 50;50;0;0\n").unwrap();
        assert_eq!(s[0].lab, [50.0, 0.0, 0.0]);
        assert!(parse_swatch_book("x.pdf", b"").is_err());
        // The nearest swatch.
        let book = parse_swatch_book("lib.csv", b"Ref;R;V;B\nRouge;255;0;0\nBleu;0;0;255\n").unwrap();
        let (i, de) = nearest_swatch(&book, srgb_to_lab([0.9, 0.05, 0.05])).unwrap();
        assert_eq!(book[i].name, "Rouge");
        assert!(de > 0.0 && de < 10.0);
    }
}
