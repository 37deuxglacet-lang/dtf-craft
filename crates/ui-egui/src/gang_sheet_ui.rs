// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Gang Sheet…: a full-window dialog to lay visuals out on a DTF film.
//!
//! Left: the visuals (PNG files, or the open document flattened), each with its print width
//! (height follows, proportions locked), its copies and the resolution it prints at. Centre: the
//! sheet at the film width, as long as the longest sheet, with rulers in centimetres; visuals are
//! dragged with the mouse and never closer than the margin to another one or to an edge (a drop
//! that would be is undone). Right: the film, Auto Build (MaxRects, see
//! [`photocraft_algo::gang_sheet`]), the selected visual and the sheet's figures. Export PNG
//! runs `dtf.gangSheet` with the layout: the sheet is written at 300 ppi and opened as a new
//! document.
//!
//! Control channel: `ui.menu.invoke {"id":"dtf.gangSheet","params":{"ui":{"addFiles":[path…],
//! "addDocument":true, "set":{"filmWidth":cm,"maxLength":cm,"margin":mm,"rotate":bool},
//! "source":{"index":i,"widthCm":cm,"quantity":n}, "removeSource":i, "auto":true,
//! "move":{"index":i,"x":cm,"y":cm}, "rotate":i, "duplicate":i, "delete":i, "export":path,
//! "cancel":true}}}`; without `ui`, the params go to the command itself.

use std::sync::Mutex;

use egui::{Align2, Color32, FontId, Mesh, Pos2, Rect as ERect, Sense, Shape, Stroke, StrokeKind, TextureHandle, pos2, vec2};
use photocraft_algo::gang_sheet::{self as gs, DpiLevel, Item, Sheet, UNITS_PER_CM};
use photocraft_doc::DocId;
use photocraft_engine::gang_sheet_cmds::{self as cmd, Placed, Visual};
use photocraft_geom::Rect;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::file_dialog::{FileDialogAnswer, FileDialogRequest};
use crate::theme::Tokens;
use crate::widgets;

const LEFT_W: f32 = 330.0;
const RIGHT_W: f32 = 270.0;
const RULER: f32 = 22.0;
const FOOTER_H: f32 = 48.0;
/// Longest side of a visual's preview texture.
const THUMB: u32 = 640;
/// Arrow-key nudge: 1 mm, 1 cm with Shift (layout units).
const NUDGE: i64 = 10;
const NUDGE_SHIFT: i64 = 100;
/// The film is at most this long (cm), and starts at it.
const MAX_LENGTH_CM: f32 = 100.0;

/// Print-resolution ratings, as data colours (green, yellow, orange, red): the same in every theme.
fn level_color(level: DpiLevel) -> Color32 {
    match level {
        DpiLevel::Optimal => Color32::from_rgb(46, 170, 90),
        DpiLevel::Good => Color32::from_rgb(222, 190, 40),
        DpiLevel::Low => Color32::from_rgb(235, 130, 35),
        DpiLevel::Poor => Color32::from_rgb(214, 54, 54),
    }
}

/// Where a visual comes from: the command reloads it from there when the sheet is exported.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Origin {
    File(String),
    Document(DocId),
}

/// One visual of the list.
pub struct Source {
    origin: Origin,
    pub name: String,
    /// Its visible pixels (cropped) and the resolution its file records.
    pub px: (u32, u32),
    dpi: f64,
    /// Print width (layout units); the height follows.
    pub width: i64,
    /// Copies wanted on the sheet.
    pub quantity: usize,
    preview: Option<egui::ColorImage>,
    tex: Option<TextureHandle>,
}

impl Source {
    pub fn size(&self) -> (i64, i64) {
        gs::size_for_width(self.px.0, self.px.1, self.width)
    }

    pub fn dpi(&self) -> f64 {
        gs::effective_dpi(self.px.0, self.width)
    }
}

/// The film settings, remembered from one opening to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Film {
    width_cm: f32,
    max_length_cm: f32,
    margin_mm: f32,
    rotate: bool,
}

impl Default for Film {
    fn default() -> Self {
        Film {
            width_cm: cmd::DEFAULT_FILM_WIDTH_CM as f32,
            max_length_cm: cmd::DEFAULT_MAX_LENGTH_CM as f32,
            margin_mm: cmd::DEFAULT_MARGIN_MM as f32,
            rotate: true,
        }
    }
}

static LAST_FILM: Mutex<Option<Film>> = Mutex::new(None);

/// A visual being dragged: which one, where it was grabbed (units from its corner) and the last
/// position that kept the margins.
#[derive(Clone, Copy, Debug)]
struct Drag {
    index: usize,
    grab: (i64, i64),
    valid: (i64, i64),
}

pub struct GangSheetDialog {
    film: Film,
    pub sources: Vec<Source>,
    pub placed: Vec<Placed>,
    pub selected: Option<usize>,
    drag: Option<Drag>,
    /// 1 = the whole sheet fits; more zooms in.
    zoom: f32,
    /// Scroll offset of the sheet in the view (screen points).
    scroll: egui::Vec2,
    confirm_export: bool,
    pub status: String,
}

fn preview_image(v: &Visual) -> egui::ColorImage {
    let (w, h) = (v.width.max(1), v.height.max(1));
    let k = (f64::from(THUMB) / f64::from(w.max(h))).min(1.0);
    let src = Rect::new(0, 0, w as i32, h as i32);
    let (surf, r) = if k < 1.0 {
        let s = photocraft_algo::resample::resize_surface(&v.pixels, k, k, photocraft_algo::resample::Resample::BicubicAutomatic);
        (s, photocraft_algo::resample::scaled_rect(src, k, k))
    } else {
        (v.pixels.clone(), src)
    };
    let (pw, ph) = (r.width().max(1) as usize, r.height().max(1) as usize);
    let mut px = vec![[0u8; 4]; pw * ph];
    surf.read_rgba8_into(Rect::new(r.x0, r.y0, r.x0 + pw as i32, r.y0 + ph as i32), &mut px);
    egui::ColorImage::new([pw, ph], px.iter().map(|p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])).collect())
}

impl GangSheetDialog {
    fn new() -> Self {
        let film = LAST_FILM.lock().ok().and_then(|f| *f).unwrap_or_default();
        GangSheetDialog {
            film,
            sources: Vec::new(),
            placed: Vec::new(),
            selected: None,
            drag: None,
            zoom: 1.0,
            scroll: egui::Vec2::ZERO,
            confirm_export: false,
            status: String::new(),
        }
    }

    pub fn sheet(&self) -> Sheet {
        let cm = |v: f32| (f64::from(v) * UNITS_PER_CM as f64).round() as i64;
        Sheet::new(cm(self.film.width_cm), cm(self.film.max_length_cm), (f64::from(self.film.margin_mm) * 10.0).round() as i64)
    }

