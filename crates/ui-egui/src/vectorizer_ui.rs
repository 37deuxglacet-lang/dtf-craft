// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Vectorization: a full-screen module over the main window, like Camera Raw and the Color
//! Analyzer. The visible image is reduced to a palette (8 colours by default) and traced into cut-out
//! shapes, one flat colour each and no shape over another ([`photocraft_algo::vectorize`]); the
//! preview shows the original and the vectorized result side by side, and the export writes an SVG
//! at the document's print size (`dtf.vectorize`), optionally opened in a new tab as shape layers.
//! The document is never changed.
//!
//! The module works on the same copy the command traces (at most [`vectorize::MAX_SIDE`] px a
//! side), so the exported SVG is the one previewed.
//!
//! The preview zooms from 100 % (fitted) to 800 % (Ctrl+wheel about the pointer, the − / + buttons,
//! a double-click), and pans by dragging or with the wheel; side by side, both images follow. Once
//! the view rests, the visible part of the vectorized result is drawn again at the screen's
//! resolution, so its contours are judged sharp at any zoom.

use std::collections::{HashMap, HashSet};

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use photocraft_algo::vectorize::{self, Detail, Kind, Quantized, Smoothing, Style, SvgSize, TraceOptions, Traced};
use photocraft_engine::dtf_vector_cmds as cmds;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::color_analyzer_ui::{self as analyzer, Book, Choice, Pick};
use crate::color_sheet::{self, ColorSheet, SheetRef, SheetRow};
use crate::theme::Tokens;

const PANEL_W: f32 = 420.0;
const SETTINGS_PREF: &str = "dtfCraft.vectorizer";
/// The vectorized preview's longest side, in px.
const PREVIEW_SIDE: f32 = 1600.0;
/// The contours drawn by "Show Contours", in preview px.
const CONTOUR_PX: f32 = 1.2;
/// The preview's zoom steps (1 = fitted).
const ZOOMS: [f32; 8] = [1.0, 1.5, 2.0, 3.0, 4.0, 5.0, 6.0, 8.0];
const MAX_ZOOM: f32 = 8.0;
/// How long the view rests before its visible part is drawn sharp, in seconds.
const SHARP_DELAY: f64 = 0.2;
/// The colour sheet's visuals, longest side in px.
const SHEET_IMAGE_MAX: usize = 800;

/// What the preview shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum View {
    Original,
    Vectorized,
    SideBySide,
}

/// The module's settings, remembered with the preferences.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// At most this many colours (2–16 here; the command takes up to 32).
    pub colors: u32,
    pub kind: Kind,
    pub detail: Detail,
    pub smoothing: Smoothing,
    pub remove_background: bool,
    /// Each shape overlaps its neighbours by half a pixel (no transparent hairline at the seams).
    pub seal: bool,
    /// Open the exported SVG in a new tab, as shape layers.
    pub open_after: bool,
    pub contours: bool,
    pub checker: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            colors: 8,
            kind: Kind::Logo,
            detail: Detail::Medium,
            smoothing: Smoothing::Curves,
            remove_background: true,
            seal: true,
            open_after: true,
            contours: false,
            checker: true,
        }
    }
}

/// A texture and the revision it shows.
struct Tex {
    revision: u64,
    handle: egui::TextureHandle,
}

/// The visible part of the vectorized result drawn at the screen's resolution: what it shows
/// (revision, region of the traced copy in px, size in screen px) and its texture.
struct Sharp {
    revision: u64,
    region: [f32; 4],
    size: [usize; 2],
    handle: egui::TextureHandle,
}

pub struct Vectorizer {
    pub doc: photocraft_doc::DocId,
    pub name: String,
    pub path: Option<String>,
    /// The document's size in px, resolution and print size in mm.
    pub size: (u32, u32),
    pub dpi: f64,
    pub size_mm: (f64, f64),
    pub mode: String,
    /// The traced copy: width, height, straight RGBA.
    pub w: usize,
    pub h: usize,
    px: Vec<[f32; 4]>,
    pub view: View,
    pub settings: Settings,
    /// Palette colours left out of the SVG, by hex.
    pub excluded: HashSet<String>,
    pub quantized: Option<Quantized>,
    pub traced: Option<Traced>,
    /// Why the last analysis or tracing failed.
    pub error: Option<String>,
    /// The reference swatch book, shared with the Color Analyzer; each colour's choices (swatch
    /// picked, replaced or not) by hex.
    pub book: Option<Book>,
    choices: HashMap<String, Choice>,
    /// The palette colour whose reference is being changed, and the picker's search.
    changing: Option<usize>,
    search: String,
    revision: u64,
    original: Option<egui::TextureHandle>,
    vectorized: Option<Tex>,
    contours: Option<Tex>,
    /// The preview's zoom (1 = fitted, at most [`MAX_ZOOM`]) and the point of the visual at the
    /// centre of the view (fractions of its width and height).
    pub zoom: f32,
    pub center: [f32; 2],
    /// When the view last moved (seconds), and what it last showed (zoom, centre, frame).
    moved_at: f64,
    last_view: [f32; 6],
    sharp: Option<Sharp>,
    sharp_contours: Option<Sharp>,
    /// The palette settings changed: reduce the colours again (once the pointer is released).
    requantize: bool,
    /// The tracing settings changed: trace again.
    retrace: bool,
    help: bool,
}

/// What a settings change asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Redo {
    Nothing,
    Trace,
    Quantize,
}

