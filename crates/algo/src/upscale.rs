//! AI upscaling (DTF Craft): Real-ESRGAN's compact general model (`realesr-general-x4v3`,
//! SRVGGNetCompact, BSD-3-Clause, see `models/README.md`), run on the CPU in safe Rust.
//!
//! The network is 34 3×3 convolutions with per-channel PReLU on 64 feature maps, a last
//! convolution to 48 maps, a pixel shuffle to 4× the size and the nearest-neighbour enlarged
//! input added back. Images are processed in overlapping tiles so memory stays bounded and the
//! work spreads over the cores; the overlap hides the seams.

use std::sync::OnceLock;

use photocraft_raster::Interrupt;

use crate::par;

/// The bundled model weights (converted from the official `.pth`, see `models/README.md`).
static MODEL_BYTES: &[u8] = include_bytes!("../models/realesr-general-x4v3.dtfsr");

/// Upscale factor of the network.
pub const SCALE: usize = 4;

#[derive(Clone, Debug)]
enum Layer {
    /// 3×3 convolution, zero padding 1: weights `[out][in][3][3]`, bias `[out]`.
    Conv { out: usize, inp: usize, w: Vec<f32>, b: Vec<f32> },
    /// Per-channel PReLU slopes.
    Prelu(Vec<f32>),
}

/// A loaded SRVGGNetCompact network.
#[derive(Clone, Debug)]
pub struct SrModel {
    layers: Vec<Layer>,
}