    fn size_of(&self, source: usize) -> (i64, i64) {
        self.sources.get(source).map_or((0, 0), Source::size)
    }

    pub fn item(&self, i: usize) -> Item {
        self.placed.get(i).map_or_else(Item::default, |p| {
            let (w, h) = self.size_of(p.source);
            cmd::footprint(p, w, h)
        })
    }

    pub fn items(&self) -> Vec<Item> {
        (0..self.placed.len()).map(|i| self.item(i)).collect()
    }

    pub fn conflicts(&self) -> Vec<bool> {
        self.sheet().conflicts(&self.items())
    }

    fn copies(&self, source: usize) -> usize {
        self.placed.iter().filter(|p| p.source == source).count()
    }

    /// Copies wanted that found no room on the sheet.
    pub fn unplaced(&self) -> usize {
        (0..self.sources.len()).map(|s| self.sources.get(s).map_or(0, |src| src.quantity).saturating_sub(self.copies(s))).sum()
    }

    /// Places the missing copies of `source` (or of every visual) where there is room.
    fn fill(&mut self, source: Option<usize>) {
        let sheet = self.sheet();
        let mut slots = Vec::new();
        for (s, src) in self.sources.iter().enumerate() {
            if source.is_none_or(|only| only == s) {
                let have = self.placed.iter().filter(|p| p.source == s).count();
                slots.extend(std::iter::repeat_n(s, src.quantity.saturating_sub(have)));
            }
        }
        if slots.is_empty() {
            return;
        }
        let sizes: Vec<(i64, i64)> = slots.iter().map(|s| self.size_of(*s)).collect();
        let spots = sheet.pack(&sizes, &self.items(), self.film.rotate);
        for (s, spot) in slots.iter().zip(spots) {
            if let Some(sp) = spot {
                self.placed.push(Placed { source: *s, x: sp.x, y: sp.y, rotated: sp.rotated });
            }
        }
    }

    /// Removes the copies of `source` beyond its quantity, the last placed first.
    fn trim(&mut self, source: usize) {
        let want = self.sources.get(source).map_or(0, |s| s.quantity);
        while self.copies(source) > want {
            if let Some(i) = self.placed.iter().rposition(|p| p.source == source) {
                self.placed.remove(i);
            }
        }
        self.selected = self.selected.filter(|i| *i < self.placed.len());
    }

    /// Auto Build: every copy laid out again, largest first.
    pub fn auto_build(&mut self) {
        self.placed.clear();
        self.selected = None;
        self.fill(None);
    }

    /// Every visual kept on the sheet (moved, not resized) after a size or film change.
    fn clamp_all(&mut self) {
        let sheet = self.sheet();
        for i in 0..self.placed.len() {
            let r = sheet.clamp(self.item(i));
            if let Some(p) = self.placed.get_mut(i) {
                (p.x, p.y) = (r.x, r.y);
            }
        }
    }

    fn set_width(&mut self, source: usize, width: i64) {
        let (iw, il) = self.sheet().inner();
        let Some(src) = self.sources.get_mut(source) else { return };
        // As tall as the sheet allows too: the longest side of a turned visual is its width.
        let fit_h = if src.px.1 > 0 { (il.max(1) as f64 * f64::from(src.px.0) / f64::from(src.px.1)).floor() as i64 } else { width };
        src.width = width.clamp(10, iw.max(10).min(fit_h.max(10)));
        self.clamp_all();
    }

    fn set_quantity(&mut self, source: usize, quantity: usize) {
        let Some(src) = self.sources.get_mut(source) else { return };
        src.quantity = quantity.min(999);
        self.trim(source);
        self.fill(Some(source));
    }

    pub(crate) fn add_visual(&mut self, origin: Origin, v: &Visual) {
        let sheet = self.sheet();
        let width = v.natural_width(&sheet);
        self.sources.push(Source { origin, name: v.name.clone(), px: (v.width, v.height), dpi: v.dpi, width, quantity: 1, preview: Some(preview_image(v)), tex: None });
        let s = self.sources.len() - 1;
        self.fill(Some(s));
        if self.copies(s) == 0 {
            self.status = crate::i18n::fmt(tl!("{n} not placed"), &[("n", "1")]);
        }
    }

    fn remove_source(&mut self, source: usize) {
        if source >= self.sources.len() {
            return;
        }
        self.placed.retain(|p| p.source != source);
        for p in &mut self.placed {
            if p.source > source {
                p.source -= 1;
            }
        }
        self.sources.remove(source);
        self.selected = None;
        self.drag = None;
    }

    /// Moves visual `i` to (`x`, `y`), kept on the sheet; refused (false) when it would come
    /// closer than the margin to another visual.
    pub fn move_to(&mut self, i: usize, x: i64, y: i64) -> bool {
        let sheet = self.sheet();
        let r = sheet.clamp(Item { x, y, ..self.item(i) });
        if sheet.collides(&self.items(), i, &r) {
            return false;
        }
        if let Some(p) = self.placed.get_mut(i) {
            (p.x, p.y) = (r.x, r.y);
        }
        true
    }

    /// Turns visual `i` by 90°, in place when there is room, else wherever there is.
    pub fn rotate(&mut self, i: usize) -> bool {
        let Some(old) = self.placed.get(i).copied() else { return false };
        let sheet = self.sheet();
        let (w, h) = self.size_of(old.source);
        let turned = Placed { rotated: !old.rotated, ..old };
        let r = sheet.clamp(cmd::footprint(&turned, w, h));
        let others: Vec<Item> = self.items().into_iter().enumerate().filter(|(j, _)| *j != i).map(|(_, r)| r).collect();
        if sheet.fits(r.w, r.h) && !sheet.collides(&others, usize::MAX, &r) {
            if let Some(p) = self.placed.get_mut(i) {
                *p = Placed { x: r.x, y: r.y, ..turned };
            }
            return true;
        }
        let (tw, th) = if turned.rotated { (h, w) } else { (w, h) };
        match sheet.pack(&[(tw, th)], &others, false).first().copied().flatten() {
            Some(sp) => {
                if let Some(p) = self.placed.get_mut(i) {
                    *p = Placed { x: sp.x, y: sp.y, ..turned };
                }
                true
            }
            None => false,
        }
    }

    /// One more copy of visual `i`'s source, where there is room.
    pub fn duplicate(&mut self, i: usize) -> bool {
        let Some(source) = self.placed.get(i).map(|p| p.source) else { return false };
        let before = self.placed.len();
        if let Some(src) = self.sources.get_mut(source) {
            src.quantity = (src.quantity + 1).min(999);
        }
        self.fill(Some(source));
        if self.placed.len() > before {
            self.selected = Some(self.placed.len() - 1);
            true
        } else {
            false
        }
    }