fn to8(v: f32) -> u8 {
    (v.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn c32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

fn rgba8(px: &[[f32; 4]]) -> Vec<u8> {
    px.iter().flat_map(|q| [to8(q[0]), to8(q[1]), to8(q[2]), to8(q[3])]).collect()
}

/// `12,5` in French, `12.5` in English: one decimal, `150` rather than `150,0`.
fn decimal(v: f64) -> String {
    let s = crate::color_sheet::decimal(v, 1);
    match s.strip_suffix(",0").or_else(|| s.strip_suffix(".0")) {
        Some(whole) => whole.to_string(),
        None => s,
    }
}

impl Vectorizer {
    /// The module for the visual `px` (`size`, straight RGBA) of document `doc`.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        doc: photocraft_doc::DocId,
        name: &str,
        path: Option<String>,
        size: (u32, u32),
        dpi: f64,
        size_mm: (f64, f64),
        mode: String,
        px: &[[f32; 4]],
        settings: Settings,
        book: Option<Book>,
    ) -> Self {
        let (w, h, px) = vectorize::reduce(size.0 as usize, size.1 as usize, px, vectorize::MAX_SIDE);
        let mut v = Self {
            doc,
            name: name.to_string(),
            path,
            size,
            dpi,
            size_mm,
            mode,
            w,
            h,
            px,
            view: View::SideBySide,
            settings,
            excluded: HashSet::new(),
            quantized: None,
            traced: None,
            error: None,
            book,
            choices: HashMap::new(),
            changing: None,
            search: String::new(),
            revision: 0,
            original: None,
            vectorized: None,
            contours: None,
            zoom: 1.0,
            center: [0.5, 0.5],
            moved_at: 0.0,
            last_view: [0.0; 6],
            sharp: None,
            sharp_contours: None,
            requantize: false,
            retrace: false,
            help: false,
        };
        v.quantize();
        v
    }

    fn palette_settings(&self) -> vectorize::Settings {
        vectorize::Settings { colors: self.settings.colors as usize, kind: self.settings.kind, detail: self.settings.detail }
    }

    /// Reduces the colours again, then traces.
    pub fn quantize(&mut self) {
        self.requantize = false;
        match vectorize::quantize(self.w, self.h, &self.px, &self.palette_settings()) {
            Ok(q) => {
                self.quantized = Some(q);
                self.trace();
            }
            Err(e) => {
                self.quantized = None;
                self.traced = None;
                self.error = Some(e);
                self.revision += 1;
            }
        }
    }

    /// The tracing options of the current palette.
    pub fn trace_options(&self) -> TraceOptions {
        let excluded = self.quantized.as_ref().map_or_else(Vec::new, |q| q.palette.iter().map(|c| self.excluded.contains(&vectorize::hex(c.rgb))).collect());
        TraceOptions { detail: self.settings.detail, smoothing: self.settings.smoothing, remove_background: self.settings.remove_background, excluded }
    }

    /// Traces the shapes again (smoothing, background or colours left out changed).
    pub fn trace(&mut self) {
        self.retrace = false;
        let o = self.trace_options();
        let result = match self.quantized.as_ref() {
            Some(q) => q.trace(&o),
            None => Err("nothing to trace".into()),
        };
        match result {
            Ok(t) => {
                self.traced = Some(t);
                self.error = None;
            }
            Err(e) => {
                self.traced = None;
                self.error = Some(e);
            }
        }
        self.revision += 1;
    }

    /// Recomputes what a settings change made stale.
    fn redo(&mut self, r: Redo) {
        match r {
            Redo::Nothing => {}
            Redo::Trace => self.retrace = true,
            Redo::Quantize => self.requantize = true,
        }
    }

    /// Runs the pending analysis or tracing.
    pub fn update(&mut self) {
        if self.requantize {
            self.quantize();
        } else if self.retrace {
            self.trace();
        }
    }

    pub fn set_excluded(&mut self, hex: &str, excluded: bool) {
        if excluded {
            self.excluded.insert(hex.to_string());
        } else {
            self.excluded.remove(hex);
        }
        self.retrace = true;
    }

    /// The background colour found on the visual's edge, when there is one.
    pub fn background(&self) -> Option<[u8; 3]> {
        let q = self.quantized.as_ref()?;
        q.background.and_then(|i| q.palette.get(i)).map(|c| c.rgb)
    }

    /// Each palette colour's reference: the swatch (picked, or the nearest), its ΔE 2000 and
    /// whether the colour takes it in the SVG.
    pub fn matches(&self) -> Vec<Option<(usize, f32, bool)>> {
        let Some(q) = self.quantized.as_ref() else { return Vec::new() };
        q.palette
            .iter()
            .map(|c| {
                let choice = self.choices.get(&vectorize::hex(c.rgb)).copied().unwrap_or_default();
                let (swatch, replace) = choice.resolve(self.book.as_ref(), analyzer::lab8(c.rgb));
                swatch.map(|(k, de)| (k, de, replace))
            })
            .collect()
    }

    fn swatch(&self, k: usize) -> Option<&photocraft_algo::dtf_color::Swatch> {
        self.book.as_ref().and_then(|b| b.swatches.get(k))
    }

    /// The palette as the SVG draws it (the swatch colours replacing the colours checked) and each
    /// colour's name (its swatch reference when replaced).
    pub fn svg_palette(&self) -> (Vec<vectorize::PaletteColor>, Vec<Option<String>>) {
        let Some(q) = self.quantized.as_ref() else { return (Vec::new(), Vec::new()) };
        let mut palette = q.palette.clone();
        let mut names = vec![None; palette.len()];
        for (i, m) in self.matches().into_iter().enumerate() {
            if let Some((k, _, true)) = m
                && let Some(s) = self.swatch(k)
            {
                palette[i].rgb = analyzer::swatch8(s);
                names[i] = Some(s.name.clone());
            }
        }
        (palette, names)
    }

    pub fn set_swatch(&mut self, hex: &str, swatch: Option<usize>) {
        self.choices.entry(hex.to_string()).or_default().swatch = swatch;
        self.revision += 1;
    }

    pub fn set_replace(&mut self, hex: &str, on: bool) {
        self.choices.entry(hex.to_string()).or_default().replace = Some(on);
        self.revision += 1;
    }

    pub fn set_book(&mut self, book: Option<Book>) {
        self.book = book;
        // Picked references belong to the previous book.
        for c in self.choices.values_mut() {
            c.swatch = None;
        }
        self.changing = None;
        self.revision += 1;
    }

    /// The parameters `dtf.vectorize` takes to trace what the module shows.
    pub fn params(&self) -> Value {
        let s = &self.settings;
        let kind = match s.kind {
            Kind::Logo => "logo",
            Kind::Illustration => "illustration",
        };
        let detail = match s.detail {
            Detail::Low => "low",
            Detail::Medium => "medium",
            Detail::High => "high",
        };
        let smoothing = match s.smoothing {
            Smoothing::Curves => "curves",
            Smoothing::Corners => "corners",
        };
        // Only the palette's colours: a colour left out of an earlier palette means nothing now.
        let mut exclude: Vec<String> = self
            .quantized
            .as_ref()
            .map(|q| q.palette.iter().map(|c| vectorize::hex(c.rgb)).filter(|h| self.excluded.contains(h)).collect())
            .unwrap_or_default();
        exclude.sort();
        // The colours that take their swatch, and its reference.
        let (palette, names) = self.svg_palette();
        let replace: Vec<Value> = self
            .quantized
            .as_ref()
            .map(|q| {
                q.palette
                    .iter()
                    .zip(palette.iter().zip(&names))
                    .filter_map(|(c, (to, name))| name.as_ref().map(|n| json!({"from": vectorize::hex(c.rgb), "to": vectorize::hex(to.rgb), "name": n})))
                    .collect()
            })
            .unwrap_or_default();
        json!({"colors": s.colors, "kind": kind, "detail": detail, "smoothing": smoothing, "removeBackground": s.remove_background, "sealSeams": s.seal, "exclude": exclude, "replace": replace})
    }

    /// The SVG as it is exported (print size in mm).
    pub fn svg(&self) -> Option<String> {
        let t = self.traced.as_ref()?;
        let (palette, names) = self.svg_palette();
        Some(vectorize::svg_named(t, &palette, &names, SvgSize::Mm(self.size_mm.0, self.size_mm.1), Style::Fill { seal: self.settings.seal }))
    }

    /// The scale of the vectorized preview against the traced copy.
    fn preview_scale(&self) -> f32 {
        (PREVIEW_SIDE / self.w.max(self.h).max(1) as f32).min(4.0)
    }

    /// The traced shapes drawn at preview size: filled, or their contours only.
    fn render(&self, contours: bool) -> Option<egui::ColorImage> {
        let k = self.preview_scale();
        self.render_at(contours, k)
    }

    /// The traced shapes drawn `k` times the traced copy's size.
    fn render_at(&self, contours: bool, k: f32) -> Option<egui::ColorImage> {
        let t = self.traced.as_ref()?;
        let style = if contours { Style::Contours(f64::from(CONTOUR_PX / k)) } else { Style::Fill { seal: self.settings.seal } };
        let (palette, _) = self.svg_palette();
        let svg = vectorize::svg(t, &palette, SvgSize::Px(self.w as f64, self.h as f64), style);
        let buf = photocraft_io::svg::rasterize(svg.as_bytes(), k).ok()?;
        let (w, h) = (buf.rect.width() as usize, buf.rect.height() as usize);
        (w * h == buf.px.len() && w > 0 && h > 0).then(|| egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba8(&buf.px)))
    }

    /// The part `region` (traced px: x0, y0, x1, y1) of the result drawn `size` screen px: filled,
    /// or its contours only.
    fn render_region(&self, contours: bool, region: [f32; 4], size: [usize; 2]) -> Option<egui::ColorImage> {
        let t = self.traced.as_ref()?;
        let (rw, rh) = (region[2] - region[0], region[3] - region[1]);
        if rw <= 0.0 || rh <= 0.0 || size[0] == 0 || size[1] == 0 {
            return None;
        }
        let k = size[0] as f32 / rw;
        let style = if contours { Style::Contours(f64::from(CONTOUR_PX / k)) } else { Style::Fill { seal: self.settings.seal } };
        let (palette, _) = self.svg_palette();
        let svg = vectorize::svg(t, &palette, SvgSize::Px(f64::from(rw), f64::from(rh)), style);
        // The same drawing, its view box narrowed to the region.
        let whole = format!("viewBox=\"0 0 {} {}\"", t.width, t.height);
        if !svg.contains(&whole) {
            return None;
        }
        let svg = svg.replacen(&whole, &format!("viewBox=\"{} {} {} {}\"", region[0], region[1], rw, rh), 1);
        let buf = photocraft_io::svg::rasterize(svg.as_bytes(), k).ok()?;
        let (w, h) = (buf.rect.width() as usize, buf.rect.height() as usize);
        (w * h == buf.px.len() && w > 0 && h > 0).then(|| egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba8(&buf.px)))
    }

    /// The sharp texture of `region` at `size`, drawn when the view has rested; `None` until then
    /// (the preview texture shows meanwhile).
    fn sharp_texture(&mut self, ctx: &egui::Context, contours: bool, region: [f32; 4], size: [usize; 2], rested: bool) -> Option<egui::TextureHandle> {
        let slot = if contours { &self.sharp_contours } else { &self.sharp };
        if let Some(s) = slot
            && s.revision == self.revision
            && s.region == region
            && s.size == size
        {
            return Some(s.handle.clone());
        }
        if !rested {
            return None;
        }
        let image = self.render_region(contours, region, size)?;
        let name = if contours { "dtf-vectorizer-sharp-contours" } else { "dtf-vectorizer-sharp" };
        let handle = ctx.load_texture(name, image, egui::TextureOptions::LINEAR);
        let sharp = Some(Sharp { revision: self.revision, region, size, handle: handle.clone() });
        if contours {
            self.sharp_contours = sharp;
        } else {
            self.sharp = sharp;
        }
        Some(handle)
    }

    /// Zooms to `zoom` (kept between 1 and [`MAX_ZOOM`]) keeping the visual's point under `at`
    /// (screen) in place, for an image `base` wide and high at 100 % centred in `frame`.
    fn zoom_at(&mut self, zoom: f32, at: egui::Pos2, frame: Rect, base: egui::Vec2) {
        let zoom = zoom.clamp(1.0, MAX_ZOOM);
        let (w0, h0) = (base.x * self.zoom, base.y * self.zoom);
        let (w1, h1) = (base.x * zoom, base.y * zoom);
        let d = at - frame.center();
        let u = self.center[0] + d.x / w0.max(1.0);
        let v = self.center[1] + d.y / h0.max(1.0);
        self.center = [u - d.x / w1.max(1.0), v - d.y / h1.max(1.0)];
        self.zoom = zoom;
    }

    /// The colour sheet of the vectorized visual: the original and the vectorized result side by
    /// side, the SVG's colours with their references.
    pub fn sheet(&self) -> Option<ColorSheet> {
        let (q, t) = (self.quantized.as_ref()?, self.traced.as_ref()?);
        let (w, h, px) = analyzer::downsample(self.w, self.h, &self.px, SHEET_IMAGE_MAX);
        let image = (w, h, rgba8(&px));
        let k = w as f32 / self.w.max(1) as f32;
        let drawn = self.render_at(false, k)?;
        let [dw, dh] = drawn.size;
        let vectorized = (dw, dh, drawn.pixels.iter().flat_map(|c| c.to_srgba_unmultiplied()).collect());
        // The colours in the SVG, in the palette's order.
        let used: HashSet<usize> = t.paths.iter().map(|p| p.color).collect();
        let matches = self.matches();
        let rows = q
            .palette
            .iter()
            .enumerate()
            .filter(|(i, _)| used.contains(i))
            .map(|(i, c)| SheetRow {
                rgb: c.rgb,
                share: c.share * 100.0,
                range: false,
                reference: matches.get(i).copied().flatten().filter(|m| m.2).and_then(|(k, de, _)| {
                    self.swatch(k).map(|s| SheetRef { name: s.name.clone(), rgb: analyzer::swatch8(s), delta_e: de })
                }),
            })
            .collect();
        Some(ColorSheet {
            file_name: self.name.clone(),
            client: String::new(),
            date: color_sheet::today(),
            profile: analyzer::PROFILE.into(),
            swatch_book: self.book.as_ref().map(|b| b.name.clone()),
            size_px: self.size,
            dpi: self.dpi as f32,
            image,
            replaced: Some(vectorized),
            replaced_label: Some(tl!("Vectorized").to_string()),
            rows,
        })
    }

    fn original_texture(&mut self, ctx: &egui::Context) -> egui::TextureHandle {
        if let Some(t) = &self.original {
            return t.clone();
        }
        let image = egui::ColorImage::from_rgba_unmultiplied([self.w.max(1), self.h.max(1)], &rgba8(&self.px));
        let t = ctx.load_texture("dtf-vectorizer-original", image, egui::TextureOptions::LINEAR);
        self.original = Some(t.clone());
        t
    }

    fn result_texture(&mut self, ctx: &egui::Context, contours: bool) -> Option<egui::TextureHandle> {
        let slot = if contours { &self.contours } else { &self.vectorized };
        if let Some(t) = slot
            && t.revision == self.revision
        {
            return Some(t.handle.clone());
        }
        let image = self.render(contours)?;
        let name = if contours { "dtf-vectorizer-contours" } else { "dtf-vectorizer-result" };
        let handle = ctx.load_texture(name, image, egui::TextureOptions::LINEAR);
        let tex = Some(Tex { revision: self.revision, handle: handle.clone() });
        if contours {
            self.contours = tex;
        } else {
            self.vectorized = tex;
        }
        Some(handle)
    }
}

