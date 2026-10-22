// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Color Analyzer (DTF Craft): the visual's main colours and their nearest swatch-book
//! colours ([`photocraft_algo::dtf_color`]), and a copy of the visual with its colours replaced by
//! the swatch colours, for a PNG export. The document itself is never changed.
//!
//! * `dtf.colorAnalyzer`: the analysis (the app opens the full-screen module instead).
//! * `dtf.colorReplace`: a new document "<name> nuancier", the merged visual with the colours
//!   replaced (same size, resolution and transparency), whose Save As suggests a PNG.
//!
//! RGB values are read as sRGB; other colour modes are converted to RGB first.

use photocraft_algo::dtf_color::{self, MainColor, Swatch};
use photocraft_doc::{Document, Layer, LayerContent};
use photocraft_raster::Surface;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

pub const ANALYZER: &str = "dtf.colorAnalyzer";
pub const REPLACE: &str = "dtf.colorReplace";
/// Suffix of the replaced copy's name.
pub const REPLACED_SUFFIX: &str = "nuancier";
/// At most this many pixels are read for the analysis (a regular sample of larger visuals).
const ANALYSIS_PIXELS: usize = 4_000_000;

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

fn bad(cmd: &str, msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: cmd.into(), msg: msg.into() }
}

/// `#rrggbb` → 0–1 sRGB.
pub fn hex_rgb(hex: &str) -> Option<[f32; 3]> {
    let h = hex.strip_prefix('#')?;
    if h.len() != 6 || !h.is_ascii() {
        return None;
    }
    let v = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok().map(|b| f32::from(b) / 255.0);
    Some([v(0)?, v(2)?, v(4)?])
}

/// The document as RGB (converted when it is in another mode).
fn rgb_document(doc: &Document) -> Result<Document> {
    if doc.mode == photocraft_color::ColorMode::Rgb {
        return Ok(doc.clone());
    }
    let mut t = Session::new();
    t.add_document(doc.clone(), None);
    t.execute("image.mode.rgb", json!({}))?;
    t.active().map(|st| (*st.doc).clone()).ok_or(EngineError::NoDocument)
}

/// The visible image (all layers merged) as straight RGBA, row by row: width, height, pixels.
pub fn visual_pixels(doc: &Document) -> Result<(u32, u32, Vec<[f32; 4]>)> {
    let doc = rgb_document(doc)?;
    let buf = photocraft_compose::flatten(&doc);
    Ok((doc.size.width, doc.size.height, buf.px))
}

/// A regular sample of at most [`ANALYSIS_PIXELS`] pixels.
pub fn sample(px: &[[f32; 4]]) -> Vec<[f32; 4]> {
    let step = px.len().div_ceil(ANALYSIS_PIXELS).max(1);
    px.iter().step_by(step).copied().collect()
}

/// The swatch-book colours given as `[{"name", "hex"} | {"name", "lab": [L, a, b]}]`.
pub fn swatches_param(cmd: &str, p: &Value) -> Result<Vec<Swatch>> {
    let Some(list) = p.get("swatches").filter(|v| !v.is_null()) else { return Ok(Vec::new()) };
    let list = list.as_array().ok_or_else(|| bad(cmd, "`swatches` must be a list of {name, hex | lab}"))?;
    list.iter()
        .enumerate()
        .map(|(i, v)| {
            let name = v.get("name").and_then(Value::as_str).map_or_else(|| format!("{}", i + 1), str::to_string);
            let lab = if let Some(h) = v.get("hex").and_then(Value::as_str) {
                photocraft_color::convert::srgb_to_lab(hex_rgb(h).ok_or_else(|| bad(cmd, format!("swatch {name}: bad colour {h}")))?)
            } else {
                let l: Vec<f32> =
                    v.get("lab").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_f64).map(|x| x as f32).collect()).unwrap_or_default();
                if l.len() != 3 {
                    return Err(bad(cmd, format!("swatch {name}: give hex or lab")));
                }
                [l[0], l[1], l[2]]
            };
            Ok(Swatch { name, lab })
        })
        .collect()
}