    /// Takes visual `i` off the sheet (one copy fewer).
    pub fn delete(&mut self, i: usize) {
        if i >= self.placed.len() {
            return;
        }
        let p = self.placed.remove(i);
        if let Some(src) = self.sources.get_mut(p.source) {
            src.quantity = src.quantity.saturating_sub(1);
        }
        self.selected = None;
        self.drag = None;
    }

    fn set_film(&mut self, film: Film) {
        if film == self.film {
            return;
        }
        self.film = film;
        if let Ok(mut last) = LAST_FILM.lock() {
            *last = Some(film);
        }
        // Visuals stay within the new film: no wider than it, moved onto it.
        for s in 0..self.sources.len() {
            let w = self.sources.get(s).map_or(0, |src| src.width);
            self.set_width(s, w);
        }
        self.clamp_all();
    }

    /// `dtf.gangSheet`'s params for this layout (`None` while a document visual is closed).
    pub fn params(&self, app: &PhotocraftApp) -> Result<Value, String> {
        let mut sources = Vec::new();
        for s in &self.sources {
            let mut v = match &s.origin {
                Origin::File(path) => json!({"path": path}),
                Origin::Document(id) => {
                    let i = app.session.documents().iter().position(|d| d.doc.id == *id).ok_or_else(|| format!("{}: {}", s.name, tl!("the document was closed")))?;
                    json!({"document": i})
                }
            };
            v["widthCm"] = json!(s.width as f64 / UNITS_PER_CM as f64);
            v["quantity"] = json!(s.quantity.max(1));
            sources.push(v);
        }
        let placements: Vec<Value> = self
            .placed
            .iter()
            .map(|p| json!({"source": p.source, "x": p.x as f64 / UNITS_PER_CM as f64, "y": p.y as f64 / UNITS_PER_CM as f64, "rotated": p.rotated}))
            .collect();
        Ok(json!({
            "sources": sources,
            "placements": placements,
            "filmWidth": self.film.width_cm,
            "maxLength": self.film.max_length_cm,
            "margin": self.film.margin_mm,
            "resolution": cmd::DEFAULT_RESOLUTION,
        }))
    }

    pub fn describe(&self) -> Value {
        let conflicts = self.conflicts();
        let sheet = self.sheet();
        json!({
            "filmWidth": self.film.width_cm,
            "maxLength": self.film.max_length_cm,
            "margin": self.film.margin_mm,
            "rotate": self.film.rotate,
            "sources": self.sources.iter().enumerate().map(|(i, s)| {
                let (w, h) = s.size();
                json!({"name": s.name, "pixels": [s.px.0, s.px.1], "widthCm": w as f64 / 100.0, "heightCm": h as f64 / 100.0,
                       "dpi": s.dpi().round(), "level": DpiLevel::of(s.dpi()), "quantity": s.quantity, "placed": self.copies(i)})
            }).collect::<Vec<_>>(),
            "placements": self.placed.iter().zip(&conflicts).map(|(p, c)| json!({"source": p.source, "x": p.x as f64 / 100.0, "y": p.y as f64 / 100.0, "rotated": p.rotated, "conflict": c})).collect::<Vec<_>>(),
            "selected": self.selected,
            "usedLengthCm": sheet.used_length(&self.items()) as f64 / 100.0,
            "conflicts": conflicts.iter().filter(|c| **c).count(),
            "unplaced": self.unplaced(),
        })
    }
}

/// Opens the dialog (kept as it is when already open).
pub fn open(app: &mut PhotocraftApp) {
    if app.gang_sheet.is_none() {
        app.gang_sheet = Some(GangSheetDialog::new());
    }
}

/// Adds PNG files from disk; failures go to the dialog's status line.
pub fn add_files(app: &mut PhotocraftApp, paths: &[String]) {
    let Some(d) = app.gang_sheet.as_mut() else { return };
    let mut errors = Vec::new();
    for path in paths {
        match cmd::load_file(path) {
            Ok(v) => d.add_visual(Origin::File(path.clone()), &v),
            Err(e) => errors.push(e.to_string()),
        }
    }
    if !errors.is_empty() {
        d.status = errors.join(" · ");
    }
}

/// Adds the active document, flattened.
pub fn add_document(app: &mut PhotocraftApp) -> Result<(), String> {
    let st = app.session.active().ok_or(tl!("Open a document first"))?;
    let v = cmd::load_document(&st.doc, &st.doc.name).map_err(|e| e.to_string())?;
    let id = st.doc.id;
    if let Some(d) = app.gang_sheet.as_mut() {
        d.add_visual(Origin::Document(id), &v);
    }
    Ok(())
}

/// Files dropped on the window while the dialog is open go onto the sheet.
pub fn drop_files(app: &mut PhotocraftApp, files: &[egui::DroppedFile]) {
    let paths: Vec<String> = files.iter().filter_map(|f| f.path.as_ref()).map(|p| p.to_string_lossy().into_owned()).collect();
    add_files(app, &paths);
}

fn pick_files(app: &mut PhotocraftApp) {
    let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
    let r = app.ask_file(FileDialogRequest::Open { multiple: true, initial_dir, extensions: Some(vec!["png".into()]) }, |app, answer| match answer {
        FileDialogAnswer::Paths(paths) => {
            add_files(app, &paths);
            Ok(Value::Null)
        }
        _ => Err("PNG files are added from a folder on this computer".to_string()),
    });
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
        && let Some(d) = app.gang_sheet.as_mut()
    {
        d.status = e;
    }
}

/// Runs the command, writing the sheet to `path` and opening it; the dialog closes on success.
fn export_to(app: &mut PhotocraftApp, path: &str) -> Result<Value, String> {
    let d = app.gang_sheet.as_ref().ok_or("Gang Sheet isn't open")?;
    let mut params = d.params(app)?;
    params["output"] = json!(path);
    match app.run(cmd::GANG_SHEET, params) {
        Ok(v) => {
            app.gang_sheet = None;
            Ok(v)
        }
        Err(e) => {
            if let Some(d) = app.gang_sheet.as_mut() {
                d.status = e.clone();
            }
            Err(e)
        }
    }
}

fn export(app: &mut PhotocraftApp) {
    let Some(d) = app.gang_sheet.as_ref() else { return };
    if d.placed.is_empty() {
        return;
    }
    if let Err(e) = d.params(app) {
        if let Some(d) = app.gang_sheet.as_mut() {
            d.status = e;
        }
        return;
    }
    let r = app.pick_save("Gang sheet.png", |app, path| export_to(app, &path));
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
        && let Some(d) = app.gang_sheet.as_mut()
    {
        d.status = e;
    }
}