// ---------------------------------------------------------------- preferences

fn load_settings(app: &PhotocraftApp) -> Settings {
    let mut s: Settings = app.session.prefs().dialogs.get(SETTINGS_PREF).and_then(|v| serde_json::from_value(v.clone()).ok()).unwrap_or_default();
    s.colors = s.colors.clamp(2, 16);
    s
}

fn save_settings(app: &mut PhotocraftApp, s: &Settings) {
    let v = serde_json::to_value(s).unwrap_or(Value::Null);
    if app.session.prefs().dialogs.get(SETTINGS_PREF) != Some(&v) {
        app.session.prefs.edit(|prefs| prefs.dialogs.insert(SETTINGS_PREF.into(), v));
    }
}

// ---------------------------------------------------------------- opening and actions

/// DTF › Vectorization…: opens the module on the active document.
pub fn open(app: &mut PhotocraftApp) -> Result<Value, String> {
    let st = app.session.active().ok_or("no document")?;
    let (w, h, px) = cmds::visual_pixels(&st.doc).map_err(|e| e.to_string())?;
    if !px.iter().any(|q| q[3] >= 0.5) {
        return Err(tl!("No visible pixels to vectorize.").to_string());
    }
    let bits = match st.doc.depth {
        photocraft_color::SampleType::U8 => 8,
        photocraft_color::SampleType::U16 => 16,
        photocraft_color::SampleType::F32 => 32,
    };
    let mode = crate::i18n::fmt(tl!("RGB {bits} bits"), &[("bits", &bits.to_string())]);
    let (doc, name, path) = (st.doc.id, st.doc.name.clone(), st.path.clone());
    let (dpi, mm) = (cmds::dpi(&st.doc), cmds::size_mm(&st.doc));
    let v = Vectorizer::new(doc, &name, path, (w, h), dpi, mm, mode, &px, load_settings(app), analyzer::load_book(app));
    let colors = v.quantized.as_ref().map_or(0, |q| q.palette.len());
    app.vectorizer = Some(v);
    Ok(json!({"colors": colors}))
}

/// Menu entry point: DTF › Vectorization… without parameters opens the module; with parameters the
/// command runs (for automation).
pub fn menu(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    (id == cmds::VECTORIZE && params.as_object().is_none_or(|o| o.is_empty())).then(|| open(app))
}

pub fn close(app: &mut PhotocraftApp) {
    if let Some(v) = app.vectorizer.take() {
        save_settings(app, &v.settings);
    }
}

/// "Export SVG…": where to save, then `dtf.vectorize` with the module's settings; the SVG is
/// written there and, when asked, opened in a new tab. The module closes.
pub fn export_svg(app: &mut PhotocraftApp) -> Result<Value, String> {
    let v = app.vectorizer.as_ref().ok_or("the Vectorization module is not open")?;
    if v.traced.as_ref().is_none_or(|t| t.paths.is_empty()) {
        return Err(v.error.clone().unwrap_or_else(|| tl!("Nothing to export: every color is left out.").to_string()));
    }
    if app.session.active().map(|d| d.doc.id) != Some(v.doc) {
        return Err("the vectorized document is no longer the active one".into());
    }
    let mut params = v.params();
    params["open"] = json!(v.settings.open_after);
    let (_, hint) = cmds::result_name(&v.name, v.path.as_deref());
    let settings = v.settings.clone();
    save_settings(app, &settings);
    app.pick_save(&hint, move |app, path| {
        let path = std::path::Path::new(&path).with_extension("svg").to_string_lossy().into_owned();
        let r = app.run(cmds::VECTORIZE, params)?;
        let svg = r.get("svg").and_then(Value::as_str).ok_or("the vectorization returned no SVG")?;
        let write = app.services.write.as_mut().ok_or("no writer configured")?;
        write(&path, svg.as_bytes())?;
        app.vectorizer = None;
        app.ui.status = crate::i18n::fmt(tl!("SVG saved: {path}"), &[("path", &path)]);
        Ok(json!({"path": path, "colors": r["colors"], "shapes": r["shapes"], "document": r.get("document")}))
    })
}

/// "PDF Sheet…": the colour sheet of the vectorized visual, written where the user chooses.
pub fn export_pdf(app: &mut PhotocraftApp) -> Result<Value, String> {
    let v = app.vectorizer.as_ref().ok_or("the Vectorization module is not open")?;
    let sheet = v.sheet().ok_or_else(|| v.error.clone().unwrap_or_else(|| tl!("Nothing to export: every color is left out.").to_string()))?;
    let stem = std::path::Path::new(&v.name).file_stem().and_then(|s| s.to_str()).unwrap_or(&v.name).to_string();
    let suggested = format!("{}.pdf", crate::i18n::fmt(tl!("{name} vectorized color sheet"), &[("name", &stem)]));
    app.pick_save(&suggested, move |app, path| {
        let path = std::path::Path::new(&path).with_extension("pdf").to_string_lossy().into_owned();
        let bytes = color_sheet::pdf(&sheet);
        let write = app.services.write.as_mut().ok_or("no writer configured")?;
        write(&path, &bytes)?;
        app.ui.status = crate::i18n::fmt(tl!("Color sheet saved: {path}"), &[("path", &path)]);
        Ok(json!({"path": path}))
    })
}

// ---------------------------------------------------------------- drawing

enum Action {
    Close,
    Export,
    LoadBook,
    Pdf,
}

/// The module, over the main window while it is open.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(v) = app.vectorizer.as_ref() else { return };
    // Its document was closed.
    if !app.session.documents().iter().any(|d| d.doc.id == v.doc) {
        app.vectorizer = None;
        return;
    }
    let before = v.settings.clone();
    let mut action = None;
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::new("dtf-vectorizer")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let Some(v) = app.vectorizer.as_mut() else { return };
        action = draw(ui, v, screen);
    });
    if let Some(v) = app.vectorizer.as_mut()
        && v.changing.is_some()
    {
        change_reference(ctx, v);
    }
    if let Some(v) = app.vectorizer.as_mut()
        && (v.requantize || v.retrace)
    {
        if ctx.input(|i| i.pointer.any_down()) {
            ctx.request_repaint();
        } else {
            v.update();
            ctx.request_repaint();
        }
    }
    let escape = app.vectorizer.as_ref().is_some_and(|v| v.changing.is_none())
        && app.file_dialog.is_none()
        && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if escape {
        action = Some(Action::Close);
    }
    if let Some(v) = app.vectorizer.as_ref()
        && v.settings != before
    {
        let s = v.settings.clone();
        save_settings(app, &s);
    }
    let result = match action {
        None => return,
        Some(Action::Close) => {
            close(app);
            return;
        }
        Some(Action::Export) => export_svg(app),
        Some(Action::LoadBook) => analyzer::load_swatch_book(app),
        Some(Action::Pdf) => export_pdf(app),
    };
    if let Err(e) = result {
        app.ui.status = e;
        app.ui.status_error = true;
    }
}