/// The analysis settings: (max colours, grouping ΔE, minimum share 0–1).
pub fn analysis_settings(p: &Value) -> (usize, f32, f32) {
    let n = |k: &str, d: f64| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(d);
    (n("maxColors", 8.0).clamp(1.0, 32.0) as usize, n("mergeDeltaE", 5.0).clamp(0.0, 30.0) as f32, (n("minShare", 0.5).clamp(0.0, 20.0) / 100.0) as f32)
}

/// The visual's colours: the main ones, then those added by hand (`extra`, sRGB), with their
/// shares over the visible pixels sampled.
pub fn colors(px: &[[f32; 4]], max_colors: usize, merge_de: f32, min_share: f32, extra: &[[f32; 3]]) -> Vec<MainColor> {
    let mut colors = dtf_color::analyze(px, max_colors, merge_de, min_share);
    if extra.is_empty() {
        return colors;
    }
    for rgb in extra {
        let rgb = rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() / 255.0);
        colors.push(MainColor { rgb, lab: photocraft_color::convert::srgb_to_lab(rgb), share: 0.0 });
    }
    let centers: Vec<[f32; 3]> = colors.iter().map(|c| c.lab).collect();
    for (c, share) in colors.iter_mut().zip(dtf_color::shares(px, &centers)) {
        c.share = share;
    }
    colors
}

fn rgb255(rgb: [f32; 3]) -> [u8; 3] {
    rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

/// One colour as JSON, with its nearest swatch when there is a swatch book.
pub fn color_json(c: &MainColor, swatches: &[Swatch]) -> Value {
    let matched = dtf_color::nearest_swatch(swatches, c.lab).map(|(i, de)| {
        let s = &swatches[i];
        json!({"name": s.name, "hex": dtf_color::hex(dtf_color::swatch_rgb(s)), "deltaE": (de * 10.0).round() / 10.0})
    });
    json!({
        "hex": dtf_color::hex(c.rgb),
        "rgb": rgb255(c.rgb),
        "lab": c.lab.map(|v| (v * 100.0).round() / 100.0),
        "share": (c.share * 1000.0).round() / 10.0,
        "match": matched,
    })
}

fn analyzer(s: &mut Session, p: &Value) -> Result<Value> {
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let swatches = swatches_param(ANALYZER, p)?;
    let extra: Vec<[f32; 3]> = match p.get("extra").filter(|v| !v.is_null()) {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .ok_or_else(|| bad(ANALYZER, "`extra` must be a list of #rrggbb colours"))?
            .iter()
            .map(|h| h.as_str().and_then(hex_rgb).ok_or_else(|| bad(ANALYZER, format!("`extra`: bad colour {h}"))))
            .collect::<Result<_>>()?,
    };
    let (max, merge, min) = analysis_settings(p);
    let (w, h, px) = visual_pixels(&st.doc)?;
    let found = colors(&sample(&px), max, merge, min, &extra);
    Ok(json!({
        "width": w,
        "height": h,
        "resolution": st.doc.resolution_dpi,
        "colors": found.iter().map(|c| color_json(c, &swatches)).collect::<Vec<_>>(),
    }))
}

/// The centres and targets of `colors`: `[{"from": "#rrggbb", "to": "#rrggbb" | null}]`.
pub fn replacements_param(cmd: &str, p: &Value) -> Result<Vec<([f32; 3], Option<[f32; 3]>)>> {
    let list = p.get("colors").and_then(Value::as_array).ok_or_else(|| bad(cmd, "`colors` must be a list of {from, to}"))?;
    let lab = |v: Option<&Value>, what: &str| -> Result<Option<[f32; 3]>> {
        match v {
            None | Some(Value::Null) => Ok(None),
            Some(Value::String(h)) => {
                hex_rgb(h).map(|rgb| Some(photocraft_color::convert::srgb_to_lab(rgb))).ok_or_else(|| bad(cmd, format!("`{what}`: bad colour {h}")))
            }
            Some(v) => Err(bad(cmd, format!("`{what}` must be #rrggbb (got {v})"))),
        }
    };
    list.iter().map(|c| Ok((lab(c.get("from"), "from")?.ok_or_else(|| bad(cmd, "each colour needs `from`"))?, lab(c.get("to"), "to")?))).collect()
}

/// "logo" for "logo.png" (a known image extension is dropped).
fn stem(name: &str) -> &str {
    match std::path::Path::new(name).extension().and_then(|e| e.to_str()) {
        Some(ext) if photocraft_codecs::from_extension(&ext.to_ascii_lowercase()).is_some() || ext.eq_ignore_ascii_case("psd") => {
            std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name)
        }
        _ => name,
    }
}