fn close(app: &mut PhotocraftApp) {
    app.gang_sheet = None;
}

/// Menu / control routing. `Some` when this module handles the invocation.
pub fn menu(app: &mut PhotocraftApp, _ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != cmd::GANG_SHEET {
        return None;
    }
    if params.as_object().is_none_or(|o| o.is_empty()) {
        open(app);
        return Some(Ok(app.gang_sheet.as_ref().map(GangSheetDialog::describe).unwrap_or(Value::Null)));
    }
    let ui = params.get("ui")?;
    open(app);
    Some(control(app, ui))
}

fn control(app: &mut PhotocraftApp, ui: &Value) -> Result<Value, String> {
    let cm = |v: &Value| v.as_f64().filter(|f| f.is_finite()).map(|f| (f * UNITS_PER_CM as f64).round() as i64);
    if let Some(paths) = ui.get("addFiles").and_then(Value::as_array) {
        let paths: Vec<String> = paths.iter().filter_map(Value::as_str).map(str::to_string).collect();
        add_files(app, &paths);
    }
    if ui.get("addDocument").and_then(Value::as_bool) == Some(true) {
        add_document(app)?;
    }
    if ui.get("cancel").and_then(Value::as_bool) == Some(true) {
        close(app);
        return Ok(json!({"cancelled": true}));
    }
    if let Some(path) = ui.get("export").and_then(Value::as_str) {
        return export_to(app, path);
    }
    let d = app.gang_sheet.as_mut().ok_or("Gang Sheet isn't open")?;
    if let Some(set) = ui.get("set") {
        let mut film = d.film;
        let f = |k: &str, v: f32| set.get(k).and_then(Value::as_f64).filter(|f| f.is_finite()).map_or(v, |f| f as f32);
        film.width_cm = f("filmWidth", film.width_cm).clamp(10.0, 300.0);
        film.max_length_cm = f("maxLength", film.max_length_cm).clamp(10.0, MAX_LENGTH_CM);
        film.margin_mm = f("margin", film.margin_mm).clamp(0.0, 20.0);
        film.rotate = set.get("rotate").and_then(Value::as_bool).unwrap_or(film.rotate);
        d.set_film(film);
    }
    if let Some(s) = ui.get("source") {
        let i = s.get("index").and_then(Value::as_u64).map(|v| v as usize).filter(|i| *i < d.sources.len()).ok_or("`source` needs a valid index")?;
        if let Some(w) = s.get("widthCm").and_then(cm) {
            d.set_width(i, w);
        }
        if let Some(q) = s.get("quantity").and_then(Value::as_u64) {
            d.set_quantity(i, q as usize);
        }
    }
    if let Some(i) = ui.get("removeSource").and_then(Value::as_u64) {
        d.remove_source(i as usize);
    }
    if ui.get("auto").and_then(Value::as_bool) == Some(true) {
        d.auto_build();
    }
    if let Some(m) = ui.get("move") {
        let i = m.get("index").and_then(Value::as_u64).map(|v| v as usize).filter(|i| *i < d.placed.len()).ok_or("`move` needs a valid index")?;
        let (Some(x), Some(y)) = (m.get("x").and_then(cm), m.get("y").and_then(cm)) else { return Err("`move` needs x and y (cm)".into()) };
        if !d.move_to(i, x, y) {
            return Err(tl!("Too close to another visual").to_string());
        }
    }
    if let Some(i) = ui.get("rotate").and_then(Value::as_u64)
        && !d.rotate(i as usize)
    {
        return Err(tl!("No room to turn this visual").to_string());
    }
    if let Some(i) = ui.get("duplicate").and_then(Value::as_u64)
        && !d.duplicate(i as usize)
    {
        d.status = crate::i18n::fmt(tl!("{n} not placed"), &[("n", "1")]);
    }
    if let Some(i) = ui.get("delete").and_then(Value::as_u64) {
        d.delete(i as usize);
    }
    Ok(d.describe())
}

/// The image of a visual drawn into `r`, turned 90° clockwise when `rotated` (as the command
/// renders it).
fn image_shape(tex: egui::TextureId, r: ERect, rotated: bool) -> Shape {
    let mut m = Mesh::with_texture(tex);
    let uv = if rotated { [pos2(0.0, 1.0), pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0)] } else { [pos2(0.0, 0.0), pos2(1.0, 0.0), pos2(1.0, 1.0), pos2(0.0, 1.0)] };
    for (p, t) in [r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom()].into_iter().zip(uv) {
        m.vertices.push(egui::epaint::Vertex { pos: p, uv: t, color: Color32::WHITE });
    }
    m.add_triangle(0, 1, 2);
    m.add_triangle(0, 2, 3);
    Shape::mesh(m)
}

/// "300 ppi" in the UI language.
fn ppi_text(dpi: f64) -> String {
    crate::i18n::fmt(tl!("{dpi} ppi"), &[("dpi", &format!("{dpi:.0}"))])
}

fn cm_text(units: i64) -> String {
    widgets::fmt_num_prec(units as f64 / UNITS_PER_CM as f64, 1)
}

