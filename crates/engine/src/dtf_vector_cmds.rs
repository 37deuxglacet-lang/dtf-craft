// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Vectorization (DTF Craft): the visible image traced into an SVG of cut-out shapes, one
//! flat colour each, at the document's print size ([`photocraft_algo::vectorize`]). The document
//! itself is never changed.
//!
//! * `dtf.vectorize`: the SVG (written to `path`, or returned), and optionally opened in a new
//!   tab as shape layers, at the document's size and resolution. The app opens the full-screen
//!   Vectorization module instead when run without parameters.
//!
//! RGB values are read as sRGB; other colour modes are converted to RGB first.

use photocraft_algo::vectorize::{self, Detail, Kind, Quantized, Settings, Smoothing, Style, SvgSize, TraceOptions, Traced};
use photocraft_doc::Document;
use serde_json::{Value, json};

use crate::commands::CommandSpec;
use crate::{EngineError, Result, Session};

pub const VECTORIZE: &str = "dtf.vectorize";
/// Suffix of the result's name.
pub const SUFFIX: &str = "vectorisé";

fn has_doc(s: &Session) -> std::result::Result<(), String> {
    s.active().map(|_| ()).ok_or_else(|| "no document open".into())
}

fn bad(msg: impl Into<String>) -> EngineError {
    EngineError::BadParams { cmd: VECTORIZE.into(), msg: msg.into() }
}

/// A text parameter: `None` when absent or null, an error when it is not a string.
fn text<'a>(p: &'a Value, key: &str) -> Result<Option<&'a str>> {
    match p.get(key).filter(|v| !v.is_null()) {
        None => Ok(None),
        Some(v) => v.as_str().map(Some).ok_or_else(|| bad(format!("`{key}` must be a string"))),
    }
}

/// The document's resolution, or 72 ppi when it has none.
pub fn dpi(doc: &Document) -> f64 {
    let r = f64::from(doc.resolution_dpi);
    if r.is_finite() && r > 0.0 { r } else { 72.0 }
}

/// The document's print size in mm: its pixels at its resolution.
pub fn size_mm(doc: &Document) -> (f64, f64) {
    let k = 25.4 / dpi(doc);
    (f64::from(doc.size.width) * k, f64::from(doc.size.height) * k)
}

/// The palette settings: `colors`, `kind`, `detail`.
pub fn settings_param(p: &Value) -> Result<Settings> {
    let colors = match p.get("colors").filter(|v| !v.is_null()) {
        None => 8,
        Some(v) => v.as_f64().filter(|n| n.is_finite() && (2.0..=vectorize::MAX_COLORS as f64).contains(n)).ok_or_else(|| bad("`colors` must be 2–32"))? as usize,
    };
    let kind = match text(p, "kind")? {
        None | Some("logo") => Kind::Logo,
        Some("illustration") => Kind::Illustration,
        Some(k) => return Err(bad(format!("`kind` must be logo or illustration (got {k})"))),
    };
    Ok(Settings { colors, kind, detail: detail_param(p)? })
}

fn detail_param(p: &Value) -> Result<Detail> {
    Ok(match text(p, "detail")? {
        None | Some("medium") => Detail::Medium,
        Some("low") => Detail::Low,
        Some("high") => Detail::High,
        Some(d) => return Err(bad(format!("`detail` must be low, medium or high (got {d})"))),
    })
}