/// The replaced copy's name and its Save As suggestion (a PNG beside the original when it has a
/// path).
pub fn replaced_name(doc_name: &str, path: Option<&str>) -> (String, String) {
    let name = format!("{} {REPLACED_SUFFIX}", stem(doc_name));
    let file = format!("{name}.png");
    let hint = path.map_or_else(|| file.clone(), |p| std::path::Path::new(p).with_file_name(&file).to_string_lossy().into_owned());
    (name, hint)
}

fn replace(s: &mut Session, p: &Value) -> Result<Value> {
    let centers = replacements_param(REPLACE, p)?;
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let (name, hint) = replaced_name(&st.doc.name, st.path.as_deref());
    let name = p.get("name").and_then(Value::as_str).filter(|n| !n.trim().is_empty()).map_or(name, str::to_string);
    let mut doc = rgb_document(&st.doc)?;
    let mut px = photocraft_compose::flatten(&doc).px;
    dtf_color::replace(&mut px, &centers);
    let fmt = doc.pixel_format();
    let mut surf = Surface::new(fmt);
    let vals: Vec<f32> = px.iter().flat_map(|q| photocraft_raster::from_rgba(&fmt, *q)).collect();
    surf.write_region(doc.bounds(), &vals);
    surf.prune();
    doc.id = photocraft_doc::DocId::fresh();
    doc.name = name.clone();
    doc.layers = vec![Layer::new(&name, LayerContent::Raster(surf))];
    doc.layer_comps.clear();
    doc.last_applied_comp = None;
    doc.last_document_state = None;
    let (w, h) = (doc.size.width, doc.size.height);
    let index = s.add_document(doc, None);
    if let Some(copy) = s.active_mut() {
        copy.save_hint = Some(hint.clone());
    }
    Ok(json!({"document": index, "name": name, "width": w, "height": h, "saveAs": hint}))
}