/// What the frame's widgets asked for, done once the dialog is no longer borrowed.
#[derive(Default)]
struct Actions {
    add_files: bool,
    add_document: bool,
    export: bool,
    cancel: bool,
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if app.gang_sheet.is_none() {
        return;
    }
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let has_doc = app.session.active().is_some();
    let mut act = Actions::default();
    egui::Area::new(egui::Id::new("gang-sheet-dialog")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let Some(d) = app.gang_sheet.as_mut() else { return };
        // Previews become textures here, where the context is.
        for (i, s) in d.sources.iter_mut().enumerate() {
            if let Some(img) = s.preview.take() {
                s.tex = Some(ctx.load_texture(format!("gang-sheet-{i}-{}", s.name), img, egui::TextureOptions::LINEAR));
            }
        }
        let (full, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        let painter = ui.painter().clone();
        painter.rect_filled(full, 0.0, t.chrome);
        let title = ERect::from_min_size(full.min, vec2(full.width(), 30.0));
        painter.rect_filled(title, 0.0, t.dock);
        painter.text(title.center(), Align2::CENTER_CENTER, tl!("Gang Sheet"), FontId::proportional(13.0), t.text);
        let body = ERect::from_min_max(pos2(full.left(), title.bottom()), pos2(full.right(), full.bottom() - FOOTER_H));
        let left = ERect::from_min_max(body.min, pos2(body.left() + LEFT_W, body.bottom()));
        let right = ERect::from_min_max(pos2(body.right() - RIGHT_W, body.top()), body.max);
        let view = ERect::from_min_max(pos2(left.right(), body.top()), pos2(right.left(), body.bottom())).shrink(12.0);
        sheet_view(ui, d, view, &t);
        visuals_panel(ui, d, left, &t, has_doc, &mut act);
        settings_panel(ui, d, right, &t);
        // Footer: the sheet's figures and the buttons.
        let foot = ERect::from_min_max(pos2(full.left(), full.bottom() - FOOTER_H), full.max);
        painter.rect_filled(foot, 0.0, t.dock);
        let mut fu = ui.new_child(egui::UiBuilder::new().max_rect(foot.shrink2(vec2(16.0, 9.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
        if let Some(role) = widgets::dialog_buttons(
            &mut fu,
            &[
                widgets::DialogButton::new(widgets::ButtonRole::Default, tl!("Export PNG…"), 120.0).enabled(!d.placed.is_empty()),
                widgets::DialogButton::new(widgets::ButtonRole::Cancel, tl!("Cancel"), 90.0),
            ],
        ) {
            if role == widgets::ButtonRole::Default {
                act.export = true;
            } else {
                act.cancel = true;
            }
        }
        if !d.status.is_empty() {
            fu.label(egui::RichText::new(&d.status).color(t.warning));
        }
    });
    // Keys: Delete removes the selected visual, arrows nudge it, Esc deselects or closes.
    if !ctx.text_edit_focused()
        && let Some(d) = app.gang_sheet.as_mut()
        && !d.confirm_export
    {
        keys(d, ctx, &mut act);
    }
    confirm_dialog(app, ctx, &mut act);
    if act.add_files {
        pick_files(app);
    }
    if act.add_document
        && let Err(e) = add_document(app)
        && let Some(d) = app.gang_sheet.as_mut()
    {
        d.status = e;
    }
    if act.export {
        // Visuals too close or off the film: ask first.
        let ask = app.gang_sheet.as_ref().is_some_and(|d| !d.confirm_export && d.conflicts().iter().any(|c| *c));
        if ask {
            if let Some(d) = app.gang_sheet.as_mut() {
                d.confirm_export = true;
            }
        } else {
            export(app);
        }
    }
    if act.cancel {
        close(app);
    }
}

fn keys(d: &mut GangSheetDialog, ctx: &egui::Context, act: &mut Actions) {
    use egui::Key;
    let (pressed, shift) = ctx.input(|i| {
        let p = |k: Key| i.key_pressed(k);
        ([p(Key::Delete) || p(Key::Backspace), p(Key::ArrowLeft), p(Key::ArrowRight), p(Key::ArrowUp), p(Key::ArrowDown), p(Key::Escape)], i.modifiers.shift)
    });
    let [del, left, right, up, down, esc] = pressed;
    if esc {
        if d.selected.is_some() {
            d.selected = None;
        } else {
            act.cancel = true;
        }
        return;
    }
    let Some(i) = d.selected else { return };
    if del {
        d.delete(i);
        return;
    }
    let step = if shift { NUDGE_SHIFT } else { NUDGE };
    let (dx, dy) = (i64::from(right) - i64::from(left), i64::from(down) - i64::from(up));
    if dx != 0 || dy != 0 {
        let r = d.item(i);
        if !d.move_to(i, r.x + dx * step, r.y + dy * step) {
            d.status = tl!("Too close to another visual").to_string();
        }
    }
}

fn confirm_dialog(app: &mut PhotocraftApp, ctx: &egui::Context, act: &mut Actions) {
    let Some(d) = app.gang_sheet.as_mut() else { return };
    if !d.confirm_export {
        return;
    }
    let n = d.conflicts().iter().filter(|c| **c).count();
    let mut answer = None;
    egui::Modal::new(egui::Id::new("gang-sheet-confirm")).show(ctx, |ui| {
        ui.set_max_width(380.0);
        ui.label(crate::i18n::fmt(tl!("{n} visuals are closer than the margin to another visual or to an edge. Export anyway?"), &[("n", &n.to_string())]));
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if let Some(role) = widgets::dialog_buttons(
                ui,
                &[widgets::DialogButton::new(widgets::ButtonRole::Default, tl!("Export"), 90.0), widgets::DialogButton::new(widgets::ButtonRole::Cancel, tl!("Cancel"), 90.0)],
            ) {
                answer = Some(role == widgets::ButtonRole::Default);
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        answer = Some(false);
    }
    if let Some(yes) = answer {
        d.confirm_export = false;
        act.export = false;
        if yes {
            export(app);
        }
    }
}

fn visuals_panel(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens, has_doc: bool, act: &mut Actions) {
    ui.painter().rect_filled(rect, 0.0, t.dock);
    ui.painter().line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, t.separator));
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 10.0))));
    panel.spacing_mut().item_spacing.y = 6.0;
    widgets::section_label(&mut panel, tl!("Visuals"));
    panel.horizontal(|ui| {
        if widgets::primary_button(ui, tl!("Add PNG Files…"), 140.0).clicked() {
            act.add_files = true;
        }
        if ui.add_enabled_ui(has_doc, |ui| widgets::secondary_button(ui, tl!("Add Open Document"), 140.0)).inner.clicked() {
            act.add_document = true;
        }
    });
    widgets::hairline(&mut panel);
    if d.sources.is_empty() {
        panel.label(egui::RichText::new(tl!("Add PNG files or the open document.")).color(t.text_faint));
        return;
    }
    let mut remove = None;
    let mut width_change = None;
    let mut quantity_change = None;
    egui::ScrollArea::vertical().id_salt("gang-sheet-visuals").auto_shrink([false, false]).show(&mut panel, |ui| {
        for (i, s) in d.sources.iter().enumerate() {
            let (w, h) = s.size();
            let dpi = s.dpi();
            let placed = d.placed.iter().filter(|p| p.source == i).count();
            ui.horizontal(|ui| {
                let (thumb, _) = ui.allocate_exact_size(vec2(64.0, 64.0), Sense::hover());
                widgets::checker(ui.painter(), thumb, 6.0);
                if let Some(tex) = &s.tex {
                    let k = (thumb.width() / s.px.0.max(1) as f32).min(thumb.height() / s.px.1.max(1) as f32);
                    let r = ERect::from_center_size(thumb.center(), vec2(s.px.0 as f32 * k, s.px.1 as f32 * k));
                    ui.painter().image(tex.id(), r, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
                }
                ui.vertical(|ui| {
                    ui.horizontal(|ui| {
                        widgets::elided_label(ui, &s.name, 150.0);
                        let level = DpiLevel::of(dpi);
                        ui.label(egui::RichText::new(format!("● {}", ppi_text(dpi))).color(level_color(level)).size(11.5));
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tl!("Width")).color(t.text_dim));
                        let mut v = (w as f64 / UNITS_PER_CM as f64) as f32;
                        if widgets::value_field_prec(ui, &mut v, 0.1..=300.0, "cm", 64.0, 1).changed() {
                            width_change = Some((i, (f64::from(v) * UNITS_PER_CM as f64).round() as i64));
                        }
                        ui.label(egui::RichText::new(format!("× {} cm", cm_text(h))).color(t.text_dim));
                    });
                    ui.horizontal(|ui| {
                        ui.label(egui::RichText::new(tl!("Copies")).color(t.text_dim));
                        let mut q = s.quantity as f32;
                        if widgets::value_field_prec(ui, &mut q, 0.0..=999.0, "", 44.0, 0).changed() {
                            quantity_change = Some((i, q.round().max(0.0) as usize));
                        }
                        if placed < s.quantity {
                            ui.label(egui::RichText::new(crate::i18n::fmt(tl!("{n} not placed"), &[("n", &(s.quantity - placed).to_string())])).color(t.warning).size(11.5));
                        }
                        if widgets::secondary_button(ui, tl!("Remove"), 60.0).clicked() {
                            remove = Some(i);
                        }
                    });
                });
            });
            widgets::hairline(ui);
        }
    });
    if let Some((i, w)) = width_change {
        d.set_width(i, w);
    }
    if let Some((i, q)) = quantity_change {
        d.set_quantity(i, q);
    }
    if let Some(i) = remove {
        d.remove_source(i);
    }
}

