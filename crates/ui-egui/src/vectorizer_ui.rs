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

use std::collections::HashSet;

use egui::{Align2, Color32, FontId, Rect, RichText, Sense, Stroke, pos2, vec2};
use photocraft_algo::vectorize::{self, Detail, Kind, Quantized, Smoothing, SvgSize, TraceOptions, Traced};
use photocraft_engine::dtf_vector_cmds as cmds;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

const PANEL_W: f32 = 420.0;
const SETTINGS_PREF: &str = "dtfCraft.vectorizer";
/// The vectorized preview's longest side, in px.
const PREVIEW_SIDE: f32 = 1600.0;
/// The contours drawn by "Show Contours", in preview px.
const CONTOUR_PX: f32 = 1.2;

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
    revision: u64,
    original: Option<egui::TextureHandle>,
    vectorized: Option<Tex>,
    contours: Option<Tex>,
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

/// `12,5` in French, `12.5` in English: one decimal, trailing zero dropped.
fn decimal(v: f64) -> String {
    crate::color_sheet::decimal(v, 1)
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
            revision: 0,
            original: None,
            vectorized: None,
            contours: None,
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
        json!({"colors": s.colors, "kind": kind, "detail": detail, "smoothing": smoothing, "removeBackground": s.remove_background, "exclude": exclude})
    }

    /// The SVG as it is exported (print size in mm).
    pub fn svg(&self) -> Option<String> {
        let (q, t) = (self.quantized.as_ref()?, self.traced.as_ref()?);
        Some(vectorize::svg(t, &q.palette, SvgSize::Mm(self.size_mm.0, self.size_mm.1), None))
    }

    /// The scale of the vectorized preview against the traced copy.
    fn preview_scale(&self) -> f32 {
        (PREVIEW_SIDE / self.w.max(self.h).max(1) as f32).min(4.0)
    }

    /// The traced shapes drawn at preview size: filled, or their contours only.
    fn render(&self, contours: bool) -> Option<egui::ColorImage> {
        let (q, t) = (self.quantized.as_ref()?, self.traced.as_ref()?);
        let k = self.preview_scale();
        let outline = contours.then(|| f64::from(CONTOUR_PX / k));
        let svg = vectorize::svg(t, &q.palette, SvgSize::Px(self.w as f64, self.h as f64), outline);
        let buf = photocraft_io::svg::rasterize(svg.as_bytes(), k).ok()?;
        let (w, h) = (buf.rect.width() as usize, buf.rect.height() as usize);
        (w * h == buf.px.len() && w > 0 && h > 0).then(|| egui::ColorImage::from_rgba_unmultiplied([w, h], &rgba8(&buf.px)))
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
    let v = Vectorizer::new(doc, &name, path, (w, h), dpi, mm, mode, &px, load_settings(app));
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

// ---------------------------------------------------------------- drawing

enum Action {
    Close,
    Export,
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
        && (v.requantize || v.retrace)
    {
        if ctx.input(|i| i.pointer.any_down()) {
            ctx.request_repaint();
        } else {
            v.update();
            ctx.request_repaint();
        }
    }
    let escape = app.file_dialog.is_none() && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
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
    let info = format!(
        "{} · {} · {} × {} px · {} ppi · {}",
        v.name,
        v.mode,
        v.size.0,
        v.size.1,
        decimal(v.dpi),
        tl!("the document is not modified")
    );
    painter.text(pos2(title.left() + 32.0 + title_w, title.center().y), Align2::LEFT_CENTER, info, FontId::proportional(12.5), t.text_dim);
    let footer_h = 56.0;
    let body = Rect::from_min_max(pos2(full.left(), title.bottom()), pos2(full.right(), full.bottom() - footer_h));
    let right = Rect::from_min_max(pos2(body.right() - PANEL_W, body.top()), body.max);
    let left = Rect::from_min_max(body.min, pos2(right.left(), body.bottom()));
    painter.rect_filled(right, 0.0, t.dock);
    painter.line_segment([right.left_top(), right.left_bottom()], Stroke::new(1.0, t.separator));
    preview(ui, v, left);
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(right.shrink2(vec2(0.0, 8.0))));
    egui::ScrollArea::vertical().id_salt("dtf-vectorizer-panel").auto_shrink([false, false]).show(&mut panel, |ui| {
        ui.add_space(4.0);
        egui::Frame::NONE.inner_margin(egui::Margin::symmetric(16, 0)).show(ui, |ui| {
            ui.set_width(PANEL_W - 40.0);
            settings_ui(ui, v);
        });
    });
    // Footer.
    let mut action = None;
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
        (Some(tr), _) => format!(
            "{}   ·   {}",
            crate::i18n::fmt(
                tl!("{colors} colors · {shapes} shapes · {points} points"),
                &[("colors", &tr.paths.len().to_string()), ("shapes", &tr.shapes().to_string()), ("points", &tr.points().to_string())]
            ),
            tl!("Cut-out shapes, no overlap")
        ),
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
    let frames: Vec<(Rect, bool)> = match v.view {
        View::Original => vec![(fit(inner), false)],
        View::Vectorized => vec![(fit(inner), true)],
        View::SideBySide => {
            let half = (inner.width() - 24.0) / 2.0;
            let l = Rect::from_min_size(inner.min + vec2(0.0, 22.0), vec2(half, inner.height() - 22.0));
            let r = Rect::from_min_size(pos2(inner.right() - half, inner.top() + 22.0), vec2(half, inner.height() - 22.0));
            vec![(fit(l), false), (fit(r), true)]
        }
    };
    let uv = Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0));
    for (rect, vectorized) in frames {
        if v.view == View::SideBySide {
            let (label, detail) = if vectorized {
                (tl!("Vectorized"), format!("SVG · {} × {} mm", decimal(v.size_mm.0), decimal(v.size_mm.1)))
            } else {
                (tl!("Original"), format!("{} · {} × {} px", file_kind(&v.name), v.size.0, v.size.1))
            };
            let y = rect.top() - 10.0;
            ui.painter().text(pos2(rect.left(), y), Align2::LEFT_BOTTOM, label, crate::theme::semibold(12.5), t.text);
            ui.painter().text(pos2(rect.right(), y), Align2::RIGHT_BOTTOM, detail, crate::theme::mono(11.0), t.text_faint);
        }
        if v.settings.checker {
            crate::widgets::checker(ui.painter(), rect, 8.0);
        } else {
            ui.painter().rect_filled(rect, 0.0, Color32::WHITE);
        }
        let tex = if vectorized { v.result_texture(ui.ctx(), false) } else { Some(v.original_texture(ui.ctx())) };
        if let Some(tex) = tex {
            ui.painter().image(tex.id(), rect, uv, Color32::WHITE);
        }
        if v.settings.contours
            && (vectorized || v.view == View::Original)
            && let Some(tex) = v.result_texture(ui.ctx(), true)
        {
            ui.painter().image(tex.id(), rect, uv, Color32::WHITE);
        }
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

/// The settings panel: source, settings, palette, export.
fn settings_ui(ui: &mut egui::Ui, v: &mut Vectorizer) {
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
        if choice(ui, &mut v.settings.detail, &[(Detail::Low, tl!("Low")), (Detail::Medium, tl!("Medium")), (Detail::High, tl!("High"))]) {
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
    v.redo(redo);
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
}

/// The palette: each colour with its share, the background marked; unchecked colours are left out
/// of the SVG.
fn palette_ui(ui: &mut egui::Ui, v: &mut Vectorizer, section: &dyn Fn(&mut egui::Ui, &str)) {
    let t = Tokens::get(ui.ctx());
    let Some(q) = v.quantized.as_ref() else { return };
    let in_svg = v.traced.as_ref().map_or(0, |tr| tr.paths.len());
    section(
        ui,
        &format!(
            "{} · {}",
            tl!("Palette"),
            crate::i18n::fmt(tl!("{n} colors · {k} in the SVG"), &[("n", &q.palette.len().to_string()), ("k", &in_svg.to_string())])
        ),
    );
    let mut toggled: Option<(String, bool)> = None;
    egui::Grid::new("dtf-vectorizer-palette").num_columns(4).spacing(vec2(8.0, 5.0)).show(ui, |ui| {
        ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
        ui.label(RichText::new("SVG").small().color(t.text_dim));
        ui.label(RichText::new(tl!("Color")).small().color(t.text_dim));
        ui.label(RichText::new(tl!("Area")).small().color(t.text_dim));
        ui.label("");
        ui.end_row();
        for (i, c) in q.palette.iter().enumerate() {
            let hex = vectorize::hex(c.rgb);
            let mut keep = !v.excluded.contains(&hex);
            let check = ui.add(egui::Checkbox::without_text(&mut keep)).on_hover_text(crate::i18n::fmt(tl!("Keep {hex} in the SVG"), &[("hex", &hex)]));
            if check.changed() {
                toggled = Some((hex.clone(), !keep));
            }
            ui.horizontal(|ui| {
                patch(ui, c.rgb, 22.0);
                ui.label(RichText::new(&hex).font(crate::theme::mono(11.5)));
                ui.label(RichText::new(format!("{} · {} · {}", c.rgb[0], c.rgb[1], c.rgb[2])).font(crate::theme::mono(10.0)).color(t.text_faint));
            });
            ui.label(RichText::new(format!("{} %", decimal(f64::from(c.share) * 100.0))).font(crate::theme::mono(11.5)));
            if q.background == Some(i) {
                ui.label(RichText::new(tl!("Edge Background")).small().color(t.warning));
            } else {
                ui.label("");
            }
            ui.end_row();
        }
    });
    ui.add_space(4.0);
    ui.label(RichText::new(tl!("Unchecked: the color is left out of the SVG; its shapes are removed, not merged.")).small().color(t.text_faint));
    if let Some((hex, excluded)) = toggled {
        v.set_excluded(&hex, excluded);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui_kittest::kittest::Queryable;

    const BLUE: [f32; 3] = [31.0 / 255.0, 79.0 / 255.0, 181.0 / 255.0];
    const ORANGE: [f32; 3] = [228.0 / 255.0, 87.0 / 255.0, 46.0 / 255.0];

    /// An app with a 60 × 40 visual at 254 ppi on white: a blue square with a white dot, an
    /// orange square; and the files written.
    fn app() -> (PhotocraftApp, std::rc::Rc<std::cell::RefCell<Vec<(String, Vec<u8>)>>>) {
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
        assert_eq!(q.palette.iter().map(|c| vectorize::hex(c.rgb)).collect::<Vec<_>>(), ["#FFFFFF", "#1F4FB5", "#E4572E"]);
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
}