/// The tracing options for the palette of `q`: `smoothing`, `removeBackground`, `exclude`.
pub fn trace_param(p: &Value, q: &Quantized) -> Result<TraceOptions> {
    let smoothing = match text(p, "smoothing")? {
        None | Some("curves") => Smoothing::Curves,
        Some("corners") => Smoothing::Corners,
        Some(v) => return Err(bad(format!("`smoothing` must be curves or corners (got {v})"))),
    };
    let remove_background = match p.get("removeBackground").filter(|v| !v.is_null()) {
        None => true,
        Some(v) => v.as_bool().ok_or_else(|| bad("`removeBackground` must be true or false"))?,
    };
    let excluded: Vec<String> = match p.get("exclude").filter(|v| !v.is_null()) {
        None => Vec::new(),
        Some(v) => v
            .as_array()
            .ok_or_else(|| bad("`exclude` must be a list of #rrggbb colours"))?
            .iter()
            .map(|h| h.as_str().map(str::to_ascii_uppercase).ok_or_else(|| bad(format!("`exclude`: bad colour {h}"))))
            .collect::<Result<_>>()?,
    };
    let excluded = q.palette.iter().map(|c| excluded.contains(&vectorize::hex(c.rgb))).collect();
    Ok(TraceOptions { detail: detail_param(p)?, smoothing, remove_background, excluded })
}

/// The visible image as straight RGBA: width, height, pixels.
pub fn visual_pixels(doc: &Document) -> Result<(u32, u32, Vec<[f32; 4]>)> {
    crate::dtf_color_cmds::visual_pixels(doc)
}

/// The palette as JSON: hex, RGB, share (%), background, in the SVG.
pub fn palette_json(q: &Quantized, o: &TraceOptions, t: &Traced) -> Vec<Value> {
    q.palette
        .iter()
        .enumerate()
        .map(|(i, c)| {
            json!({
                "hex": vectorize::hex(c.rgb),
                "rgb": c.rgb,
                "share": (c.share * 1000.0).round() / 10.0,
                "background": q.background == Some(i),
                "excluded": o.excluded.get(i).copied().unwrap_or(false),
                "inSvg": t.paths.iter().any(|p| p.color == i),
            })
        })
        .collect()
}

/// "logo" for "logo.png".
fn stem(name: &str) -> &str {
    match std::path::Path::new(name).extension().and_then(|e| e.to_str()) {
        Some(ext) if photocraft_codecs::from_extension(&ext.to_ascii_lowercase()).is_some() || ext.eq_ignore_ascii_case("psd") => {
            std::path::Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name)
        }
        _ => name,
    }
}

/// The result's name and its file name suggestion (an SVG beside the original when it has a path).
pub fn result_name(doc_name: &str, path: Option<&str>) -> (String, String) {
    let name = format!("{} {SUFFIX}", stem(doc_name));
    let file = format!("{name}.svg");
    let hint = path.map_or_else(|| file.clone(), |p| std::path::Path::new(p).with_file_name(&file).to_string_lossy().into_owned());
    (name, hint)
}

/// `sealSeams` (default true): each shape overlaps its neighbours by half a pixel.
pub fn seal_param(p: &Value) -> Result<bool> {
    match p.get("sealSeams").filter(|v| !v.is_null()) {
        None => Ok(true),
        Some(v) => v.as_bool().ok_or_else(|| bad("`sealSeams` must be true or false")),
    }
}

/// The palette with the swatch colours that replace some of its colours, and their references:
/// `"replace": [{"from": "#rrggbb", "to": "#rrggbb", "name": "EX-201 Bleu roi"}]`.
pub fn replace_param(p: &Value, q: &Quantized) -> Result<(Vec<vectorize::PaletteColor>, Vec<Option<String>>)> {
    let mut palette = q.palette.clone();
    let mut names = vec![None; palette.len()];
    let Some(list) = p.get("replace").filter(|v| !v.is_null()) else { return Ok((palette, names)) };
    let list = list.as_array().ok_or_else(|| bad("`replace` must be a list of {from, to, name}"))?;
    let rgb = |v: Option<&Value>, what: &str| -> Result<[u8; 3]> {
        let h = v.and_then(Value::as_str).ok_or_else(|| bad(format!("`replace`: `{what}` must be #rrggbb")))?;
        let x = h.strip_prefix('#').filter(|x| x.len() == 6 && x.is_ascii()).ok_or_else(|| bad(format!("`replace`: bad colour {h}")))?;
        let b = |i: usize| u8::from_str_radix(&x[i..i + 2], 16).map_err(|_| bad(format!("`replace`: bad colour {h}")));
        Ok([b(0)?, b(2)?, b(4)?])
    };
    for r in list {
        let from = rgb(r.get("from"), "from")?;
        let to = rgb(r.get("to"), "to")?;
        // A colour of an earlier palette means nothing now.
        if let Some(i) = palette.iter().position(|c| c.rgb == from) {
            palette[i].rgb = to;
            names[i] = r.get("name").and_then(Value::as_str).map(str::trim).filter(|n| !n.is_empty()).map(str::to_string);
        }
    }
    Ok((palette, names))
}