fn settings_panel(ui: &mut egui::Ui, d: &mut GangSheetDialog, rect: ERect, t: &Tokens) {
    ui.painter().rect_filled(rect, 0.0, t.dock);
    ui.painter().line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0, t.separator));
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 10.0))));
    panel.spacing_mut().item_spacing.y = 6.0;
    let mut film = d.film;
    widgets::section_label(&mut panel, tl!("Sheet"));
    let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(110.0, 20.0), egui::Label::new(egui::RichText::new(label).color(t.text_dim)));
            add(ui);
        });
    };
    row(&mut panel, tl!("Film Width"), &mut |ui| {
        widgets::value_field_prec(ui, &mut film.width_cm, 10.0..=300.0, "cm", 80.0, 1);
    });
    row(&mut panel, tl!("Max Length"), &mut |ui| {
        widgets::value_field_prec(ui, &mut film.max_length_cm, 10.0..=MAX_LENGTH_CM, "cm", 80.0, 1);
    });
    row(&mut panel, tl!("Margin"), &mut |ui| {
        widgets::value_field_prec(ui, &mut film.margin_mm, 0.0..=20.0, "mm", 80.0, 1);
    });
    widgets::checkbox(&mut panel, &mut film.rotate, tl!("Allow Rotation"));
    d.set_film(film);
    panel.horizontal(|ui| {
        if widgets::primary_button(ui, tl!("Auto Build"), 110.0).clicked() {
            d.auto_build();
            d.status.clear();
        }
        if widgets::secondary_button(ui, tl!("Clear All"), 90.0).clicked() {
            d.placed.clear();
            d.selected = None;
            for s in &mut d.sources {
                s.quantity = 0;
            }
        }
    });
    widgets::hairline(&mut panel);
    widgets::section_label(&mut panel, tl!("Selected Visual"));
    match d.selected.filter(|i| *i < d.placed.len()) {
        Some(i) => {
            let r = d.item(i);
            let name = d.placed.get(i).and_then(|p| d.sources.get(p.source)).map_or(String::new(), |s| s.name.clone());
            widgets::elided_label(&mut panel, &name, RIGHT_W - 30.0);
            panel.label(egui::RichText::new(format!("{} × {} cm  ·  X {} · Y {} cm", cm_text(r.w), cm_text(r.h), cm_text(r.x), cm_text(r.y))).color(t.text_dim).size(11.5));
            panel.horizontal(|ui| {
                if widgets::secondary_button(ui, tl!("Rotate 90°"), 80.0).clicked() && !d.rotate(i) {
                    d.status = tl!("No room to turn this visual").to_string();
                }
                if widgets::secondary_button(ui, tl!("Duplicate"), 80.0).clicked() && !d.duplicate(i) {
                    d.status = crate::i18n::fmt(tl!("{n} not placed"), &[("n", "1")]);
                }
                if widgets::secondary_button(ui, tl!("Delete"), 70.0).clicked() {
                    d.delete(i);
                }
            });
        }
        None => {
            panel.label(egui::RichText::new("—").color(t.text_faint));
        }
    }
    widgets::hairline(&mut panel);
    let items = d.items();
    let used = d.sheet().used_length(&items);
    let conflicts = d.conflicts().iter().filter(|c| **c).count();
    panel.label(crate::i18n::fmt(tl!("Length used: {length} cm"), &[("length", &cm_text(used))]));
    let unplaced = d.unplaced();
    if unplaced > 0 {
        panel.label(egui::RichText::new(crate::i18n::fmt(tl!("{n} not placed"), &[("n", &unplaced.to_string())])).color(t.warning));
    }
    if conflicts > 0 {
        panel.label(egui::RichText::new(crate::i18n::fmt(tl!("{n} too close or overlapping"), &[("n", &conflicts.to_string())])).color(t.danger));
    }
    widgets::hairline(&mut panel);
    widgets::section_label(&mut panel, tl!("Resolution"));
    for (level, sign, dpi) in [(DpiLevel::Optimal, "≥", 300.0), (DpiLevel::Good, "≥", 250.0), (DpiLevel::Low, "≥", 200.0), (DpiLevel::Poor, "<", 200.0)] {
        panel.label(egui::RichText::new(format!("●  {sign} {}", ppi_text(dpi))).color(level_color(level)).size(11.5));
    }
    widgets::hairline(&mut panel);
    panel.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Zoom")).color(t.text_dim));
        if widgets::secondary_button(ui, "−", 28.0).clicked() {
            d.zoom = (d.zoom / 1.25).max(1.0);
        }
        if widgets::secondary_button(ui, tl!("Fit on Screen"), 90.0).clicked() {
            d.zoom = 1.0;
            d.scroll = egui::Vec2::ZERO;
        }
        if widgets::secondary_button(ui, "+", 28.0).clicked() {
            d.zoom = (d.zoom * 1.25).min(12.0);
        }
    });
    panel.label(
        egui::RichText::new(tl!("Drag a visual to move it. Arrows: 1 mm (Shift: 1 cm). Delete takes it off the sheet."))
            .color(t.text_faint)
            .size(11.0),
    );
}