fn draw(ui: &mut egui::Ui, v: &mut Vectorizer, screen: Rect) -> Option<Action> {
    let t = Tokens::get(ui.ctx());
    let (full, _) = ui.allocate_exact_size(screen.size(), Sense::click_and_drag());
    let painter = ui.painter().clone();
    painter.rect_filled(full, 0.0, t.chrome);
    // Title bar.
    let title = Rect::from_min_size(full.min, vec2(full.width(), 40.0));
    painter.rect_filled(title, 0.0, t.dock);
    painter.line_segment([title.left_bottom(), title.right_bottom()], Stroke::new(1.0, t.separator));
    let galley = painter.layout_no_wrap(tl!("Vectorization").to_string(), crate::theme::semibold(15.0), t.text);
    let title_w = galley.size().x;
    painter.galley(pos2(title.left() + 16.0, title.center().y - galley.size().y / 2.0), galley, t.text);
    let info = format!("{} · {} · {} × {} px · {}", v.name, v.mode, v.size.0, v.size.1, tl!("the document is not modified"));
    painter.text(pos2(title.left() + 32.0 + title_w, title.center().y), Align2::LEFT_CENTER, info, FontId::proportional(12.5), t.text_dim);
    let footer_h = 56.0;
    let body = Rect::from_min_max(pos2(full.left(), title.bottom()), pos2(full.right(), full.bottom() - footer_h));
    let right = Rect::from_min_max(pos2(body.right() - PANEL_W, body.top()), body.max);
    let left = Rect::from_min_max(body.min, pos2(right.left(), body.bottom()));
    painter.rect_filled(right, 0.0, t.dock);
    painter.line_segment([right.left_top(), right.left_bottom()], Stroke::new(1.0, t.separator));
    preview(ui, v, left);
    let mut action = None;
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(right.shrink2(vec2(0.0, 8.0))));
    egui::ScrollArea::vertical().id_salt("dtf-vectorizer-panel").auto_shrink([false, false]).show(&mut panel, |ui| {
        ui.add_space(4.0);
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 0)).show(ui, |ui| {
            ui.set_width(PANEL_W - 40.0);
            if settings_ui(ui, v) {
                action = Some(Action::LoadBook);
            }
        });
    });
    // Footer.
    let foot = Rect::from_min_max(pos2(full.left(), body.bottom()), full.max);
    painter.rect_filled(foot, 0.0, t.dock);
    painter.line_segment([foot.left_top(), foot.right_top()], Stroke::new(1.0, t.separator));
    let mut buttons = ui.new_child(egui::UiBuilder::new().max_rect(foot.shrink2(vec2(16.0, 10.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    let ready = v.traced.as_ref().is_some_and(|t| !t.paths.is_empty());
    let export = buttons
        .add_enabled_ui(ready, |ui| crate::widgets::primary_button(ui, tl!("Export SVG…"), 120.0))
        .inner
        .on_hover_text(tl!("Cut-out shapes at the document's print size, saved as SVG."));
    if export.clicked() {
        action = Some(Action::Export);
    }
    let pdf = buttons
        .add_enabled(ready, egui::Button::new(tl!("PDF Sheet…")))
        .on_hover_text(tl!("An A4 sheet with the visual, the vectorized result, its colors and their references."));
    if pdf.clicked() {
        action = Some(Action::Pdf);
    }
    if buttons.button(tl!("Close")).clicked() {
        action = Some(Action::Close);
    }
    action
}

/// The preview: its toolbar (views, contours, checkerboard), the visuals fitted below and the
/// result's figures.
fn preview(ui: &mut egui::Ui, v: &mut Vectorizer, area: Rect) {
    let t = Tokens::get(ui.ctx());
    let bar = Rect::from_min_size(area.min + vec2(16.0, 10.0), vec2(area.width() - 32.0, 30.0));
    let mut tools = ui.new_child(egui::UiBuilder::new().max_rect(bar).layout(egui::Layout::left_to_right(egui::Align::Center)));
    for (view, label) in [(View::Original, tl!("Original")), (View::Vectorized, tl!("Vectorized")), (View::SideBySide, tl!("Side by Side"))] {
        if tools.add(egui::Button::new(label).selected(v.view == view)).clicked() {
            v.view = view;
        }
    }
    tools.add_space(16.0);
    tools.checkbox(&mut v.settings.contours, tl!("Show Contours"));
    tools.checkbox(&mut v.settings.checker, tl!("Checkerboard under Transparency"));
    let status_h = 26.0;
    let view = Rect::from_min_max(pos2(area.left() + 16.0, bar.bottom() + 10.0), pos2(area.right() - 16.0, area.bottom() - status_h - 8.0));
    ui.painter().rect_filled(view, 4.0, t.canvas);
    // The result's figures, under the preview.
    let status = Rect::from_min_max(pos2(view.left(), view.bottom()), pos2(view.right(), area.bottom()));
    let figures = match (&v.traced, &v.error) {
        (Some(tr), _) => {
            let figures = crate::i18n::fmt(
                tl!("{colors} colors · {shapes} shapes · {points} points"),
                &[("colors", &tr.paths.len().to_string()), ("shapes", &tr.shapes().to_string()), ("points", &tr.points().to_string())],
            );
            // Sealed seams overlap by half a pixel: "no overlap" only when they are not.
            if v.settings.seal { figures } else { format!("{figures}   ·   {}", tl!("Cut-out shapes, no overlap")) }
        }
        (None, Some(e)) => e.clone(),
        (None, None) => String::new(),
    };
    ui.painter().text(pos2(status.left(), status.center().y), Align2::LEFT_CENTER, figures, FontId::proportional(12.0), if v.traced.is_some() { t.text_dim } else { t.warning });
    if view.width() < 60.0 || view.height() < 60.0 {
        return;
    }
    let inner = view.shrink(16.0);
    let (iw, ih) = (v.w.max(1) as f32, v.h.max(1) as f32);
    let fit = |space: Rect| {
        let k = (space.width() / iw).min(space.height() / ih);
        Rect::from_center_size(space.center(), vec2(iw * k, ih * k))
    };
    // The frames, each with the image fitted in it at 100 %.
    let frames: Vec<(Rect, bool)> = match v.view {
        View::Original => vec![(inner, false)],
        View::Vectorized => vec![(inner, true)],
        View::SideBySide => {
            let half = (inner.width() - 24.0) / 2.0;
            let l = Rect::from_min_size(inner.min + vec2(0.0, 22.0), vec2(half, inner.height() - 22.0));
            let r = Rect::from_min_size(pos2(inner.right() - half, inner.top() + 22.0), vec2(half, inner.height() - 22.0));
            vec![(l, false), (r, true)]
        }
    };
    let Some(&(frame0, _)) = frames.first() else { return };
    let base = fit(frame0).size();
    // Zoom and pan: the pointer over either frame moves both.
    for (k, (frame, _)) in frames.iter().enumerate() {
        let resp = ui.interact(*frame, ui.id().with(("dtf-vectorizer-view", k)), Sense::click_and_drag());
        if resp.hovered() {
            let (zoom_delta, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
            if (zoom_delta - 1.0).abs() > 1e-4
                && let Some(p) = resp.hover_pos()
            {
                v.zoom_at(v.zoom * zoom_delta, p, *frame, base);
            } else if v.zoom > 1.0 && scroll != egui::Vec2::ZERO {
                v.center[0] -= scroll.x / (base.x * v.zoom);
                v.center[1] -= scroll.y / (base.y * v.zoom);
            }
            if v.zoom > 1.0 {
                ui.ctx().set_cursor_icon(if resp.dragged() { egui::CursorIcon::Grabbing } else { egui::CursorIcon::Grab });
            }
        }
        if resp.dragged() {
            let d = resp.drag_delta();
            v.center[0] -= d.x / (base.x * v.zoom);
            v.center[1] -= d.y / (base.y * v.zoom);
        }
        if resp.double_clicked()
            && let Some(p) = resp.interact_pointer_pos()
        {
            if v.zoom > 1.0 {
                v.zoom = 1.0;
            } else {
                v.zoom_at(2.0, p, *frame, base);
            }
        }
    }
    // The visual stays in the frame: centred on an axis it doesn't fill.
    let (zw, zh) = (base.x * v.zoom, base.y * v.zoom);
    let keep = |c: f32, image: f32, room: f32| if image <= room { 0.5 } else { c.clamp(room / 2.0 / image, 1.0 - room / 2.0 / image) };
    v.center = [keep(v.center[0], zw, frame0.width()), keep(v.center[1], zh, frame0.height())];
    // The view rests: its visible part is drawn sharp.
    let now = ui.input(|i| i.time);
    let key = [v.zoom, v.center[0], v.center[1], frame0.width(), frame0.height(), if v.view == View::SideBySide { 1.0 } else { 0.0 }];
    if key != v.last_view {
        v.last_view = key;
        v.moved_at = now;
    }
    let rested = now - v.moved_at >= SHARP_DELAY;
    if !rested {
        ui.ctx().request_repaint_after(std::time::Duration::from_secs_f64(SHARP_DELAY));
    }
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    let ppp = ui.ctx().pixels_per_point();
    for (frame, vectorized) in frames {
        let rect = Rect::from_min_size(frame.center() - vec2(v.center[0] * zw, v.center[1] * zh), vec2(zw, zh));
        let shown = rect.intersect(frame);
        if v.view == View::SideBySide {
            let (label, detail) = if vectorized {
                (tl!("Vectorized"), format!("SVG · {} × {} mm", decimal(v.size_mm.0), decimal(v.size_mm.1)))
            } else {
                (tl!("Original"), format!("{} · {} × {} px", file_kind(&v.name), v.size.0, v.size.1))
            };
            let y = frame.top() - 10.0;
            ui.painter().text(pos2(frame.left(), y), Align2::LEFT_BOTTOM, label, crate::theme::semibold(12.5), t.text);
            ui.painter().text(pos2(frame.right(), y), Align2::RIGHT_BOTTOM, detail, crate::theme::mono(11.0), t.text_faint);
        }
        if shown.width() <= 0.0 || shown.height() <= 0.0 {
            continue;
        }
        let painter = ui.painter_at(frame);
        if v.settings.checker {
            crate::widgets::checker(&painter, shown, 8.0);
        } else {
            painter.rect_filled(shown, 0.0, Color32::WHITE);
        }
        // The visible part, in traced px and in screen px, for the sharp drawing.
        let region = [
            ((shown.left() - rect.left()) / zw * iw).max(0.0),
            ((shown.top() - rect.top()) / zh * ih).max(0.0),
            ((shown.right() - rect.left()) / zw * iw).min(iw),
            ((shown.bottom() - rect.top()) / zh * ih).min(ih),
        ];
        let region = region.map(|x| (x * 4.0).round() / 4.0);
        let size = [(shown.width() * ppp).round() as usize, (shown.height() * ppp).round() as usize];
        // Past the preview texture's own resolution, the result is drawn again for the screen.
        let sharp = v.zoom > 1.0 && zw * ppp > iw * v.preview_scale();
        let layer = |ui: &egui::Ui, v: &mut Vectorizer, contours: bool| {
            let fine = if sharp { v.sharp_texture(ui.ctx(), contours, region, size, rested) } else { None };
            match fine {
                Some(tex) => {
                    painter.image(tex.id(), shown, uv, Color32::WHITE);
                }
                None => {
                    if let Some(tex) = v.result_texture(ui.ctx(), contours) {
                        painter.image(tex.id(), rect, uv, Color32::WHITE);
                    }
                }
            }
        };
        if vectorized {
            layer(ui, v, false);
        } else {
            let tex = v.original_texture(ui.ctx());
            painter.image(tex.id(), rect, uv, Color32::WHITE);
        }
        if v.settings.contours && (vectorized || v.view == View::Original) {
            layer(ui, v, true);
        }
    }
    zoom_controls(ui, v, view, frame0, base);
}

/// − / zoom / + / fit, at the bottom right of the preview.
fn zoom_controls(ui: &mut egui::Ui, v: &mut Vectorizer, view: Rect, frame: Rect, base: egui::Vec2) {
    let t = Tokens::get(ui.ctx());
    let r = Rect::from_min_size(pos2(view.right() - 230.0, view.bottom() - 40.0), vec2(220.0, 30.0));
    ui.painter().rect_filled(r, 6.0, t.card);
    let mut bar = ui.new_child(egui::UiBuilder::new().max_rect(r.shrink2(vec2(6.0, 2.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
    bar.spacing_mut().item_spacing.x = 4.0;
    let step = |up: bool, z: f32| -> f32 {
        if up { ZOOMS.iter().copied().find(|s| *s > z + 1e-3).unwrap_or(MAX_ZOOM) } else { ZOOMS.iter().rev().copied().find(|s| *s < z - 1e-3).unwrap_or(1.0) }
    };
    if bar.add_enabled(v.zoom < MAX_ZOOM, egui::Button::new("+")).on_hover_text(tl!("Zoom In")).clicked() {
        v.zoom_at(step(true, v.zoom), frame.center(), frame, base);
    }
    bar.add(egui::Label::new(RichText::new(format!("{:.0} %", v.zoom * 100.0)).color(t.text)).wrap_mode(egui::TextWrapMode::Extend));
    if bar.add_enabled(v.zoom > 1.0, egui::Button::new("−")).on_hover_text(tl!("Zoom Out")).clicked() {
        v.zoom_at(step(false, v.zoom), frame.center(), frame, base);
    }
    if bar.add_enabled(v.zoom > 1.0, egui::Button::new(tl!("Fit on Screen"))).clicked() {
        v.zoom = 1.0;
        v.center = [0.5, 0.5];
    }
}

/// "PNG" for "logo.png" (the name itself when it has no extension).
fn file_kind(name: &str) -> String {
    std::path::Path::new(name).extension().and_then(|e| e.to_str()).map_or_else(|| name.to_string(), str::to_ascii_uppercase)
}

/// A row of mutually exclusive buttons; true when the value changed.
fn choice<T: Copy + PartialEq>(ui: &mut egui::Ui, value: &mut T, options: &[(T, &str)]) -> bool {
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 2.0;
        for (o, label) in options {
            if ui.add(egui::Button::new(*label).selected(*value == *o)).clicked() && *value != *o {
                *value = *o;
                changed = true;
            }
        }
    });
    changed
}

fn patch(ui: &mut egui::Ui, rgb: [u8; 3], size: f32) -> egui::Response {
    let (r, resp) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(r, 2.0, c32(rgb));
    ui.painter().rect_stroke(r, 2.0, Stroke::new(1.0, Tokens::get(ui.ctx()).field_border), egui::StrokeKind::Inside);
    resp
}

/// The settings panel: source, settings, swatch book, palette, export. True when "Load…" was
/// clicked.
fn settings_ui(ui: &mut egui::Ui, v: &mut Vectorizer) -> bool {
    let t = Tokens::get(ui.ctx());
    let section = |ui: &mut egui::Ui, title: &str| {
        ui.add_space(10.0);
        ui.label(RichText::new(title).font(crate::theme::semibold(13.0)));
        ui.add_space(2.0);
        crate::widgets::hairline(ui);
        ui.add_space(4.0);
    };
    crate::dtf_help::sheet_toggle(ui, &mut v.help, cmds::VECTORIZE);
    section(ui, tl!("Source"));
    let print = crate::i18n::fmt(tl!("{w} × {h} mm at {dpi} ppi"), &[("w", &decimal(v.size_mm.0)), ("h", &decimal(v.size_mm.1)), ("dpi", &decimal(v.dpi))]);
    egui::Grid::new("dtf-vectorizer-source").num_columns(2).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
        for (k, val) in [(tl!("Document"), tl!("Open document, flattened").to_string()), (tl!("Print Size"), print)] {
            ui.label(RichText::new(k).color(t.text_dim));
            ui.label(val);
            ui.end_row();
        }
    });
    section(ui, tl!("Settings"));
    let mut redo = Redo::Nothing;
    egui::Grid::new("dtf-vectorizer-settings").num_columns(2).spacing(vec2(12.0, 8.0)).show(ui, |ui| {
        ui.label(tl!("Type of Visual"));
        if choice(ui, &mut v.settings.kind, &[(Kind::Logo, tl!("Logo · flat colors")), (Kind::Illustration, tl!("Illustration · shades"))]) {
            redo = Redo::Quantize;
        }
        ui.end_row();
        ui.label(tl!("Number of Colors"));
        if ui.add(egui::Slider::new(&mut v.settings.colors, 2..=16)).changed() {
            redo = Redo::Quantize;
        }
        ui.end_row();
        ui.label(tl!("Detail Level"));
        if choice(ui, &mut v.settings.detail, &[(Detail::Low, tl!("Low Detail")), (Detail::Medium, tl!("Medium Detail")), (Detail::High, tl!("High Detail"))]) {
            redo = Redo::Quantize;
        }
        ui.end_row();
        ui.label(tl!("Smoothing"));
        if choice(ui, &mut v.settings.smoothing, &[(Smoothing::Curves, tl!("Curves")), (Smoothing::Corners, tl!("Sharp Corners"))]) && redo == Redo::Nothing {
            redo = Redo::Trace;
        }
        ui.end_row();
    });
    ui.add_space(4.0);
    let background = v.background();
    ui.horizontal(|ui| {
        let check = ui.add_enabled(background.is_some(), egui::Checkbox::new(&mut v.settings.remove_background, tl!("Remove the Background")));
        if check.changed() && redo == Redo::Nothing {
            redo = Redo::Trace;
        }
        if let Some(rgb) = background {
            ui.add_space(6.0);
            patch(ui, rgb, 16.0);
            ui.label(RichText::new(vectorize::hex(rgb)).font(crate::theme::mono(11.5)));
            ui.label(RichText::new(tl!("read on the visual's edge")).small().color(t.text_faint));
        }
    });
    if background.is_none() {
        ui.label(RichText::new(tl!("No background found: the visual is already cut out.")).small().color(t.text_faint));
    }
    let seal = ui
        .checkbox(&mut v.settings.seal, tl!("Close the Seams between Colors"))
        .on_hover_text(tl!("Each shape overlaps its neighbors by half a pixel, so no transparent hairline shows between two colors in print. Uncheck for strictly cut-out shapes (cutting plotter)."));
    if seal.changed() {
        // Only the drawing changes: the shapes stay.
        v.revision += 1;
    }
    v.redo(redo);
    section(ui, tl!("Reference Swatch Book"));
    let mut load = false;
    ui.horizontal(|ui| {
        let text = match &v.book {
            Some(b) => crate::i18n::fmt(tl!("{name} · {n} colors"), &[("name", &b.name), ("n", &b.swatches.len().to_string())]),
            None => tl!("None loaded").to_string(),
        };
        ui.add(egui::Label::new(text).truncate()).on_hover_text(v.book.as_ref().map_or("", |b| b.name.as_str()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            load = ui.button(tl!("Load…")).clicked();
        });
    });
    ui.label(RichText::new(tl!("Shared with the Color Analyzer. Formats: .ase, .aco, .csv; difference in ΔE 2000.")).small().color(t.text_faint));
    palette_ui(ui, v, &section);
    section(ui, tl!("Export"));
    egui::Grid::new("dtf-vectorizer-export").num_columns(2).spacing(vec2(12.0, 4.0)).show(ui, |ui| {
        let size = crate::i18n::fmt(tl!("{w} × {h} mm (document size)"), &[("w", &decimal(v.size_mm.0)), ("h", &decimal(v.size_mm.1))]);
        for (k, val) in [(tl!("Format"), tl!("SVG · cut-out shapes").to_string()), (tl!("Size"), size)] {
            ui.label(RichText::new(k).color(t.text_dim));
            ui.label(val);
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.checkbox(&mut v.settings.open_after, tl!("Open the SVG in DTF Craft after the export"));
    ui.add_space(12.0);
    load
}

/// The palette: each colour with its share, the background marked, and its swatch-book reference;
/// unchecked colours are left out of the SVG, colours checked in ⇄ take their reference's colour
/// and name in the SVG.
fn palette_ui(ui: &mut egui::Ui, v: &mut Vectorizer, section: &dyn Fn(&mut egui::Ui, &str)) {
    let t = Tokens::get(ui.ctx());
    let Some(q) = v.quantized.as_ref() else { return };
    let in_svg = v.traced.as_ref().map_or(0, |tr| tr.paths.len());
    section(
        ui,
        &format!(
            "{} · {}",
            tl!("Detected Colors"),
            crate::i18n::fmt(tl!("{n} colors · {k} in the SVG"), &[("n", &q.palette.len().to_string()), ("k", &in_svg.to_string())])
        ),
    );
    let matches = v.matches();
    let mut toggled: Option<(String, bool)> = None;
    let mut replace: Option<(String, bool)> = None;
    let mut change = None;
    egui::Grid::new("dtf-vectorizer-palette").num_columns(6).spacing(vec2(6.0, 6.0)).show(ui, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.label(RichText::new("SVG").small().color(t.text_dim));
        ui.add(crate::icons::image("arrow-left-right", 14.0, t.text_dim)).on_hover_text(tl!("Replaced by the reference in the SVG"));
        ui.label(RichText::new(tl!("Color")).small().color(t.text_dim));
        ui.label(RichText::new(tl!("Area")).small().color(t.text_dim));
        ui.label(RichText::new(tl!("Match")).small().color(t.text_dim));
        ui.label(RichText::new("ΔE").small().color(t.text_dim));
        ui.end_row();
        for (i, c) in q.palette.iter().enumerate() {
            let hex = vectorize::hex(c.rgb);
            let mut keep = !v.excluded.contains(&hex);
            let check = ui.add(egui::Checkbox::without_text(&mut keep)).on_hover_text(crate::i18n::fmt(tl!("Keep {hex} in the SVG"), &[("hex", &hex)]));
            if check.changed() {
                toggled = Some((hex.clone(), !keep));
            }
            let m = matches.get(i).copied().flatten();
            let mut on = m.is_some_and(|m| m.2);
            if ui.add_enabled(m.is_some(), egui::Checkbox::without_text(&mut on)).on_hover_text(tl!("Replaced by the reference in the SVG")).changed() {
                replace = Some((hex.clone(), on));
            }
            // The RGB values on hover: the reference's name keeps the room.
            ui.horizontal(|ui| {
                patch(ui, c.rgb, 22.0).on_hover_text(format!("RGB {} · {} · {}", c.rgb[0], c.rgb[1], c.rgb[2]));
                ui.vertical(|ui| {
                    ui.label(RichText::new(&hex).font(crate::theme::mono(11.5)));
                    if q.background == Some(i) {
                        ui.label(RichText::new(tl!("Edge Background")).small().color(t.warning));
                    }
                });
            });
            ui.label(RichText::new(format!("{} %", decimal(f64::from(c.share) * 100.0))).font(crate::theme::mono(11.5)));
            match m.and_then(|(k, de, _)| v.swatch(k).map(|s| (s, de))) {
                Some((s, de)) => {
                    ui.horizontal(|ui| {
                        patch(ui, analyzer::swatch8(s), 22.0);
                        ui.vertical(|ui| {
                            // Long swatch names are cut so the ΔE column stays in the panel.
                            ui.set_max_width(122.0);
                            ui.add(egui::Label::new(RichText::new(&s.name).strong()).truncate()).on_hover_text(&s.name);
                            if ui.link(RichText::new(tl!("Change…")).small()).clicked() {
                                change = Some(i);
                            }
                        });
                    });
                    let (bg, fg) = analyzer::de_badge(de);
                    let galley = ui.painter().layout_no_wrap(color_sheet::decimal(f64::from(de), 1), crate::theme::mono(11.0), fg);
                    let (r, _) = ui.allocate_exact_size(galley.size() + vec2(10.0, 4.0), Sense::hover());
                    ui.painter().rect_filled(r, 3.0, bg);
                    ui.painter().galley(r.center() - galley.size() / 2.0, galley, fg);
                }
                None => {
                    ui.label(RichText::new("—").color(t.text_faint));
                    ui.label("");
                }
            }
            ui.end_row();
        }
    });
    if v.book.is_none() {
        ui.label(RichText::new(tl!("Load a swatch book to find the references.")).color(t.warning));
    }
    ui.add_space(4.0);
    ui.label(RichText::new(tl!("SVG unchecked: the color is left out of the SVG; its shapes are removed, not merged.")).small().color(t.text_faint));
    ui.label(
        RichText::new(tl!("⇄ checked: the shapes take the reference's color and are named after it in the SVG. ΔE: up to 2 = imperceptible · 2 to 5 = visible up close · above = another hue"))
            .small()
            .color(t.text_faint),
    );
    if let Some((hex, excluded)) = toggled {
        v.set_excluded(&hex, excluded);
    }
    if let Some((hex, on)) = replace {
        v.set_replace(&hex, on);
    }
    if change.is_some() {
        v.changing = change;
        v.search.clear();
    }
}

/// "Change…": the reference of a palette colour.
fn change_reference(ctx: &egui::Context, v: &mut Vectorizer) {
    let target = v.changing.and_then(|i| v.quantized.as_ref()?.palette.get(i).map(|c| (i, c.rgb)));
    let (Some((i, rgb)), Some(book)) = (target, v.book.as_ref()) else {
        v.changing = None;
        return;
    };
    let current = v.matches().get(i).copied().flatten().map(|m| m.0);
    let mut search = std::mem::take(&mut v.search);
    let pick = analyzer::swatch_picker(ctx, "dtf-vectorizer-change", rgb, analyzer::lab8(rgb), current, book, &mut search);
    v.search = search;
    match pick {
        Pick::Open => return,
        Pick::Close => {}
        Pick::Chosen(choice) => v.set_swatch(&vectorize::hex(rgb), choice),
    }
    v.changing = None;
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    const BLUE: [f32; 3] = [31.0 / 255.0, 79.0 / 255.0, 181.0 / 255.0];
    const ORANGE: [f32; 3] = [228.0 / 255.0, 87.0 / 255.0, 46.0 / 255.0];

    /// The files the app wrote: path and bytes.
    type Written = std::rc::Rc<std::cell::RefCell<Vec<(String, Vec<u8>)>>>;

    /// An app with a 60 × 40 visual at 254 ppi on white: a blue square with a white dot, an
    /// orange square; and the files written.
    fn app() -> (PhotocraftApp, Written) {
        let written = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
        let files = written.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                write: Some(Box::new(move |path, bytes| {
                    files.borrow_mut().push((path.to_string(), bytes.to_vec()));
                    Ok(())
                })),
                ..Default::default()
            },
        );
        app.run("file.new", json!({"width": 60, "height": 40, "resolution": 254, "background": "white", "name": "logo.png"})).unwrap();
        app.session
            .edit("Paint", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 8..32 {
                    for x in 6..54 {
                        let dot = (14..20).contains(&x) && (17..23).contains(&y);
                        let c = if dot { [1.0; 3] } else if x < 30 { BLUE } else { ORANGE };
                        surf.write_pixel(x, y, &[c[0], c[1], c[2], 1.0]);
                    }
                }
                Ok(())
            })
            .unwrap();
        (app, written)
    }

    #[test]
    fn the_menu_opens_the_module_and_blocks_the_other_menus() {
        let (mut app, _) = app();
        let ctx = egui::Context::default();
        crate::menus::invoke(&mut app, &ctx, cmds::VECTORIZE, json!({})).unwrap();
        let v = app.vectorizer.as_ref().unwrap();
        let q = v.quantized.as_ref().unwrap();
        // Largest share first: orange (576 px) before blue (540 px, its white dot aside).
        assert_eq!(q.palette.iter().map(|c| vectorize::hex(c.rgb)).collect::<Vec<_>>(), ["#FFFFFF", "#E4572E", "#1F4FB5"]);
        assert_eq!(v.background(), Some([255, 255, 255]));
        assert_eq!((v.size_mm.0.round(), v.size_mm.1.round()), (6.0, 4.0));
        // White dot, blue, orange: the white around is removed.
        assert_eq!(v.traced.as_ref().unwrap().shapes(), 3);
        assert!(!crate::menus::modal_allows(&app, "image.imageSize"));
        // With parameters the command answers.
        app.vectorizer = None;
        let r = crate::menus::invoke(&mut app, &ctx, cmds::VECTORIZE, json!({"colors": 4})).unwrap();
        assert!(r["svg"].as_str().is_some_and(|s| s.contains("<svg ")));
        assert!(app.vectorizer.is_none());
    }

    #[test]
    fn settings_trace_again_and_left_out_colours_leave_the_svg() {
        let (mut app, _) = app();
        open(&mut app).unwrap();
        let v = app.vectorizer.as_mut().unwrap();
        let rev = v.revision;
        v.set_excluded("#E4572E", true);
        v.update();
        assert!(v.revision > rev);
        assert_eq!(v.traced.as_ref().unwrap().paths.len(), 2);
        assert_eq!(v.params()["exclude"], json!(["#E4572E"]));
        assert!(!v.svg().unwrap().contains("#E4572E"));
        // The background kept: the white around is one more shape.
        v.settings.remove_background = false;
        v.redo(Redo::Trace);
        v.update();
        assert_eq!(v.traced.as_ref().unwrap().shapes(), 3);
        // Fewer colours: analysed again; a colour left out of another palette is not sent.
        v.excluded.insert("#123456".into());
        v.settings.colors = 2;
        v.redo(Redo::Quantize);
        v.update();
        assert_eq!(v.quantized.as_ref().unwrap().palette.len(), 2);
        assert!(v.params()["exclude"].as_array().unwrap().iter().all(|h| h != "#123456"));
        assert_eq!(v.params()["colors"], 2);
    }

    #[test]
    fn the_preview_renders_the_shapes_and_their_contours() {
        let (mut app, _) = app();
        open(&mut app).unwrap();
        let v = app.vectorizer.as_ref().unwrap();
        let fill = v.render(false).unwrap();
        // 60 px traced, drawn at 4× (the cap).
        assert_eq!(fill.size, [240, 160]);
        let at = |img: &egui::ColorImage, x: usize, y: usize| img.pixels[y * img.size[0] + x];
        assert_eq!(at(&fill, 2, 2).a(), 0, "the white background is removed");
        let blue = at(&fill, 4 * 10, 4 * 12);
        assert_eq!((blue.r(), blue.g(), blue.b()), (31, 79, 181));
        let lines = v.render(true).unwrap();
        assert_eq!(at(&lines, 4 * 10, 4 * 12).a(), 0, "contours only");
        assert!(lines.pixels.iter().any(|p| p.a() > 0));
    }

    /// Two colours meeting on a wavy line, the whole image covered: sealed, no pixel inside is
    /// transparent.
    #[test]
    fn sealed_seams_leave_no_transparent_hairline() {
        let (w, h) = (90usize, 60usize);
        let mut px = Vec::with_capacity(w * h);
        for y in 0..h {
            for x in 0..w {
                let wave = 30.0 + 9.0 * (x as f32 / 9.0).sin();
                let c = if (y as f32 + 0.5) < wave { ORANGE } else { BLUE };
                px.push([c[0], c[1], c[2], 1.0]);
            }
        }
        let settings = Settings { remove_background: false, ..Settings::default() };
        let mut v = Vectorizer::new(photocraft_doc::DocId::fresh(), "vague.png", None, (w as u32, h as u32), 300.0, (7.62, 5.08), "RVB 8 bits".into(), &px, settings, None);
        let holes = |img: &egui::ColorImage| {
            let [iw, ih] = img.size;
            // Inside only: the outer edge is antialiased whatever the seams.
            (8..ih - 8).flat_map(|y| (8..iw - 8).map(move |x| (x, y))).filter(|(x, y)| img.pixels[y * iw + x].a() < 250).count()
        };
        assert_eq!(holes(&v.render(false).unwrap()), 0, "sealed");
        v.settings.seal = false;
        assert!(!v.svg().unwrap().contains("stroke"));
        assert_eq!(v.params()["sealSeams"], false);
    }

    #[test]
    fn the_export_writes_the_svg_and_opens_it_in_a_new_tab() {
        let (mut app, written) = app();
        open(&mut app).unwrap();
        app.vectorizer.as_mut().unwrap().set_excluded("#E4572E", true);
        let (dialog, _) = crate::file_dialog::fake(vec![Some(crate::file_dialog::FileDialogAnswer::SaveTo("out/logo vectorisé.png".into()))]);
        app.services.file_dialog = Some(dialog);
        export_svg(&mut app).unwrap();
        app.poll_file_dialog(&egui::Context::default(), None);
        assert!(app.vectorizer.is_none(), "closed after the export");
        let files = written.borrow();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].0, "out/logo vectorisé.svg");
        let svg = String::from_utf8(files[0].1.clone()).unwrap();
        assert!(svg.contains("width=\"6mm\" height=\"4mm\""), "{svg}");
        assert!(svg.contains("#1F4FB5") && !svg.contains("#E4572E"));
        // Opened in a new tab as shape layers; the original is unchanged.
        assert_eq!(app.session.documents().len(), 2);
        let copy = app.session.active().unwrap();
        assert_eq!(copy.doc.name, "logo vectorisé");
        assert!(copy.doc.layers.iter().all(|l| matches!(l.content, photocraft_doc::LayerContent::Shape(_))));
        assert_eq!(app.session.documents()[0].doc.name, "logo.png");
    }

    #[test]
    fn nothing_is_exported_when_every_colour_is_left_out() {
        let (mut app, written) = app();
        open(&mut app).unwrap();
        let v = app.vectorizer.as_mut().unwrap();
        for h in ["#FFFFFF", "#1F4FB5", "#E4572E"] {
            v.set_excluded(h, true);
        }
        v.update();
        assert!(export_svg(&mut app).is_err());
        assert!(written.borrow().is_empty());
        assert!(app.vectorizer.is_some());
    }

    #[test]
    fn a_visual_without_visible_pixels_does_not_open() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        assert!(open(&mut app).is_err(), "no document");
        app.run("file.new", json!({"width": 10, "height": 10, "background": "transparent"})).unwrap();
        assert!(open(&mut app).is_err());
        assert!(app.vectorizer.is_none());
    }

    /// A screenshot of the module in French on an antialiased logo (`DTF_VISUALS=<folder>`).
    #[test]
    fn capture_vectorizer() {
        let Some(dir) = std::env::var_os("DTF_VISUALS").map(std::path::PathBuf::from) else { return };
        std::fs::create_dir_all(&dir).unwrap();
        let _fr = crate::i18n::language_scope(crate::i18n::Lang::from_pref("fr"));
        let mut session = photocraft_engine::Session::new();
        session.execute("file.new", json!({"width": 600, "height": 600, "resolution": 150, "background": "white", "name": "logo-client.png"})).unwrap();
        session
            .edit("Logo", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                let rgb = |h: u32| [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0];
                let mix = |a: [f32; 3], b: [f32; 3], k: f32| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * k);
                let cover = |edge: f32| (edge + 0.5).clamp(0.0, 1.0);
                for y in 0..600 {
                    for x in 0..600 {
                        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                        let d = ((fx - 300.0).powi(2) + (fy - 300.0).powi(2)).sqrt();
                        let sun = ((fx - 300.0).powi(2) + (fy - 240.0).powi(2)).sqrt();
                        let wave = 360.0 + 18.0 * (fx / 55.0).sin();
                        let mut c = rgb(0xFFFFFF);
                        c = mix(c, rgb(0x14264F), cover(280.0 - d));
                        c = mix(c, rgb(0x8FC1E3), cover(250.0 - d));
                        c = mix(c, rgb(0xF2C14E), cover(75.0 - sun) * cover(250.0 - d));
                        c = mix(c, rgb(0x1F4FB5), cover(fy - wave) * cover(250.0 - d));
                        c = mix(c, rgb(0xE4572E), cover(14.0 - (fy - wave).abs()) * cover(250.0 - d));
                        surf.write_pixel(x, y, &[c[0], c[1], c[2], 1.0]);
                    }
                }
                Ok(())
            })
            .unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1440.0, 900.0)).wgpu().build_eframe(move |cc| {
            PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
            PhotocraftApp::new(session, crate::Services::default())
        });
        h.state_mut().session.prefs.edit(|p| p.interface.language = "fr".into());
        h.run_steps(4);
        open(h.state_mut()).unwrap();
        h.state_mut().vectorizer.as_mut().unwrap().settings.contours = false;
        h.run_steps(6);
        h.render().unwrap().save(dir.join("vectorisation.png")).unwrap();
        h.state_mut().vectorizer.as_mut().unwrap().settings.contours = true;
        h.state_mut().vectorizer.as_mut().unwrap().view = View::Vectorized;
        h.run_steps(6);
        h.render().unwrap().save(dir.join("vectorisation_contours.png")).unwrap();
        // Side by side at 400 %, on the sun's edge, once drawn sharp.
        let v = h.state_mut().vectorizer.as_mut().unwrap();
        v.settings.contours = false;
        v.view = View::SideBySide;
        v.zoom = 4.0;
        v.center = [0.62, 0.36];
        h.run_steps(30);
        h.render().unwrap().save(dir.join("vectorisation_zoom.png")).unwrap();
        // With the swatch book, and its PDF sheet.
        let csv = b"Ref;R;V;B\nPANTONE 2768 C;20;36;84\nPANTONE 2905 C;141;200;232\nPANTONE 7404 C;244;196;76\nPANTONE 2728 C;30;77;180\nPANTONE Warm Red C;228;84;48\nBlanc;255;255;255\n";
        let book = Book { name: "Pantone DTF.csv".into(), swatches: photocraft_algo::dtf_color::parse_swatch_book("x.csv", csv).unwrap() };
        let v = h.state_mut().vectorizer.as_mut().unwrap();
        v.zoom = 1.0;
        v.center = [0.5, 0.5];
        v.set_book(Some(book));
        h.run_steps(6);
        h.render().unwrap().save(dir.join("vectorisation_nuancier.png")).unwrap();
        let sheet = h.state().vectorizer.as_ref().unwrap().sheet().unwrap();
        std::fs::write(dir.join("vectorisation_fiche.pdf"), crate::color_sheet::pdf(&sheet)).unwrap();
    }

    /// Zoomed in, the visible part of the result is drawn again at the screen's resolution;
    /// zooming keeps the point under the pointer in place.
    #[test]
    fn the_zoomed_preview_draws_the_visible_part_sharp() {
        let (mut app, _) = app();
        open(&mut app).unwrap();
        let v = app.vectorizer.as_mut().unwrap();
        // Traced px 8..14 × 10..14 drawn 60 × 40: 10 screen px a traced px.
        let img = v.render_region(false, [8.0, 10.0, 14.0, 14.0], [60, 40]).unwrap();
        assert_eq!(img.size, [60, 40]);
        let blue = img.pixels[25 * 60 + 25];
        assert_eq!((blue.r(), blue.g(), blue.b()), (31, 79, 181));
        let frame = Rect::from_min_size(pos2(0.0, 0.0), vec2(600.0, 400.0));
        let base = vec2(600.0, 400.0);
        let at = pos2(450.0, 100.0);
        let point = |v: &Vectorizer| [v.center[0] + (at.x - 300.0) / (base.x * v.zoom), v.center[1] + (at.y - 200.0) / (base.y * v.zoom)];
        let before = point(v);
        v.zoom_at(4.0, at, frame, base);
        let after = point(v);
        assert_eq!(v.zoom, 4.0);
        assert!((before[0] - after[0]).abs() < 1e-5 && (before[1] - after[1]).abs() < 1e-5);
        v.zoom_at(20.0, at, frame, base);
        assert_eq!(v.zoom, MAX_ZOOM, "800 % at most");
    }

    /// Ctrl+wheel over the preview zooms in; once the view rests the sharp drawing replaces the
    /// preview texture; Fit on Screen comes back to 100 %.
    #[test]
    fn the_wheel_zooms_the_preview_and_fit_comes_back() {
        let (mut app, _) = app();
        open(&mut app).unwrap();
        app.vectorizer.as_mut().unwrap().view = View::Vectorized;
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                let ready = egui::Id::new("vectorizer-zoom-fonts");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ALL[0]);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    return;
                }
                show(app, ui.ctx());
            },
            app,
        );
        h.run_steps(3);
        h.hover_at(pos2(300.0, 400.0));
        h.run();
        h.event(egui::Event::Zoom(3.0));
        h.run_steps(2);
        let zoom = h.state().vectorizer.as_ref().unwrap().zoom;
        assert!(zoom > 2.5, "{zoom}");
        h.run_steps(30);
        assert!(h.state().vectorizer.as_ref().unwrap().sharp.is_some(), "drawn sharp once the view rests");
        h.get_by_label("Fit on Screen").click();
        h.run_steps(2);
        assert_eq!(h.state().vectorizer.as_ref().unwrap().zoom, 1.0);
    }

    #[test]
    fn the_module_draws_over_the_window_and_closes_with_escape() {
        let (mut app, _) = app();
        open(&mut app).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                let ready = egui::Id::new("vectorizer-test-fonts");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ALL[0]);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    return;
                }
                show(app, ui.ctx());
            },
            app,
        );
        h.run_steps(3);
        assert!(h.query_by_label("Export SVG…").is_some());
        assert!(h.query_by_label("#1F4FB5").is_some());
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().vectorizer.is_none());
        // The settings are remembered.
        assert!(h.state().session.prefs().dialogs.contains_key(SETTINGS_PREF));
    }

    /// A swatch book with a blue and an orange close to the visual's, and black.
    fn book() -> Book {
        let csv = b"Ref;R;V;B\nEX-201 Bleu roi;30;80;180\nEX-114 Orange vif;226;90;48\nNoir;0;0;0\n";
        Book { name: "Ex.csv".into(), swatches: photocraft_algo::dtf_color::parse_swatch_book("Ex.csv", csv).unwrap() }
    }

    /// The Color Analyzer's swatch book is the module's: the close colours take their references'
    /// colours and names in the SVG; unchecked or picked by hand, they follow the choice.
    #[test]
    fn the_swatch_book_recolours_and_names_the_svg() {
        let (mut app, written) = app();
        crate::color_analyzer_ui::save_book(&mut app, &book());
        open(&mut app).unwrap();
        let v = app.vectorizer.as_mut().unwrap();
        assert_eq!(v.book.as_ref().map(|b| b.swatches.len()), Some(3), "shared with the Color Analyzer");
        let q = v.quantized.as_ref().unwrap();
        let blue = q.palette.iter().position(|c| vectorize::hex(c.rgb) == "#1F4FB5").unwrap();
        let m = v.matches();
        assert!(m[blue].is_some_and(|(k, de, on)| k == 0 && de < 2.0 && on), "{m:?}");
        let svg = v.svg().unwrap();
        assert!(svg.contains("id=\"EX-201_Bleu_roi\"") && svg.contains("#1E50B4") && !svg.contains("#1F4FB5"), "{svg}");
        assert!(svg.contains("id=\"EX-114_Orange_vif\""), "{svg}");
        // Unchecked: the colour stays as it is.
        v.set_replace("#E4572E", false);
        let svg = v.svg().unwrap();
        assert!(svg.contains("couleur-E4572E") && svg.contains("#E4572E"), "{svg}");
        // Picked by hand: replaced, even far away.
        v.set_swatch("#1F4FB5", Some(2));
        let svg = v.svg().unwrap();
        assert!(svg.contains("id=\"Noir\"") && svg.contains("#000000"), "{svg}");
        assert_eq!(v.params()["replace"], json!([{"from": "#1F4FB5", "to": "#000000", "name": "Noir"}]));
        // The export writes the same SVG and names the layers of the new tab.
        let (dialog, _) = crate::file_dialog::fake(vec![Some(crate::file_dialog::FileDialogAnswer::SaveTo("out/logo vectorisé.svg".into()))]);
        app.services.file_dialog = Some(dialog);
        export_svg(&mut app).unwrap();
        app.poll_file_dialog(&egui::Context::default(), None);
        let svg = String::from_utf8(written.borrow()[0].1.clone()).unwrap();
        assert!(svg.contains("id=\"Noir\"") && svg.contains("couleur-E4572E"), "{svg}");
        assert!(app.session.active().unwrap().doc.layers.iter().any(|l| l.name == "Noir"));
    }

    #[test]
    fn the_pdf_sheet_shows_the_vectorized_visual_and_its_references() {
        let (mut app, written) = app();
        crate::color_analyzer_ui::save_book(&mut app, &book());
        open(&mut app).unwrap();
        let v = app.vectorizer.as_mut().unwrap();
        v.set_excluded("#E4572E", true);
        v.update();
        let sheet = v.sheet().unwrap();
        assert_eq!(sheet.replaced_label.as_deref(), Some("Vectorized"));
        let (w, h, _) = &sheet.image;
        let (vw, vh, vpx) = sheet.replaced.as_ref().unwrap();
        assert!((w.abs_diff(*vw), h.abs_diff(*vh)) <= (1, 1) && vpx.len() == vw * vh * 4);
        // Only the colours in the SVG; the blue with its reference.
        assert!(sheet.rows.iter().all(|r| r.rgb != [228, 87, 46]), "{:?}", sheet.rows);
        let blue = sheet.rows.iter().find(|r| r.rgb == [31, 79, 181]).unwrap();
        assert_eq!(blue.reference.as_ref().map(|r| r.name.as_str()), Some("EX-201 Bleu roi"));
        let (dialog, _) = crate::file_dialog::fake(vec![Some(crate::file_dialog::FileDialogAnswer::SaveTo("out/fiche".into()))]);
        app.services.file_dialog = Some(dialog);
        export_pdf(&mut app).unwrap();
        app.poll_file_dialog(&egui::Context::default(), None);
        let files = written.borrow();
        assert_eq!(files[0].0, "out/fiche.pdf");
        assert!(files[0].1.starts_with(b"%PDF"));
        assert!(app.vectorizer.is_some(), "the module stays open");
    }

    #[test]
    fn change_picks_another_reference_in_the_list() {
        let (mut app, _) = app();
        crate::color_analyzer_ui::save_book(&mut app, &book());
        open(&mut app).unwrap();
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1280.0, 800.0)).build_ui_state(
            |ui, app: &mut PhotocraftApp| {
                let ready = egui::Id::new("vectorizer-test-fonts");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ALL[0]);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    return;
                }
                show(app, ui.ctx());
            },
            app,
        );
        h.run_steps(3);
        assert!(h.query_by_label("EX-201 Bleu roi").is_some() && h.query_by_label("PDF Sheet…").is_some());
        let q = h.state().vectorizer.as_ref().unwrap().quantized.as_ref().unwrap();
        let blue = q.palette.iter().position(|c| vectorize::hex(c.rgb) == "#1F4FB5").unwrap();
        h.state_mut().vectorizer.as_mut().unwrap().changing = Some(blue);
        h.run_steps(3);
        h.get_by_label("Noir").click();
        h.run_steps(3);
        let v = h.state().vectorizer.as_ref().unwrap();
        assert!(v.changing.is_none());
        assert_eq!(v.matches()[blue].map(|m| m.0), Some(2));
        // Escape closes the module, not a picker that is no longer there.
        h.key_press(egui::Key::Escape);
        h.run_steps(2);
        assert!(h.state().vectorizer.is_none());
    }
}