/// Opens `t` in a new tab: shape layers at the document's pixel size and resolution.
pub fn open_traced(s: &mut Session, t: &Traced, q: &Quantized, name: &str, size_px: (u32, u32), dpi: f64, seal: bool) -> Result<usize> {
    open_traced_as(s, t, &q.palette, &[], name, size_px, dpi, seal)
}

/// [`open_traced`] with the colours and names given.
#[allow(clippy::too_many_arguments)]
pub fn open_traced_as(s: &mut Session, t: &Traced, palette: &[vectorize::PaletteColor], names: &[Option<String>], name: &str, size_px: (u32, u32), dpi: f64, seal: bool) -> Result<usize> {
    let svg = vectorize::svg_named(t, palette, names, SvgSize::Px(f64::from(size_px.0), f64::from(size_px.1)), Style::Fill { seal });
    let r = photocraft_io::import(&format!("{name}.svg"), svg.as_bytes()).map_err(|e| EngineError::Other(format!("{name}: {e}")))?;
    let mut doc = r.document;
    doc.name = name.to_string();
    doc.resolution_dpi = dpi as f32;
    Ok(s.add_document(doc, None))
}

fn run(s: &mut Session, p: &Value) -> Result<Value> {
    let settings = settings_param(p)?;
    let path = text(p, "path")?.filter(|v| !v.trim().is_empty()).map(str::to_string);
    let open = match p.get("open").filter(|v| !v.is_null()) {
        None => false,
        Some(v) => v.as_bool().ok_or_else(|| bad("`open` must be true or false"))?,
    };
    let st = s.active().ok_or(EngineError::NoDocument)?;
    let (name, _) = result_name(&st.doc.name, st.path.as_deref());
    let (mm, dpi, size) = (size_mm(&st.doc), dpi(&st.doc), (st.doc.size.width, st.doc.size.height));
    let (w, h, px) = visual_pixels(&st.doc)?;
    let q = vectorize::quantize(w as usize, h as usize, &px, &settings).map_err(bad)?;
    let o = trace_param(p, &q)?;
    let seal = seal_param(p)?;
    let t = q.trace(&o).map_err(EngineError::Other)?;
    let (palette, names) = replace_param(p, &q)?;
    let svg = vectorize::svg_named(&t, &palette, &names, SvgSize::Mm(mm.0, mm.1), Style::Fill { seal });
    let mut out = json!({
        "width": t.width,
        "height": t.height,
        "widthMm": (mm.0 * 100.0).round() / 100.0,
        "heightMm": (mm.1 * 100.0).round() / 100.0,
        "palette": palette_json(&q, &o, &t),
        "colors": t.paths.len(),
        "shapes": t.shapes(),
        "points": t.points(),
    });
    match &path {
        Some(path) => {
            let path = std::path::Path::new(path).with_extension("svg").to_string_lossy().into_owned();
            crate::file_cmds::write_file(&path, svg.as_bytes())?;
            out["path"] = json!(path);
        }
        None => out["svg"] = json!(svg),
    }
    if open {
        out["document"] = json!(open_traced_as(s, &t, &palette, &names, &name, size, dpi, seal)?);
        out["name"] = json!(name);
    }
    Ok(out)
}