/// The sheet: rulers, film, visuals with their margins, selection and dragging.
fn sheet_view(ui: &mut egui::Ui, d: &mut GangSheetDialog, view: ERect, t: &Tokens) {
    let sheet = d.sheet();
    let painter = ui.painter_at(view);
    painter.rect_filled(view, 0.0, t.canvas);
    let area = ERect::from_min_max(view.min + vec2(RULER, RULER), view.max);
    let (sw, sl) = (sheet.width.max(1) as f32, sheet.max_length.max(1) as f32);
    let fit = ((area.width() - 16.0) / sw).min((area.height() - 16.0) / sl).max(1e-4);
    let resp = ui.interact(area, ui.id().with("gang-sheet-view"), Sense::click_and_drag());
    // Ctrl + wheel (or a pinch) zooms about the pointer; the wheel scrolls.
    if resp.hovered() {
        let (zoom_delta, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        if (zoom_delta - 1.0).abs() > 1e-4 {
            let old = fit * d.zoom;
            d.zoom = (d.zoom * zoom_delta).clamp(1.0, 12.0);
            if let Some(p) = resp.hover_pos() {
                let k = fit * d.zoom / old;
                let origin = area.min + d.scroll;
                d.scroll = (p - (p - origin) * k) - area.min;
            }
        } else {
            d.scroll += scroll;
        }
    }
    let s = fit * d.zoom;
    let size = vec2(sw * s, sl * s);
    // Keep the sheet in view: centred while it fits, else scrolled no further than its edges.
    let clamp_axis = |off: f32, content: f32, avail: f32| if content + 16.0 <= avail { (avail - content) / 2.0 } else { off.clamp(avail - content - 8.0, 8.0) };
    d.scroll = vec2(clamp_axis(d.scroll.x, size.x, area.width()), clamp_axis(d.scroll.y, size.y, area.height()));
    let origin = area.min + d.scroll;
    let to_screen = |x: i64, y: i64| pos2(origin.x + x as f32 * s, origin.y + y as f32 * s);
    let to_units = |p: Pos2| (((p.x - origin.x) / s).round() as i64, ((p.y - origin.y) / s).round() as i64);
    let film_r = ERect::from_min_size(origin, size);
    let clip = painter.with_clip_rect(area);
    // The film: transparent (checker), its margin band, the length in use.
    widgets::checker(&clip, film_r, 8.0);
    clip.rect_stroke(film_r, 0.0, Stroke::new(1.0, t.separator), StrokeKind::Outside);
    let m = sheet.margin as f32 * s;
    if m >= 2.0 {
        clip.rect_stroke(film_r.shrink(m), 0.0, Stroke::new(1.0, t.text_faint.gamma_multiply(0.5)), StrokeKind::Inside);
    }
    let items = d.items();
    let conflicts = sheet.conflicts(&items);
    let used = sheet.used_length(&items);
    if used > 0 {
        let y = origin.y + used as f32 * s;
        let dash = Shape::dashed_line(&[pos2(film_r.left(), y), pos2(film_r.right(), y)], Stroke::new(1.5, t.accent), 6.0, 4.0);
        clip.extend(dash);
        clip.text(pos2(film_r.right() - 4.0, y - 3.0), Align2::RIGHT_BOTTOM, format!("{} cm", cm_text(used)), FontId::proportional(11.0), t.accent);
    }
    // Pointer: pick, drag, drop.
    let hit = |p: Pos2| -> Option<usize> {
        let (ux, uy) = to_units(p);
        items.iter().rposition(|r| ux >= r.x && ux < r.x + r.w && uy >= r.y && uy < r.y + r.h)
    };
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        match hit(p) {
            Some(i) => {
                let r = items.get(i).copied().unwrap_or_default();
                let (ux, uy) = to_units(p);
                d.selected = Some(i);
                d.drag = Some(Drag { index: i, grab: (ux - r.x, uy - r.y), valid: (r.x, r.y) });
            }
            None => d.selected = None,
        }
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        d.selected = hit(p);
    }
    if let Some(dr) = d.drag
        && let Some(p) = resp.interact_pointer_pos()
        && resp.dragged()
    {
        let (ux, uy) = to_units(p);
        // Whole millimetres, kept on the film.
        let snap = |v: i64| (v as f64 / 10.0).round() as i64 * 10;
        let r = sheet.clamp(Item { x: snap(ux - dr.grab.0), y: snap(uy - dr.grab.1), ..d.item(dr.index) });
        let ok = !sheet.collides(&d.items(), dr.index, &r);
        if let Some(pl) = d.placed.get_mut(dr.index) {
            (pl.x, pl.y) = (r.x, r.y);
        }
        if ok && let Some(drag) = d.drag.as_mut() {
            drag.valid = (r.x, r.y);
        }
    }
    if resp.drag_stopped()
        && let Some(dr) = d.drag.take()
    {
        let r = d.item(dr.index);
        if sheet.collides(&d.items(), dr.index, &r)
            && let Some(pl) = d.placed.get_mut(dr.index)
        {
            // Dropped too close to another visual: back to the last place that kept the margin.
            (pl.x, pl.y) = dr.valid;
            d.status = tl!("Too close to another visual").to_string();
        }
    }
    // Visuals (positions re-read: the drag may have moved one).
    let items = d.items();
    let dragging = d.drag.map(|dr| dr.index);
    for (i, (r, p)) in items.iter().zip(&d.placed).enumerate() {
        let rect = ERect::from_min_max(to_screen(r.x, r.y), to_screen(r.x + r.w, r.y + r.h));
        if !rect.intersects(area) {
            continue;
        }
        let bad = conflicts.get(i).copied().unwrap_or(false) || (dragging == Some(i) && sheet.collides(&items, i, r));
        let zone = rect.expand(m);
        if bad {
            clip.rect_filled(zone, 0.0, t.danger.gamma_multiply(0.18));
        }
        clip.rect_stroke(zone, 0.0, Stroke::new(1.0, if bad { t.danger } else { t.text_faint.gamma_multiply(0.6) }), StrokeKind::Inside);
        if let Some(tex) = d.sources.get(p.source).and_then(|s| s.tex.as_ref()) {
            clip.add(image_shape(tex.id(), rect, p.rotated));
        }
        if d.selected == Some(i) {
            clip.rect_stroke(rect, 0.0, Stroke::new(2.0, t.accent), StrokeKind::Outside);
        }
        // The visual's print resolution, at its corner.
        if let Some(src) = d.sources.get(p.source) {
            clip.circle_filled(rect.left_top() + vec2(6.0, 6.0), 4.0, level_color(DpiLevel::of(src.dpi())));
        }
    }
    // Rulers in centimetres.
    let top = ERect::from_min_max(pos2(area.left(), view.top()), pos2(view.right(), area.top()));
    let side = ERect::from_min_max(pos2(view.left(), area.top()), pos2(area.left(), view.bottom()));
    painter.rect_filled(top, 0.0, t.dock);
    painter.rect_filled(side, 0.0, t.dock);
    painter.rect_filled(ERect::from_min_max(view.min, area.min), 0.0, t.dock);
    painter.text(view.min + vec2(RULER / 2.0, RULER / 2.0), Align2::CENTER_CENTER, "cm", FontId::proportional(9.5), t.text_faint);
    let px_per_cm = UNITS_PER_CM as f32 * s;
    let label_every = [1, 2, 5, 10, 20, 50].into_iter().find(|n| *n as f32 * px_per_cm >= 34.0).unwrap_or(100);
    let tick_every = if px_per_cm >= 6.0 { 1 } else { label_every };
    let top_p = painter.with_clip_rect(top);
    let side_p = painter.with_clip_rect(side);
    for c in (0..=(sheet.width / UNITS_PER_CM)).step_by(tick_every as usize) {
        let x = origin.x + c as f32 * px_per_cm;
        let long = c % label_every == 0;
        top_p.line_segment([pos2(x, top.bottom()), pos2(x, top.bottom() - if long { 9.0 } else { 4.0 })], Stroke::new(1.0, t.text_faint));
        if long {
            top_p.text(pos2(x + 2.0, top.top() + 1.0), Align2::LEFT_TOP, c.to_string(), FontId::proportional(9.5), t.text_dim);
        }
    }
    for c in (0..=(sheet.max_length / UNITS_PER_CM)).step_by(tick_every as usize) {
        let y = origin.y + c as f32 * px_per_cm;
        let long = c % label_every == 0;
        side_p.line_segment([pos2(side.right(), y), pos2(side.right() - if long { 9.0 } else { 4.0 }, y)], Stroke::new(1.0, t.text_faint));
        if long {
            side_p.text(pos2(side.left() + 1.0, y + 2.0), Align2::LEFT_TOP, c.to_string(), FontId::proportional(9.5), t.text_dim);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png(dir: &std::path::Path, name: &str, w: u32, h: u32) -> String {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": w, "height": h, "resolution": 300, "background": "transparent"})).unwrap();
        s.execute("edit.fill", json!({"color": "#c03020"})).unwrap();
        let path = dir.join(name).to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": path})).unwrap();
        path
    }

    fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, ui: Value) -> Value {
        menu(app, ctx, cmd::GANG_SHEET, &json!({"ui": ui})).unwrap().unwrap()
    }

    #[test]
    fn layout_keeps_margins_and_exports_a_document() {
        let dir = std::env::temp_dir().join(format!("gang-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // 1181 px at 300 ppi: 10 cm.
        let a = png(&dir, "a.png", 1181, 591);
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        let r = menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({})).unwrap().unwrap();
        assert_eq!(r["filmWidth"], 56.0);
        assert_eq!(r["margin"], 5.0);
        let r = invoke(&mut app, &ctx, json!({"addFiles": [a]}));
        assert_eq!(r["sources"][0]["widthCm"], 10.0);
        assert_eq!(r["sources"][0]["level"], "optimal");
        assert_eq!(r["placements"][0]["x"], 0.5, "placed inside the margin");
        // Five copies, then twice as wide: the resolution drops to 150 ppi.
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "quantity": 5}}));
        assert_eq!(r["placements"].as_array().unwrap().len(), 5);
        assert_eq!(r["conflicts"], 0);
        let r = invoke(&mut app, &ctx, json!({"source": {"index": 0, "widthCm": 20}, "auto": true}));
        assert_eq!(r["sources"][0]["level"], "poor");
        assert_eq!(r["conflicts"], 0);
        assert_eq!(r["unplaced"], 0);
        // A move closer than 5 mm to another visual is refused; one with room is kept.
        let first = r["placements"][0].clone();
        let second = r["placements"][1].clone();
        let too_close = json!({"move": {"index": 1, "x": first["x"], "y": first["y"]}});
        assert!(menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({"ui": too_close})).unwrap().is_err());
        let r = invoke(&mut app, &ctx, json!({}));
        assert_eq!(r["placements"][1], second);
        // Rotate, duplicate, delete.
        let r = invoke(&mut app, &ctx, json!({"rotate": 0}));
        assert_eq!(r["placements"][0]["rotated"], true);
        assert_eq!(r["conflicts"], 0);
        let r = invoke(&mut app, &ctx, json!({"duplicate": 0}));
        assert_eq!(r["sources"][0]["quantity"], 6);
        let r = invoke(&mut app, &ctx, json!({"delete": 5}));
        assert_eq!(r["sources"][0]["quantity"], 5);
        // A narrower film keeps every visual on it.
        let r = invoke(&mut app, &ctx, json!({"set": {"filmWidth": 30}}));
        for p in r["placements"].as_array().unwrap() {
            assert!(p["x"].as_f64().unwrap() >= 0.5);
        }
        invoke(&mut app, &ctx, json!({"set": {"filmWidth": 56}, "auto": true}));
        let out = dir.join("sheet.png").to_string_lossy().into_owned();
        let r = invoke(&mut app, &ctx, json!({"export": out}));
        assert_eq!(r["images"], 5);
        assert!(app.gang_sheet.is_none(), "closed once exported");
        assert_eq!(app.session.documents().len(), 1);
        assert!(std::path::Path::new(&out).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn open_document_is_added_flattened_and_params_go_to_the_command() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        app.run("file.new", json!({"width": 600, "height": 300, "resolution": 300})).unwrap();
        let r = invoke(&mut app, &ctx, json!({"addDocument": true}));
        assert_eq!(r["sources"][0]["pixels"], json!([600, 300]));
        let d = app.gang_sheet.as_ref().unwrap();
        let p = d.params(&app).unwrap();
        assert_eq!(p["sources"][0]["document"], 0);
        assert_eq!(p["placements"].as_array().unwrap().len(), 1);
        invoke(&mut app, &ctx, json!({"cancel": true}));
        assert!(app.gang_sheet.is_none());
        // Params other than `ui` run the command itself.
        assert!(menu(&mut app, &ctx, cmd::GANG_SHEET, &json!({"sources": [{"document": 0}]})).is_none());
    }

    #[test]
    fn the_dialog_draws() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        app.run("file.new", json!({"width": 400, "height": 200, "resolution": 300})).unwrap();
        invoke(&mut app, &ctx, json!({"addDocument": true, "source": {"index": 0, "quantity": 3}}));
        let _ = ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx()));
        assert!(app.gang_sheet.as_ref().is_some_and(|d| d.sources[0].tex.is_some()));
    }
}