fn read_u32(b: &[u8], at: &mut usize) -> Result<u32, String> {
    let s = b.get(*at..*at + 4).ok_or("truncated model")?;
    *at += 4;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

fn read_f32s(b: &[u8], at: &mut usize, n: usize) -> Result<Vec<f32>, String> {
    let len = n.checked_mul(4).ok_or("model too large")?;
    let s = b.get(*at..*at + len).ok_or("truncated model")?;
    *at += len;
    Ok(s.chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect())
}

impl SrModel {
    /// Parses the `DTFSR1` weight file: a layer count, then per layer a kind byte (0 conv: out,
    /// in, kh, kw as u32 then weights and bias; 1 PReLU: count then slopes), all little-endian.
    pub fn parse(b: &[u8]) -> Result<Self, String> {
        if b.get(..8) != Some(b"DTFSR1\0\0".as_slice()) {
            return Err("not a DTF Craft super-resolution model".into());
        }
        let mut at = 8;
        let n = read_u32(b, &mut at)? as usize;
        if n == 0 || n > 256 {
            return Err(format!("bad layer count {n}"));
        }
        let mut layers = Vec::with_capacity(n);
        let mut channels = 3usize;
        for _ in 0..n {
            let kind = *b.get(at).ok_or("truncated model")?;
            at += 1;
            match kind {
                0 => {
                    let out = read_u32(b, &mut at)? as usize;
                    let inp = read_u32(b, &mut at)? as usize;
                    let (kh, kw) = (read_u32(b, &mut at)?, read_u32(b, &mut at)?);
                    if kh != 3 || kw != 3 || out == 0 || out > 1024 || inp != channels {
                        return Err(format!("unsupported convolution {out}×{inp}×{kh}×{kw}"));
                    }
                    let w = read_f32s(b, &mut at, out * inp * 9)?;
                    let bias = read_f32s(b, &mut at, out)?;
                    layers.push(Layer::Conv { out, inp, w, b: bias });
                    channels = out;
                }
                1 => {
                    let c = read_u32(b, &mut at)? as usize;
                    if c != channels {
                        return Err(format!("PReLU of {c} channels after {channels}"));
                    }
                    layers.push(Layer::Prelu(read_f32s(b, &mut at, c)?));
                }
                k => return Err(format!("unknown layer kind {k}")),
            }
        }
        if channels != 3 * SCALE * SCALE {
            return Err(format!("the network ends with {channels} channels, not {}", 3 * SCALE * SCALE));
        }
        if layers.iter().any(|l| match l {
            Layer::Conv { w, b, .. } => w.iter().chain(b).any(|v| !v.is_finite()),
            Layer::Prelu(p) => p.iter().any(|v| !v.is_finite()),
        }) {
            return Err("the model holds non-finite weights".into());
        }
        Ok(SrModel { layers })
    }

    /// The bundled model, parsed once.
    pub fn bundled() -> Result<&'static SrModel, String> {
        static MODEL: OnceLock<Result<SrModel, String>> = OnceLock::new();
        MODEL.get_or_init(|| SrModel::parse(MODEL_BYTES)).as_ref().map_err(Clone::clone)
    }

    /// Runs the network on one planar RGB tile (`3 × h × w`, 0–1) and returns the planar
    /// `3 × 4h × 4w` result, clamped to 0–1.
    pub fn run_planar(&self, rgb: &[f32], w: usize, h: usize) -> Vec<f32> {
        let (pw, ph) = (w + 2, h + 2);
        // Feature maps live in zero-bordered planes (the convolution's padding).
        let mut cur = vec![0.0f32; 3 * pw * ph];
        for c in 0..3 {
            for y in 0..h {
                let src = rgb.get((c * h + y) * w..(c * h + y) * w + w).unwrap_or(&[]);
                let at = c * pw * ph + (y + 1) * pw + 1;
                if let Some(dst) = cur.get_mut(at..at + src.len()) {
                    dst.copy_from_slice(src);
                }
            }
        }
        for layer in &self.layers {
            match layer {
                Layer::Conv { out, inp, w: wt, b } => {
                    let plane = pw * ph;
                    let mut next = vec![0.0f32; out * plane];
                    let input = &cur;
                    // Output channels in groups of `G`: each input row is loaded once per group.
                    const G: usize = 8;
                    par::chunks_mut(&mut next, plane * G, true, |g, dst| {
                        let o0 = g * G;
                        let n = (dst.len() / plane).min(G);
                        for (k, d) in dst.chunks_exact_mut(plane).enumerate() {
                            let bias = b.get(o0 + k).copied().unwrap_or(0.0);
                            for y in 0..h {
                                let row = (y + 1) * pw + 1;
                                if let Some(r) = d.get_mut(row..row + w) {
                                    r.fill(bias);
                                }
                            }
                        }
                        let mut acc = vec![0.0f32; G * w];
                        for y in 0..h {
                            acc.iter_mut().for_each(|v| *v = 0.0);
                            for c in 0..*inp {
                                let src = input.get(c * plane..(c + 1) * plane).unwrap_or(&[]);
                                for ky in 0..3 {
                                    let srow0 = (y + ky) * pw;
                                    for kx in 0..3 {
                                        let Some(s) = src.get(srow0 + kx..srow0 + kx + w) else { continue };
                                        for k in 0..n {
                                            let kv = wt.get(((o0 + k) * inp + c) * 9 + ky * 3 + kx).copied().unwrap_or(0.0);
                                            if let Some(a) = acc.get_mut(k * w..k * w + w) {
                                                for (av, sv) in a.iter_mut().zip(s) {
                                                    *av += kv * sv;
                                                }
                                            }
                                        }
                                    }
                                }
                            }
                            let drow = (y + 1) * pw + 1;
                            for (k, d) in dst.chunks_exact_mut(plane).enumerate() {
                                if let (Some(r), Some(a)) = (d.get_mut(drow..drow + w), acc.get(k * w..k * w + w)) {
                                    for (rv, av) in r.iter_mut().zip(a) {
                                        *rv += av;
                                    }
                                }
                            }
                        }
                    });
                    cur = next;
                }
                Layer::Prelu(a) => {
                    let plane = pw * ph;
                    par::chunks_mut(&mut cur, plane, true, |c, p| {
                        let s = a.get(c).copied().unwrap_or(0.25);
                        for v in p.iter_mut() {
                            if *v < 0.0 {
                                *v *= s;
                            }
                        }
                    });
                }
            }
        }
        // Pixel shuffle (PyTorch order: channel c·16 + i·4 + j → pixel (4y + i, 4x + j) of c)
        // plus the nearest-neighbour enlarged input.
        let (ow, oh) = (w * SCALE, h * SCALE);
        let mut out = vec![0.0f32; 3 * ow * oh];
        let plane = pw * ph;
        for c in 0..3 {
            for y in 0..h {
                for x in 0..w {
                    let base = rgb.get((c * h + y) * w + x).copied().unwrap_or(0.0);
                    for i in 0..SCALE {
                        for j in 0..SCALE {
                            let ch = c * SCALE * SCALE + i * SCALE + j;
                            let v = cur.get(ch * plane + (y + 1) * pw + x + 1).copied().unwrap_or(0.0) + base;
                            if let Some(o) = out.get_mut((c * oh + y * SCALE + i) * ow + x * SCALE + j) {
                                *o = v.clamp(0.0, 1.0);
                            }
                        }
                    }
                }
            }
        }
        out
    }

    /// Upscales an interleaved `w × h` image of `ch` channels (0–1; the first three are RGB, a
    /// fourth is alpha) by 4. RGB goes through the network; alpha too when `alpha_model`, else it
    /// is enlarged by nearest neighbour then softened (bilinear). Tiles of `tile` pixels with
    /// `pad` pixels of context. Returns `None` when cancelled.
    pub fn upscale(&self, px: &[f32], w: usize, h: usize, ch: usize, alpha_model: bool, tile: usize, pad: usize, ir: &Interrupt) -> Option<Vec<f32>> {
        if w == 0 || h == 0 || ch < 3 || px.len() < w * h * ch {
            return Some(Vec::new());
        }
        let tile = tile.clamp(16, 512);
        let (ow, oh) = (w * SCALE, h * SCALE);
        let mut out = vec![0.0f32; ow * oh * ch];
        let tiles_x = w.div_ceil(tile);
        let tiles_y = h.div_ceil(tile);
        let has_alpha = ch >= 4;
        let passes = if has_alpha && alpha_model { 2 } else { 1 };
        let total = (tiles_x * tiles_y * passes).max(1);
        let mut done = 0usize;
        for pass in 0..passes {
            for ty in 0..tiles_y {
                for tx in 0..tiles_x {
                    if ir.cancelled() {
                        return None;
                    }
                    let (x0, y0) = (tx * tile, ty * tile);
                    let (x1, y1) = ((x0 + tile).min(w), (y0 + tile).min(h));
                    let (ex0, ey0) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
                    let (ex1, ey1) = ((x1 + pad).min(w), (y1 + pad).min(h));
                    let (ew, eh) = (ex1 - ex0, ey1 - ey0);
                    let mut planar = vec![0.0f32; 3 * ew * eh];
                    for y in 0..eh {
                        for x in 0..ew {
                            let si = ((ey0 + y) * w + ex0 + x) * ch;
                            for c in 0..3 {
                                // Pass 0: the straight colour; pass 1: the alpha as grey.
                                let v = if pass == 0 { px.get(si + c) } else { px.get(si + 3) };
                                planar[(c * eh + y) * ew + x] = v.copied().unwrap_or(0.0);
                            }
                        }
                    }
                    let res = self.run_planar(&planar, ew, eh);
                    let (rw, rh) = (ew * SCALE, eh * SCALE);
                    for y in (y0 * SCALE)..(y1 * SCALE) {
                        for x in (x0 * SCALE)..(x1 * SCALE) {
                            let (lx, ly) = (x - ex0 * SCALE, y - ey0 * SCALE);
                            let di = (y * ow + x) * ch;
                            if pass == 0 {
                                for c in 0..3 {
                                    if let (Some(d), Some(s)) = (out.get_mut(di + c), res.get((c * rh + ly) * rw + lx)) {
                                        *d = *s;
                                    }
                                }
                            } else {
                                let mean = (0..3).filter_map(|c| res.get((c * rh + ly) * rw + lx)).sum::<f32>() / 3.0;
                                if let Some(d) = out.get_mut(di + 3) {
                                    *d = mean.clamp(0.0, 1.0);
                                }
                            }
                        }
                    }
                    done += 1;
                    ir.progress(done as f32 / total as f32);
                }
            }
        }
        if has_alpha && !alpha_model {
            // Bilinear alpha (pixel centres aligned), extra channels beyond alpha the same way.
            for y in 0..oh {
                let sy = ((y as f32 + 0.5) / SCALE as f32 - 0.5).clamp(0.0, (h - 1) as f32);
                let (y0, fy) = (sy.floor() as usize, sy - sy.floor());
                let y1 = (y0 + 1).min(h - 1);
                for x in 0..ow {
                    let sx = ((x as f32 + 0.5) / SCALE as f32 - 0.5).clamp(0.0, (w - 1) as f32);
                    let (x0, fx) = (sx.floor() as usize, sx - sx.floor());
                    let x1 = (x0 + 1).min(w - 1);
                    for c in 3..ch {
                        let g = |xx: usize, yy: usize| px.get((yy * w + xx) * ch + c).copied().unwrap_or(0.0);
                        let v = (g(x0, y0) * (1.0 - fx) + g(x1, y0) * fx) * (1.0 - fy) + (g(x0, y1) * (1.0 - fx) + g(x1, y1) * fx) * fy;
                        if let Some(d) = out.get_mut((y * ow + x) * ch + c) {
                            *d = v;
                        }
                    }
                }
            }
        } else if ch > 4 {
            for y in 0..oh {
                for x in 0..ow {
                    for c in 4..ch {
                        let v = px.get(((y / SCALE) * w + x / SCALE) * ch + c).copied().unwrap_or(0.0);
                        if let Some(d) = out.get_mut((y * ow + x) * ch + c) {
                            *d = v;
                        }
                    }
                }
            }
        }
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn load(name: &str) -> Vec<f32> {
        let p = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("models/test").join(name);
        std::fs::read(p).unwrap().chunks_exact(4).map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect()
    }

    #[test]
    fn bundled_model_parses() {
        let m = SrModel::bundled().unwrap();
        assert_eq!(m.layers.len(), 67);
    }

    #[test]
    fn matches_the_reference_implementation() {
        // 32 × 24 RGB crop and its ×4 result computed by a NumPy port of SRVGGNetCompact.
        let input = load("ref_in.f32");
        let want = load("ref_out.f32");
        let got = SrModel::bundled().unwrap().run_planar(&input, 32, 24);
        assert_eq!(got.len(), want.len());
        let max = got.iter().zip(&want).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(max < 2e-4, "max difference {max}");
    }

    #[test]
    fn tiled_upscale_matches_one_tile_in_the_middle() {
        let m = SrModel::bundled().unwrap();
        let (w, h) = (40, 30);
        let px: Vec<f32> = (0..w * h).flat_map(|i| {
            let (x, y) = ((i % w) as f32, (i / w) as f32);
            [x / w as f32, y / h as f32, ((x + y) % 7.0) / 7.0, if x > 20.0 { 1.0 } else { 0.4 }]
        }).collect();
        let never = || false;
        let quiet = |_: f32| {};
        let ir = Interrupt::new(&never, &quiet);
        let whole = m.upscale(&px, w, h, 4, true, 512, 0, &ir).unwrap();
        let tiled = m.upscale(&px, w, h, 4, true, 16, 40, &ir).unwrap();
        assert_eq!(whole.len(), w * h * 16 * 4);
        // With context covering the whole image, tiles give the same result.
        let max = whole.iter().zip(&tiled).map(|(a, b)| (a - b).abs()).fold(0.0f32, f32::max);
        assert!(max < 1e-4, "{max}");
        let bilinear_alpha = m.upscale(&px, w, h, 4, false, 64, 8, &ir).unwrap();
        assert!(bilinear_alpha.chunks_exact(4).all(|p| (0.0..=1.0).contains(&p[3])));
    }

    #[test]
    fn cancel_and_bad_input() {
        let m = SrModel::bundled().unwrap();
        let yes = || true;
        let quiet = |_: f32| {};
        let ir = Interrupt::new(&yes, &quiet);
        assert!(m.upscale(&[0.5; 64 * 4], 8, 8, 4, false, 16, 4, &ir).is_none());
        let no = || false;
        let ir = Interrupt::new(&no, &quiet);
        assert_eq!(m.upscale(&[0.5; 3], 8, 8, 4, false, 16, 4, &ir), Some(Vec::new()));
        assert!(SrModel::parse(b"nope").is_err());
        assert!(SrModel::parse(b"DTFSR1\0\0\x01\0\0\0\x07").is_err());
    }
}