pub fn specs() -> Vec<CommandSpec> {
    vec![
        CommandSpec {
            id: ANALYZER,
            label: "Color Analyzer…",
            menu: &["DTF"],
            shortcut: None,
            params: r##"{"maxColors":1..32=8,"mergeDeltaE":0..30=5 (colours closer than this ΔE 2000 are grouped),"minShare":0..20=0.5 (% of the visible pixels; smaller colours are left out),"extra":json (optional #rrggbb colours added by hand),"swatches":json (optional swatch book: [{name, hex | lab}])} → {width,height,resolution,colors:[{hex,rgb,lab,share (%),match:{name,hex,deltaE}|null}]}. Visible pixels only (transparency ignored), read as sRGB; the document is not changed. The app opens the full-screen Color Analyzer"##,
            enabled: has_doc,
            run: analyzer,
            journal: false,
        },
        CommandSpec {
            id: REPLACE,
            label: "Replace Colors with Swatches",
            // Run from the Color Analyzer's PNG export.
            menu: &[],
            shortcut: None,
            params: r##"{"colors":json ([{from: "#rrggbb", to: "#rrggbb" | null}]: every main colour of the analysis, with the swatch colour it becomes or null to keep it),"name":str (optional; "<name> nuancier")} → {document,name,width,height,saveAs}. A new document: the merged visual, each pixel shifted by the colours near it, so flat areas take the swatch colour exactly and contours stay smooth; same size, resolution and transparency. The original is not changed"##,
            enabled: has_doc,
            run: replace,
            journal: true,
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 × 50: blue left half, orange right half, a transparent band at the top.
    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 100, "height": 50, "resolution": 300, "background": "transparent", "name": "logo.png"})).unwrap();
        s.edit("Paint", |doc, active| {
            let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
            for y in 10..50 {
                for x in 0..100 {
                    let c = if x < 50 { [31.0, 58.0, 147.0] } else { [228.0, 87.0, 46.0] };
                    surf.write_pixel(x, y, &[c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0]);
                }
            }
            Ok(())
        })
        .unwrap();
        s
    }

    #[test]
    fn the_analysis_reads_the_colours_and_matches_the_swatch_book() {
        let mut s = session();
        let revision = s.active().unwrap().revision;
        let r = s
            .execute(
                ANALYZER,
                json!({"swatches": [{"name": "EX-201 Bleu roi", "hex": "#1E3B96"}, {"name": "EX-114 Orange vif", "hex": "#E25E2B"}], "extra": ["#FFFFFF"]}),
            )
            .unwrap();
        let c = r["colors"].as_array().unwrap();
        assert_eq!(c.len(), 3, "{r}");
        assert_eq!((c[0]["hex"].as_str(), c[0]["share"].as_f64()), (Some("#1F3A93"), Some(50.0)));
        assert_eq!(c[0]["match"]["name"], "EX-201 Bleu roi");
        assert!(c[0]["match"]["deltaE"].as_f64().unwrap() < 2.0);
        assert_eq!(c[1]["match"]["name"], "EX-114 Orange vif");
        // The colour added by hand has no pixels here.
        assert_eq!((c[2]["hex"].as_str(), c[2]["share"].as_f64()), (Some("#FFFFFF"), Some(0.0)));
        assert_eq!(s.active().unwrap().revision, revision, "the analysis leaves the document alone");
        assert!(s.execute(ANALYZER, json!({"extra": ["blue"]})).is_err());
    }

    #[test]
    fn replacing_opens_a_copy_and_leaves_the_original() {
        let mut s = session();
        let r = s.execute(REPLACE, json!({"colors": [{"from": "#1F3A93", "to": "#003399"}, {"from": "#E4572E", "to": null}]})).unwrap();
        assert_eq!((r["name"].as_str(), r["saveAs"].as_str(), r["document"].as_u64()), (Some("logo nuancier"), Some("logo nuancier.png"), Some(1)));
        assert_eq!(s.documents().len(), 2);
        let copy = s.active().unwrap();
        assert_eq!((copy.doc.size.width, copy.doc.size.height, copy.doc.resolution_dpi), (100, 50, 300.0));
        assert_eq!(copy.save_hint.as_deref(), Some("logo nuancier.png"));
        let (_, _, px) = visual_pixels(&copy.doc).unwrap();
        let hex = |q: [f32; 4]| dtf_color::hex([q[0], q[1], q[2]]);
        assert_eq!(hex(px[30 * 100 + 10]), "#003399");
        assert_eq!(hex(px[30 * 100 + 90]), "#E4572E");
        assert_eq!(px[5 * 100 + 10][3], 0.0, "transparency stays");
        // The original is untouched.
        let (_, _, orig) = visual_pixels(&s.documents()[0].doc).unwrap();
        assert_eq!(hex(orig[30 * 100 + 10]), "#1F3A93");
        assert!(s.execute(REPLACE, json!({"colors": [{"to": "#000000"}]})).is_err());
    }

    #[test]
    fn the_copy_is_saved_beside_the_original() {
        assert_eq!(replaced_name("logo.png", Some("/work/logo.png")), ("logo nuancier".into(), "/work/logo nuancier.png".into()));
        assert_eq!(replaced_name("Sans titre-1", None).1, "Sans titre-1 nuancier.png");
    }
}