pub fn specs() -> Vec<CommandSpec> {
    vec![CommandSpec {
        id: VECTORIZE,
        label: "Vectorization…",
        menu: &["DTF"],
        shortcut: None,
        params: r##"{"colors":2..32=8,"kind":logo|illustration=logo (logo: colours read in the flat areas, close ones grouped),"detail":low|medium|high=medium (smallest area kept, closeness of the curves),"smoothing":curves|corners=curves,"removeBackground":bool=true (the colour on the visual's edge, where it touches the edge; the same colour inside stays),"exclude":json (optional #rrggbb palette colours left out of the SVG),"replace":json (optional palette colours given a swatch colour: [{from: #rrggbb, to: #rrggbb, name: reference}]; their shapes take that colour and are named after the reference),"sealSeams":bool=true (each shape overlaps its neighbours by half a pixel, so no transparent hairline shows between two colours; false: strictly cut out),"path":str (optional; the SVG is written there, else returned as "svg"),"open":bool=false (open the result in a new tab, as shape layers at the document's size and resolution)} → {width,height (px traced),widthMm,heightMm,palette:[{hex,rgb,share (%),background,excluded,inSvg}],colors,shapes,points,svg|path,document?,name?}. Cut-out shapes, one flat colour each, no shape over another; the SVG has the document's print size. The document is not changed. The app opens the full-screen Vectorization module"##,
        enabled: has_doc,
        run,
        journal: true,
    }]
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 100 × 60 at 254 ppi on white: a blue square and an orange square, a white dot inside the
    /// blue one.
    fn session() -> Session {
        let mut s = Session::new();
        s.execute("file.new", json!({"width": 100, "height": 60, "resolution": 254, "background": "white", "name": "logo.png"})).unwrap();
        s.edit("Paint", |doc, active| {
            let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
            for y in 10..50 {
                for x in 10..90 {
                    let inner = (25..35).contains(&x) && (25..35).contains(&y);
                    let c = if inner {
                        [255.0, 255.0, 255.0]
                    } else if x < 50 {
                        [31.0, 79.0, 181.0]
                    } else {
                        [228.0, 87.0, 46.0]
                    };
                    surf.write_pixel(x, y, &[c[0] / 255.0, c[1] / 255.0, c[2] / 255.0, 1.0]);
                }
            }
            Ok(())
        })
        .unwrap();
        s
    }

    #[test]
    fn the_svg_has_the_print_size_and_the_cut_out_colours() {
        let mut s = session();
        let revision = s.active().unwrap().revision;
        let r = s.execute(VECTORIZE, json!({})).unwrap();
        assert_eq!((r["widthMm"].as_f64(), r["heightMm"].as_f64()), (Some(10.0), Some(6.0)));
        let pal = r["palette"].as_array().unwrap();
        assert_eq!(pal.len(), 3);
        assert_eq!((pal[0]["hex"].as_str(), pal[0]["background"].as_bool()), (Some("#FFFFFF"), Some(true)));
        // The white dot stays, the white around goes.
        assert_eq!(r["colors"], 3);
        assert_eq!(r["shapes"], 3);
        let svg = r["svg"].as_str().unwrap();
        assert!(svg.contains("width=\"10mm\" height=\"6mm\" viewBox=\"0 0 100 60\""), "{svg}");
        assert!(svg.contains("stroke-width=\"1\""), "seams closed by default");
        let cut = s.execute(VECTORIZE, json!({"sealSeams": false})).unwrap();
        assert!(!cut["svg"].as_str().unwrap().contains("stroke"));
        assert_eq!(s.active().unwrap().revision, revision, "the document is left alone");
        assert_eq!(s.documents().len(), 1);
        // White left out: two colours.
        let r = s.execute(VECTORIZE, json!({"exclude": ["#ffffff"], "smoothing": "corners", "detail": "high"})).unwrap();
        assert_eq!((r["colors"].as_u64(), r["palette"][0]["excluded"].as_bool()), (Some(2), Some(true)));
        // The background kept: white is one area more.
        let r = s.execute(VECTORIZE, json!({"removeBackground": false})).unwrap();
        assert_eq!(r["shapes"], 4);
    }

    #[test]
    fn opening_adds_a_tab_of_shape_layers_at_the_document_size() {
        let mut s = session();
        let r = s.execute(VECTORIZE, json!({"open": true})).unwrap();
        assert_eq!((r["document"].as_u64(), r["name"].as_str()), (Some(1), Some("logo vectorisé")));
        let st = s.active().unwrap();
        assert_eq!((st.doc.size.width, st.doc.size.height, st.doc.resolution_dpi), (100, 60, 254.0));
        assert_eq!(st.doc.layers.len(), 3);
        assert!(st.doc.layers.iter().all(|l| matches!(l.content, photocraft_doc::LayerContent::Shape(_))), "shape layers");
        assert!(st.doc.layers.iter().any(|l| l.name == "couleur-1F4FB5"));
        // The original is still there, unchanged.
        assert_eq!(s.documents()[0].doc.name, "logo.png");
    }

    #[test]
    fn swatch_colours_replace_palette_colours_and_name_the_shapes() {
        let mut s = session();
        let replace = json!([
            {"from": "#1F4FB5", "to": "#0033A0", "name": "PANTONE 286 C"},
            {"from": "#123456", "to": "#000000", "name": "absente"},
        ]);
        let r = s.execute(VECTORIZE, json!({"replace": replace})).unwrap();
        let svg = r["svg"].as_str().unwrap();
        assert!(svg.contains("id=\"PANTONE_286_C\"") && svg.contains("#0033A0"), "{svg}");
        assert!(!svg.contains("#1F4FB5"), "the swatch colour replaces it");
        assert!(svg.contains("couleur-E4572E"), "the others keep their hex");
        let r = s.execute(VECTORIZE, json!({"replace": replace, "open": true})).unwrap();
        assert!(r["document"].is_u64());
        assert!(s.active().unwrap().doc.layers.iter().any(|l| l.name == "PANTONE_286_C"));
        for p in [json!({"replace": "#000000"}), json!({"replace": [{"from": "#1F4FB5"}]}), json!({"replace": [{"from": "blue", "to": "#000000"}]})] {
            assert!(s.execute(VECTORIZE, p.clone()).is_err(), "{p}");
        }
    }

    #[test]
    fn the_svg_is_written_where_asked() {
        let mut s = session();
        let dir = std::env::temp_dir().join(format!("dtf-vector-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("logo vectorisé.png");
        let r = s.execute(VECTORIZE, json!({"path": path.to_string_lossy()})).unwrap();
        let written = r["path"].as_str().unwrap();
        assert!(written.ends_with("logo vectorisé.svg") && r.get("svg").is_none());
        assert!(std::fs::read_to_string(written).unwrap().contains("<svg "));
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(result_name("logo.png", Some("/w/logo.png")).1, "/w/logo vectorisé.svg");
    }

    #[test]
    fn bad_params_are_errors() {
        let mut s = session();
        for p in [
            json!({"colors": 1}),
            json!({"colors": "eight"}),
            json!({"kind": "photo"}),
            json!({"detail": 3}),
            json!({"detail": "max"}),
            json!({"smoothing": "round"}),
            json!({"exclude": "#ffffff"}),
            json!({"exclude": [3]}),
            json!({"kind": 2}),
            json!({"smoothing": false}),
            json!({"removeBackground": "yes"}),
            json!({"open": 1}),
            json!({"path": 5}),
            json!({"sealSeams": "no"}),
            json!({"colors": 1e300}),
            json!({"colors": -4}),
        ] {
            assert!(s.execute(VECTORIZE, p.clone()).is_err(), "{p}");
        }
        // Nothing visible.
        s.execute("file.new", json!({"width": 10, "height": 10, "background": "transparent"})).unwrap();
        assert!(s.execute(VECTORIZE, json!({})).is_err());
        assert!(Session::new().execute(VECTORIZE, json!({})).is_err());
    }
}
