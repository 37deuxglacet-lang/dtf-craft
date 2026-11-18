// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Mockup (beta)…: the customer's visuals shown on a photo of the garment, at their real
//! print size.
//!
//! * Header: the DTF Craft logo, "DTF Mockup" and its BETA badge, then Report a Bug, Cancel and
//!   Export….
//! * Left column, in three numbered steps: **Product Photo** (a PNG or JPG of the garment),
//!   **Scale** (a line drawn on the photo along something whose length is known, the chest width
//!   say, and that length in cm: the photo's pixels per cm, and rulers in cm round it) and
//!   **Visuals** (PNG or SVG files, or the open document; each arrives at its print size, its
//!   pixels at its resolution). Visuals wait for the scale: without it their size means nothing.
//! * Canvas: the photo with the visuals on it as they would print (fabric shadows and folds), the
//!   reference line (its ends can be dragged) and the rulers. A click picks a visual, a drag moves
//!   it; its size in cm is written under it.
//! * Right column, in folding sections: **Selected Visual** (width and height in cm, proportions
//!   kept or not, rotation, centre on the reference line, delete), **Rendering** (fabric shadows,
//!   folds, ink opacity) and **Export** (PNG or JPG), folded.
//! * Status bar: the scale, the photo's size in cm, the visuals, the last message.
//!
//! The sizes are the mockup's only: the visuals' files are not changed. Export renders the mockup
//! at the photo's resolution through `dtf.mockup` (the same render as the preview, see
//! [`photocraft_engine::mockup_cmds::composite`]), writes it and opens it as a new document. In
//! beta: no project is saved, the export is the result.
//!
//! The window is light, as Gang Sheet's ([`crate::gang_sheet_ui::light_scope`]).
//!
//! Control channel: `ui.menu.invoke {"id":"dtf.mockup","params":{"ui":{…}}}` with `photo` (a
//! path), `scale` (`from`, `to`: photo pixels, `cm`), `draw` (arm the line), `addFiles`,
//! `addDocument`, `select` (index or `null`), `visual` (`index`, `x`, `y`, `widthCm`, `heightCm`,
//! `lock`, `rotation`), `center`, `remove` (index), `render` (`shadows`, `folds`, `opacity`),
//! `format` (`png`/`jpg`), `export` (path), `cancel`.

use egui::{Align2, Color32, FontId, Pos2, Rect as ERect, Sense, Shape, Stroke, StrokeKind, TextureHandle, pos2, vec2};
use photocraft_doc::DocId;
use photocraft_engine::gang_sheet_cmds::{self as gang, Visual};
use photocraft_engine::mockup_cmds::{self as cmd, Light, Photo, Print, Render};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::file_dialog::{FileDialogAnswer, FileDialogRequest};
use crate::gang_sheet_ui::{BLUE, MAGENTA, blue_button, card_frame, light_scope, number_row, preview_image, switch_row};
use crate::theme::Tokens;
use crate::widgets;

const HEADER_H: f32 = 52.0;
const STATUS_H: f32 = 30.0;
const LEFT_W: f32 = 320.0;
const RIGHT_W: f32 = 320.0;
const RULER: f32 = 20.0;
/// The preview is rendered on the photo reduced to this many pixels on its longest side.
const PREVIEW_SIDE: u32 = 1400;
/// Most visuals on a mockup.
const MAX_ITEMS: usize = 50;
/// Where a new visual lands, after the previous one: this far right and down (cm).
const CASCADE_CM: f64 = 2.0;
/// An end of the reference line is grabbed this close (screen points).
const GRAB: f32 = 10.0;
const GREEN: Color32 = Color32::from_rgb(22, 140, 72);

const SECTION_VISUAL: usize = 0;
const SECTION_RENDER: usize = 1;
const SECTION_EXPORT: usize = 2;

// ---------------------------------------------------------------- state

/// Where a visual comes from: the export reloads it from there.
#[derive(Clone, Debug, PartialEq)]
enum Origin {
    File(String),
    Document(DocId),
}

/// One visual on the photo.
struct Item {
    origin: Origin,
    visual: Visual,
    width_cm: f32,
    height_cm: f32,
    lock: bool,
    /// Degrees, clockwise.
    rotation: f32,
    /// Its centre, in the photo's pixels.
    center: [f64; 2],
    thumb: Option<egui::ColorImage>,
    tex: Option<TextureHandle>,
    /// Its pixels at the preview's scale, and their size.
    cache: Option<(usize, usize, Vec<u8>)>,
}

/// The loaded photo: its file and size, the preview's reduced copy and its light.
struct PhotoState {
    path: String,
    name: String,
    width: u32,
    height: u32,
    preview: Photo,
    /// The preview's pixels per photo pixel.
    k: f64,
    light: Light,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    /// Drawing the reference line from its first end.
    Line,
    /// Moving an end of the line.
    End(usize),
    /// Moving a visual, grabbed this far from its centre (photo pixels).
    Move { index: usize, grab: [f64; 2] },
}

pub struct MockupDialog {
    photo: Option<PhotoState>,
    /// The reference line's ends, in photo pixels.
    line: Option<[[f64; 2]; 2]>,
    /// Its real length.
    length_cm: f32,
    /// The next drag on the photo draws the line.
    drawing: bool,
    items: Vec<Item>,
    selected: Option<usize>,
    shadows: f32,
    folds: f32,
    opacity: f32,
    jpg: bool,
    closed: [bool; 3],
    status: String,
    drag: Option<Drag>,
    /// The preview needs rendering again.
    dirty: bool,
    shown: Option<TextureHandle>,
}

impl MockupDialog {
    fn new() -> Self {
        MockupDialog {
            photo: None,
            line: None,
            length_cm: 50.0,
            drawing: false,
            items: Vec::new(),
            selected: None,
            shadows: cmd::DEFAULT_SHADOWS as f32,
            folds: cmd::DEFAULT_FOLDS as f32,
            opacity: cmd::DEFAULT_OPACITY as f32,
            jpg: false,
            closed: [false, false, true],
            status: String::new(),
            drag: None,
            dirty: true,
            shown: None,
        }
    }

    /// The photo's pixels per cm, once the line and its length are given.
    fn ppcm(&self) -> Option<f64> {
        let [a, b] = self.line?;
        cmd::pixels_per_cm(a, b, f64::from(self.length_cm))
    }

    fn render(&self) -> Render {
        Render { shadows: self.shadows / 100.0, folds: self.folds / 100.0, opacity: self.opacity / 100.0 }
    }

    /// A photo in place of the current one: the scale starts again (drawing armed), the visuals
    /// stay, at the same place on the new photo.
    fn set_photo(&mut self, path: &str, photo: Photo) {
        let (w, h) = (photo.width, photo.height);
        if let Some(old) = &self.photo {
            let (kx, ky) = (f64::from(w) / f64::from(old.width.max(1)), f64::from(h) / f64::from(old.height.max(1)));
            for it in &mut self.items {
                it.center = [it.center[0] * kx, it.center[1] * ky];
            }
        }
        let (preview, k) = photo.reduced(PREVIEW_SIDE);
        let light = Light::new(&preview.rgba, preview.width as usize, preview.height as usize);
        self.photo = Some(PhotoState { path: path.to_string(), name: photo.name, width: w, height: h, preview, k, light });
        self.line = None;
        self.drawing = true;
        self.status = tl!("Draw the reference line on the photo").to_string();
        self.dirty = true;
    }

    /// Where a new visual lands: on the middle of the line (else the photo's), after the others.
    fn landing(&self) -> [f64; 2] {
        let Some(ph) = &self.photo else { return [0.0, 0.0] };
        let base = match self.line {
            Some([a, b]) => [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0],
            None => [f64::from(ph.width) / 2.0, f64::from(ph.height) / 2.0],
        };
        let step = self.ppcm().unwrap_or(0.0) * CASCADE_CM * self.items.len() as f64;
        [(base[0] + step).min(f64::from(ph.width)), (base[1] + step).min(f64::from(ph.height))]
    }

    fn add_visual(&mut self, origin: Origin, v: Visual) -> Result<(), String> {
        if self.ppcm().is_none() {
            return Err(tl!("Set the scale first").to_string());
        }
        if self.items.len() >= MAX_ITEMS {
            return Err(crate::i18n::fmt(tl!("At most {n} visuals"), &[("n", &MAX_ITEMS.to_string())]));
        }
        let (w, h) = cmd::natural_size_cm(&v);
        let center = self.landing();
        self.items.push(Item {
            origin,
            thumb: Some(preview_image(&v)),
            visual: v,
            width_cm: w as f32,
            height_cm: h as f32,
            lock: true,
            rotation: 0.0,
            center,
            tex: None,
            cache: None,
        });
        self.selected = Some(self.items.len() - 1);
        self.closed[SECTION_VISUAL] = false;
        self.dirty = true;
        Ok(())
    }

    fn remove(&mut self, i: usize) {
        if i < self.items.len() {
            self.items.remove(i);
            self.selected = match self.selected {
                Some(s) if s == i => None,
                Some(s) if s > i => Some(s - 1),
                s => s,
            };
            self.dirty = true;
        }
    }

    /// The visual's centre on the reference line's middle (left to right), else the photo's.
    fn center(&mut self, i: usize) {
        let x = match (self.line, &self.photo) {
            (Some([a, b]), _) => (a[0] + b[0]) / 2.0,
            (None, Some(ph)) => f64::from(ph.width) / 2.0,
            _ => return,
        };
        if let Some(it) = self.items.get_mut(i) {
            it.center[0] = x;
            self.dirty = true;
        }
    }

    /// The visual's size in photo pixels.
    fn size_px(&self, it: &Item) -> (f64, f64) {
        let k = self.ppcm().unwrap_or(0.0);
        (f64::from(it.width_cm) * k, f64::from(it.height_cm) * k)
    }

    /// The top-most visual under `q` (photo pixels).
    fn hit(&self, q: [f64; 2]) -> Option<usize> {
        (0..self.items.len()).rev().find(|i| {
            let it = &self.items[*i];
            let (w, h) = self.size_px(it);
            let (s, c) = f64::from(it.rotation).to_radians().sin_cos();
            let (dx, dy) = (q[0] - it.center[0], q[1] - it.center[1]);
            let (u, v) = (dx * c + dy * s, -dx * s + dy * c);
            u.abs() <= w / 2.0 && v.abs() <= h / 2.0
        })
    }

    /// The preview rendered again, if anything changed.
    fn refresh(&mut self, ctx: &egui::Context) {
        for (i, it) in self.items.iter_mut().enumerate() {
            if let Some(img) = it.thumb.take() {
                it.tex = Some(ctx.load_texture(format!("mockup-thumb-{i}-{}", it.visual.name), img, egui::TextureOptions::LINEAR));
            }
        }
        if !self.dirty {
            return;
        }
        self.dirty = false;
        let ppcm = self.ppcm();
        let Some(ph) = &self.photo else {
            self.shown = None;
            return;
        };
        let pp = ppcm.unwrap_or(0.0) * ph.k;
        let mut errors = Vec::new();
        if pp > 0.0 {
            for it in &mut self.items {
                let size = ((f64::from(it.width_cm) * pp).round().max(1.0) as usize, (f64::from(it.height_cm) * pp).round().max(1.0) as usize);
                if it.cache.as_ref().is_none_or(|c| (c.0, c.1) != size) {
                    it.cache = match cmd::print_pixels(&it.visual, f64::from(it.width_cm), f64::from(it.height_cm), pp) {
                        Ok((px, w, h)) => Some((w, h, px)),
                        Err(e) => {
                            errors.push(e.to_string());
                            None
                        }
                    };
                }
            }
        }
        let prints: Vec<Print> = if pp > 0.0 {
            self.items
                .iter()
                .filter_map(|it| {
                    let (w, h, px) = it.cache.as_ref()?;
                    Some(Print { rgba: px, width: *w, height: *h, center: (it.center[0] * ph.k, it.center[1] * ph.k), rotation: f64::from(it.rotation) })
                })
                .collect()
        } else {
            Vec::new()
        };
        let (w, h) = (ph.preview.width as usize, ph.preview.height as usize);
        let out = cmd::composite(&ph.preview.rgba, w, h, &ph.light, &prints, self.render(), pp);
        let img = egui::ColorImage::from_rgba_unmultiplied([w, h], &out);
        match &mut self.shown {
            Some(tex) => tex.set(img, egui::TextureOptions::LINEAR),
            None => self.shown = Some(ctx.load_texture("mockup-preview", img, egui::TextureOptions::LINEAR)),
        }
        if !errors.is_empty() {
            self.status = errors.join(" · ");
        }
    }

    /// The export's params (without `output`).
    fn params(&self, app: &PhotocraftApp) -> Result<Value, String> {
        let ph = self.photo.as_ref().ok_or(tl!("Load a photo first"))?;
        let ppcm = self.ppcm().ok_or(tl!("Set the scale first"))?;
        if self.items.is_empty() {
            return Err(tl!("Add a visual first").to_string());
        }
        let mut visuals = Vec::new();
        for it in &self.items {
            let mut v = match &it.origin {
                Origin::File(path) => json!({"path": path}),
                Origin::Document(id) => {
                    let i = app
                        .session
                        .documents()
                        .iter()
                        .position(|d| d.doc.id == *id)
                        .ok_or_else(|| format!("{}: {}", it.visual.name, tl!("the document was closed")))?;
                    json!({"document": i})
                }
            };
            v["x"] = json!(it.center[0]);
            v["y"] = json!(it.center[1]);
            v["widthCm"] = json!(it.width_cm);
            v["heightCm"] = json!(it.height_cm);
            v["rotation"] = json!(it.rotation);
            visuals.push(v);
        }
        Ok(json!({
            "photo": ph.path,
            "pixelsPerCm": ppcm,
            "visuals": visuals,
            "shadows": self.shadows,
            "folds": self.folds,
            "opacity": self.opacity,
            "name": format!("{} mockup", ph.name),
        }))
    }

    fn describe(&self) -> Value {
        json!({
            "photo": self.photo.as_ref().map(|p| json!({"name": p.name, "pixels": [p.width, p.height]})),
            "line": self.line,
            "lengthCm": self.length_cm,
            "pixelsPerCm": self.ppcm(),
            "drawing": self.drawing,
            "visuals": self.items.iter().map(|it| json!({"name": it.visual.name, "x": it.center[0], "y": it.center[1], "widthCm": it.width_cm, "heightCm": it.height_cm, "lock": it.lock, "rotation": it.rotation})).collect::<Vec<_>>(),
            "selected": self.selected,
            "render": {"shadows": self.shadows, "folds": self.folds, "opacity": self.opacity},
            "format": if self.jpg { "jpg" } else { "png" },
            "status": self.status,
        })
    }
}

// ---------------------------------------------------------------- opening, files, export

pub fn open(app: &mut PhotocraftApp) {
    if app.mockup.is_none() {
        app.mockup = Some(MockupDialog::new());
        // In beta: its window, the first time in this version.
        crate::beta::opened(app, cmd::MOCKUP);
    }
}

fn set_status(app: &mut PhotocraftApp, msg: String) {
    if let Some(d) = app.mockup.as_mut() {
        d.status = msg;
    }
}

/// The garment's photo, from a PNG or JPG file.
pub fn load_photo(app: &mut PhotocraftApp, path: &str) {
    match cmd::load_photo(path) {
        Ok(p) => {
            if let Some(d) = app.mockup.as_mut() {
                d.set_photo(path, p);
            }
        }
        Err(e) => set_status(app, e.to_string()),
    }
}

/// PNG and SVG visuals from disk; failures go to the status line.
pub fn add_files(app: &mut PhotocraftApp, paths: &[String]) {
    let Some(d) = app.mockup.as_mut() else { return };
    let mut errors = Vec::new();
    for path in paths {
        match gang::load_file(path) {
            Ok(v) => {
                if let Err(e) = d.add_visual(Origin::File(path.clone()), v) {
                    errors.push(e);
                    break;
                }
            }
            Err(e) => errors.push(e.to_string()),
        }
    }
    if !errors.is_empty() {
        d.status = errors.join(" · ");
    }
}

/// The active document, flattened, as a visual.
pub fn add_document(app: &mut PhotocraftApp) -> Result<(), String> {
    let st = app.session.active().ok_or(tl!("Open a document first"))?;
    let v = gang::load_document(&st.doc, &st.doc.name).map_err(|e| e.to_string())?;
    let id = st.doc.id;
    app.mockup.as_mut().map_or(Ok(()), |d| d.add_visual(Origin::Document(id), v))
}

/// Files dropped on the window: the photo first (PNG or JPG), then the visuals (PNG or SVG).
pub fn drop_files(app: &mut PhotocraftApp, files: &[egui::DroppedFileHandle]) {
    let paths: Vec<String> = files.iter().map(|f| f.path()).filter(|p| p.is_absolute()).map(|p| p.to_string_lossy().into_owned()).collect();
    let mut visuals = Vec::new();
    for path in paths {
        let photo_first = app.mockup.as_ref().is_some_and(|d| d.photo.is_none());
        let jpg = matches!(std::path::Path::new(&path).extension().and_then(|e| e.to_str()).map(str::to_ascii_lowercase).as_deref(), Some("jpg" | "jpeg"));
        if cmd::is_photo_file(&path) && (photo_first || jpg) {
            load_photo(app, &path);
        } else {
            visuals.push(path);
        }
    }
    add_files(app, &visuals);
}

fn pick_photo(app: &mut PhotocraftApp) {
    let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
    let r = app.ask_file(
        FileDialogRequest::Open { multiple: false, initial_dir, extensions: Some(vec!["png".into(), "jpg".into(), "jpeg".into()]) },
        |app, answer| match answer {
            FileDialogAnswer::Paths(paths) => {
                if let Some(p) = paths.first() {
                    load_photo(app, p);
                }
                Ok(Value::Null)
            }
            _ => Err("The photo is opened from a folder on this computer".to_string()),
        },
    );
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
    {
        set_status(app, e);
    }
}

fn pick_visuals(app: &mut PhotocraftApp) {
    let initial_dir = crate::file_dialog::last_used_dir(&app.ui.recent_files);
    let r = app.ask_file(
        FileDialogRequest::Open { multiple: true, initial_dir, extensions: Some(vec!["png".into(), "svg".into(), "svgz".into()]) },
        |app, answer| match answer {
            FileDialogAnswer::Paths(paths) => {
                add_files(app, &paths);
                Ok(Value::Null)
            }
            _ => Err("PNG and SVG files are added from a folder on this computer".to_string()),
        },
    );
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
    {
        set_status(app, e);
    }
}

/// Renders the mockup at the photo's resolution, writes it to `path` and opens it; the window
/// closes on success.
fn export_to(app: &mut PhotocraftApp, path: &str) -> Result<Value, String> {
    let d = app.mockup.as_ref().ok_or("Mockup isn't open")?;
    let mut params = d.params(app)?;
    params["output"] = json!(path);
    match app.run(cmd::MOCKUP, params) {
        Ok(v) => {
            app.mockup = None;
            Ok(v)
        }
        Err(e) => {
            set_status(app, e.clone());
            Err(e)
        }
    }
}

fn export(app: &mut PhotocraftApp) {
    let Some(d) = app.mockup.as_ref() else { return };
    if let Err(e) = d.params(app) {
        set_status(app, e);
        return;
    }
    let name = d.photo.as_ref().map_or("Mockup".to_string(), |p| format!("{} mockup", p.name));
    let suggested = format!("{name}.{}", if d.jpg { "jpg" } else { "png" });
    let r = app.pick_save(&suggested, |app, path| export_to(app, &path));
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
    {
        set_status(app, e);
    }
}

// ---------------------------------------------------------------- control channel

/// Menu / control routing. `Some` when this module handles the invocation.
pub fn menu(app: &mut PhotocraftApp, _ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != cmd::MOCKUP {
        return None;
    }
    if params.as_object().is_none_or(|o| o.is_empty()) {
        open(app);
        return Some(Ok(app.mockup.as_ref().map(MockupDialog::describe).unwrap_or(Value::Null)));
    }
    let ui = params.get("ui")?;
    open(app);
    Some(control(app, ui))
}

fn point(v: Option<&Value>) -> Option<[f64; 2]> {
    let a = v?.as_array()?;
    Some([a.first()?.as_f64()?, a.get(1)?.as_f64()?])
}

fn control(app: &mut PhotocraftApp, ui: &Value) -> Result<Value, String> {
    let f32_of = |v: Option<&Value>| v.and_then(Value::as_f64).filter(|f| f.is_finite()).map(|f| f as f32);
    if let Some(path) = ui.get("photo").and_then(Value::as_str) {
        load_photo(app, path);
    }
    if let Some(sc) = ui.get("scale")
        && let Some(d) = app.mockup.as_mut()
    {
        if let (Some(a), Some(b)) = (point(sc.get("from")), point(sc.get("to"))) {
            d.line = Some([a, b]);
            d.drawing = false;
        }
        if let Some(cm) = f32_of(sc.get("cm")) {
            d.length_cm = cm.clamp(0.1, 1000.0);
        }
        if d.ppcm().is_some() {
            d.status.clear();
        }
        d.dirty = true;
    }
    if let Some(on) = ui.get("draw").and_then(Value::as_bool)
        && let Some(d) = app.mockup.as_mut()
    {
        d.drawing = on && d.photo.is_some();
    }
    if let Some(paths) = ui.get("addFiles").and_then(Value::as_array) {
        let paths: Vec<String> = paths.iter().filter_map(Value::as_str).map(str::to_string).collect();
        add_files(app, &paths);
    }
    if ui.get("addDocument").and_then(Value::as_bool) == Some(true)
        && let Err(e) = add_document(app)
    {
        set_status(app, e);
    }
    let Some(d) = app.mockup.as_mut() else { return Ok(Value::Null) };
    if let Some(sel) = ui.get("select") {
        d.selected = sel.as_u64().map(|v| v as usize).filter(|i| *i < d.items.len());
    }
    if let Some(v) = ui.get("visual") {
        let i = v.get("index").and_then(Value::as_u64).map(|v| v as usize).or(d.selected).ok_or("no visual")?;
        let it = d.items.get_mut(i).ok_or("no such visual")?;
        if let Some(l) = v.get("lock").and_then(Value::as_bool) {
            it.lock = l;
        }
        let ratio = it.height_cm / it.width_cm.max(1e-6);
        if let Some(w) = f32_of(v.get("widthCm")) {
            it.width_cm = w.clamp(0.1, 500.0);
            if it.lock {
                it.height_cm = (it.width_cm * ratio).clamp(0.1, 500.0);
            }
        }
        if let Some(h) = f32_of(v.get("heightCm")) {
            it.height_cm = h.clamp(0.1, 500.0);
            if it.lock && v.get("widthCm").is_none() {
                it.width_cm = (it.height_cm / ratio.max(1e-6)).clamp(0.1, 500.0);
            }
        }
        if let Some(r) = f32_of(v.get("rotation")) {
            it.rotation = r.clamp(-180.0, 180.0);
        }
        if let (Some(x), Some(y)) = (v.get("x").and_then(Value::as_f64), v.get("y").and_then(Value::as_f64)) {
            it.center = [x, y];
        }
        d.selected = Some(i);
        d.dirty = true;
    }
    if ui.get("center").and_then(Value::as_bool) == Some(true)
        && let Some(i) = d.selected
    {
        d.center(i);
    }
    if let Some(i) = ui.get("remove").and_then(Value::as_u64) {
        d.remove(i as usize);
    }
    if let Some(r) = ui.get("render") {
        for (k, slot) in [("shadows", &mut d.shadows), ("folds", &mut d.folds), ("opacity", &mut d.opacity)] {
            if let Some(v) = f32_of(r.get(k)) {
                *slot = v.clamp(0.0, 100.0);
            }
        }
        d.dirty = true;
    }
    if let Some(f) = ui.get("format").and_then(Value::as_str) {
        d.jpg = f.eq_ignore_ascii_case("jpg") || f.eq_ignore_ascii_case("jpeg");
    }
    if let Some(path) = ui.get("export").and_then(Value::as_str) {
        return export_to(app, path);
    }
    if ui.get("cancel").and_then(Value::as_bool) == Some(true) {
        app.mockup = None;
        return Ok(Value::Null);
    }
    Ok(app.mockup.as_ref().map(MockupDialog::describe).unwrap_or(Value::Null))
}

// ---------------------------------------------------------------- the window

#[derive(Default)]
struct Actions {
    load_photo: bool,
    add_files: bool,
    add_document: bool,
    export: bool,
    cancel: bool,
    report_bug: bool,
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let _dtf = crate::dtf_i18n::scope();
    if app.mockup.is_none() {
        return;
    }
    let has_doc = app.session.active().is_some();
    let mut act = Actions::default();
    light_scope(ctx, || draw(app, ctx, has_doc, &mut act));
    if act.report_bug {
        crate::beta::report(app, Some(cmd::MOCKUP));
    }
    if act.load_photo {
        pick_photo(app);
    }
    if act.add_files {
        pick_visuals(app);
    }
    if act.add_document
        && let Err(e) = add_document(app)
    {
        set_status(app, e);
    }
    if act.export {
        export(app);
    }
    if act.cancel {
        app.mockup = None;
    }
}

struct Regions {
    header: ERect,
    left: ERect,
    canvas: ERect,
    right: ERect,
    status: ERect,
}

fn regions(full: ERect) -> Regions {
    let header = ERect::from_min_size(full.min, vec2(full.width(), HEADER_H));
    let status = ERect::from_min_max(pos2(full.left(), full.bottom() - STATUS_H), full.max);
    let left = ERect::from_min_max(pos2(full.left(), header.bottom()), pos2(full.left() + LEFT_W, status.top()));
    let right = ERect::from_min_max(pos2(full.right() - RIGHT_W, header.bottom()), pos2(full.right(), status.top()));
    let canvas = ERect::from_min_max(pos2(left.right(), header.bottom()), pos2(right.left(), status.top()));
    Regions { header, left, canvas, right, status }
}

fn draw(app: &mut PhotocraftApp, ctx: &egui::Context, has_doc: bool, act: &mut Actions) {
    let screen = ctx.content_rect();
    egui::Area::new(egui::Id::unique("dtf-mockup-dialog")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let Some(d) = app.mockup.as_mut() else { return };
        let t = Tokens::get(ctx);
        let (full, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        let p = ui.painter().clone();
        p.rect_filled(full, 0.0, t.canvas);
        let r = regions(full);
        header_bar(ui, d, r.header, &t, act);
        left_column(ui, d, r.left, &t, has_doc, act);
        canvas(ui, d, r.canvas, &t);
        right_column(ui, d, r.right, &t, act);
        status_bar(ui, d, r.status, &t);
        d.refresh(ctx);
        p.line_segment([r.header.left_bottom(), r.header.right_bottom()], Stroke::new(1.0, t.separator));
        p.line_segment([r.left.right_top(), r.left.right_bottom()], Stroke::new(1.0, t.separator));
        p.line_segment([r.right.left_top(), r.right.left_bottom()], Stroke::new(1.0, t.separator));
        p.line_segment([r.status.left_top(), r.status.right_top()], Stroke::new(1.0, t.separator));
    });
    if !ctx.text_edit_focused()
        && let Some(d) = app.mockup.as_mut()
    {
        keys(d, ctx);
    }
}

fn ready(d: &MockupDialog) -> bool {
    d.photo.is_some() && d.ppcm().is_some() && !d.items.is_empty()
}

/// The header: the logo, "DTF Mockup" (its last word in the logo's magenta) and the BETA badge,
/// then Report a Bug, Cancel and Export… on the right.
fn header_bar(ui: &mut egui::Ui, d: &mut MockupDialog, rect: ERect, t: &Tokens, act: &mut Actions) {
    let p = ui.painter().clone();
    p.rect_filled(rect, 0.0, Color32::WHITE);
    let mark = ERect::from_center_size(pos2(rect.left() + 30.0, rect.center().y), vec2(28.0, 28.0));
    crate::brand::paint_mark(ui, mark);
    let title = tl!("DTF Mockup");
    let (first, last) = title.rsplit_once(' ').map_or((title, ""), |(a, b)| (a, b));
    let font = crate::theme::semibold(16.0);
    let ga = p.layout_no_wrap(format!("{first} "), font.clone(), t.text);
    let gb = p.layout_no_wrap(last.to_string(), font, MAGENTA);
    let y = rect.center().y - ga.size().y / 2.0;
    let x = mark.right() + 10.0;
    let (wa, wb) = (ga.size().x, gb.size().x);
    p.galley(pos2(x, y), ga, t.text);
    p.galley(pos2(x + wa, y), gb, MAGENTA);
    ui.interact(ERect::from_min_size(pos2(x, y), vec2(wa + wb, 20.0)), ui.scope_id().with("mockup-title"), Sense::hover())
        .widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, title));
    let mut bx = ui.new_child(
        egui::UiBuilder::new()
            .max_rect(ERect::from_min_size(pos2(x + wa + wb + 8.0, rect.center().y - 10.0), vec2(60.0, 20.0)))
            .layout(egui::Layout::left_to_right(egui::Align::Center)),
    );
    crate::beta::badge(&mut bx);
    let buttons = ERect::from_min_max(pos2(rect.right() - 470.0, rect.top()), rect.max).shrink2(vec2(16.0, 10.0));
    let mut b = ui.new_child(egui::UiBuilder::new().max_rect(buttons).layout(egui::Layout::right_to_left(egui::Align::Center)));
    let ok = ready(d);
    let r = b.add_enabled_ui(ok, |h| widgets::primary_button(h, tl!("Export…"), 120.0)).inner;
    if r.on_disabled_hover_text(tl!("A photo, its scale and at least one visual")).clicked() {
        act.export = true;
    }
    if widgets::secondary_button(&mut b, tl!("Cancel"), 90.0).clicked() {
        act.cancel = true;
    }
    b.add_space(8.0);
    if b.link(tl!("Report a Bug")).on_hover_text(tl!("A report to complete and copy into an e-mail")).clicked() {
        act.report_bug = true;
    }
}

/// A step's card title: its number in a magenta disc, then its name.
fn step_title(ui: &mut egui::Ui, n: u32, title: &str, t: &Tokens) {
    let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
    let disc = pos2(r.left() + 9.0, r.center().y);
    ui.painter().circle_filled(disc, 9.0, MAGENTA);
    ui.painter().text(disc, Align2::CENTER_CENTER, n.to_string(), crate::theme::semibold(11.0), Color32::WHITE);
    ui.painter().text(pos2(r.left() + 26.0, r.center().y), Align2::LEFT_CENTER, title, crate::theme::semibold(13.0), t.text);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, title));
}

/// A wide button that stays pressed while `on` (magenta outline): Draw the Reference Line.
fn toggle_button(ui: &mut egui::Ui, label: &str, on: bool, width: f32) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(width, 30.0), Sense::click());
    let enabled = ui.is_enabled();
    let (fill, stroke, text) = if on {
        (Color32::from_rgb(251, 234, 246), Stroke::new(1.5, MAGENTA), Color32::from_rgb(156, 31, 138))
    } else if resp.hovered() {
        (t.hover, Stroke::new(1.0, t.field_border), t.text)
    } else {
        (Color32::WHITE, Stroke::new(1.0, t.field_border), if enabled { t.text } else { t.text_faint })
    };
    ui.painter().rect(r, 15.0, fill, stroke, StrokeKind::Inside);
    ui.painter().text(r.center(), Align2::CENTER_CENTER, label, crate::theme::medium(12.5), text);
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Button, enabled, on, label));
    resp.on_hover_cursor(egui::CursorIcon::PointingHand)
}

fn num_text(v: f64, decimals: u32) -> String {
    widgets::fmt_num_prec(v, decimals)
}

/// The left column: the three steps (photo, scale, visuals).
fn left_column(ui: &mut egui::Ui, d: &mut MockupDialog, rect: ERect, t: &Tokens, has_doc: bool, act: &mut Actions) {
    ui.painter().rect_filled(rect, 0.0, t.dock);
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 12.0))));
    panel.spacing_mut().item_spacing.y = 10.0;
    egui::ScrollArea::vertical().id_salt("mockup-left").auto_shrink([false, false]).show(&mut panel, |ui| {
        let w = ui.available_width();
        // 1. The photo.
        card_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            step_title(ui, 1, tl!("Product Photo"), t);
            if blue_button(ui, tl!("Load a Photo (PNG, JPG)"), ui.available_width(), false).clicked() {
                act.load_photo = true;
            }
            match &d.photo {
                Some(ph) => {
                    ui.label(egui::RichText::new(format!("{} · {} × {} px", ph.name, ph.width, ph.height)).size(11.5).color(t.text_faint));
                }
                None => {
                    ui.label(egui::RichText::new(tl!("The garment from the front, flat or worn, in good light.")).size(11.5).color(t.text_faint));
                }
            }
        });
        // 2. The scale.
        card_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            step_title(ui, 2, tl!("Scale"), t);
            ui.add_enabled_ui(d.photo.is_some(), |ui| {
                let label = if d.line.is_some() && !d.drawing { tl!("Draw the Line Again") } else { tl!("Draw a Reference Line on the Photo") };
                if toggle_button(ui, label, d.drawing, ui.available_width())
                    .on_hover_text(tl!("Drag along something whose length you know: the chest width, say"))
                    .clicked()
                {
                    d.drawing = !d.drawing;
                }
                let before = d.length_cm;
                if number_row(ui, tl!("Real length:"), &mut d.length_cm, 0.1..=1000.0, "cm", 1) && (d.length_cm - before).abs() > f32::EPSILON {
                    d.dirty = true;
                }
            });
            match d.ppcm() {
                Some(k) => {
                    let text = crate::i18n::fmt(tl!("✓ 1 cm = {px} px on the photo · rulers in cm"), &[("px", &num_text(k, 1))]);
                    ui.label(egui::RichText::new(text).size(11.5).color(GREEN));
                }
                None => {
                    ui.label(
                        egui::RichText::new(tl!("Draw a line along a length you can measure on the garment, then type that length."))
                            .size(11.5)
                            .color(t.text_faint),
                    );
                }
            }
        });
        // 3. The visuals.
        card_frame(t).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.spacing_mut().item_spacing.y = 6.0;
            step_title(ui, 3, tl!("Visuals"), t);
            let scaled = d.ppcm().is_some();
            ui.add_enabled_ui(scaled, |ui| {
                if blue_button(ui, tl!("Import Images (PNG, SVG)"), ui.available_width(), false).clicked() {
                    act.add_files = true;
                }
                ui.add_enabled_ui(has_doc, |ui| {
                    if widgets::secondary_button(ui, tl!("Add Open Document"), ui.available_width()).clicked() {
                        act.add_document = true;
                    }
                });
            });
            if !scaled {
                ui.label(egui::RichText::new(tl!("Set the scale first")).size(11.5).color(t.warning));
            }
            let mut pick = None;
            let mut remove = None;
            for (i, it) in d.items.iter().enumerate() {
                let (row, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 46.0), Sense::click());
                let on = d.selected == Some(i);
                let (fill, stroke) =
                    if on { (Color32::from_rgb(251, 234, 246), Stroke::new(1.5, MAGENTA)) } else { (Color32::WHITE, Stroke::new(1.0, t.card_border)) };
                ui.painter().rect(row, 8.0, fill, stroke, StrokeKind::Inside);
                let img = ERect::from_min_size(row.min + vec2(7.0, 7.0), vec2(32.0, 32.0));
                widgets::checker_rounded(ui.painter(), img, 5.0, 4);
                if let Some(tex) = &it.tex {
                    let s = tex.size_vec2();
                    let k = (img.width() / s.x.max(1.0)).min(img.height() / s.y.max(1.0));
                    ui.painter().image(
                        tex.id(),
                        ERect::from_center_size(img.center(), s * k),
                        ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)),
                        Color32::WHITE,
                    );
                }
                let text_w = row.right() - 30.0 - (img.right() + 8.0);
                let name = ui.painter().layout_no_wrap(it.visual.name.clone(), crate::theme::medium(12.5), t.text);
                let name = if name.size().x > text_w {
                    let short: String = it.visual.name.chars().take(((text_w / 7.0) as usize).max(4)).collect();
                    ui.painter().layout_no_wrap(format!("{short}…"), crate::theme::medium(12.5), t.text)
                } else {
                    name
                };
                ui.painter().galley(pos2(img.right() + 8.0, row.top() + 7.0), name, t.text);
                let dpi = crate::i18n::fmt(tl!("{dpi} ppi"), &[("dpi", &format!("{:.0}", it.visual.dpi))]);
                let sub = format!("{} × {} cm · {dpi}", num_text(f64::from(it.width_cm), 1), num_text(f64::from(it.height_cm), 1));
                ui.painter().text(pos2(img.right() + 8.0, row.top() + 27.0), Align2::LEFT_TOP, sub, FontId::proportional(11.0), t.text_faint);
                let x = ERect::from_center_size(pos2(row.right() - 16.0, row.center().y), vec2(18.0, 18.0));
                let xr = ui.interact(x, ui.scope_id().with(("mockup-x", i)), Sense::click());
                if xr.hovered() {
                    ui.painter().rect_filled(x, 4.0, t.danger);
                }
                crate::icons::image("x", 11.0, if xr.hovered() { Color32::WHITE } else { t.text_faint })
                    .paint_at(ui, ERect::from_center_size(x.center(), vec2(11.0, 11.0)));
                let remove_name = crate::i18n::fmt(tl!("Remove {name}"), &[("name", &it.visual.name)]);
                xr.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &remove_name));
                resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, &it.visual.name));
                if xr.on_hover_text(tl!("Remove")).clicked() {
                    remove = Some(i);
                } else if resp.clicked() {
                    pick = Some(i);
                }
            }
            if !d.items.is_empty() {
                ui.label(egui::RichText::new(tl!("Each visual arrives at its print size: its pixels at its resolution.")).size(11.0).color(t.text_faint));
            }
            if let Some(i) = remove {
                d.remove(i);
            } else if let Some(i) = pick {
                d.selected = Some(i);
                d.closed[SECTION_VISUAL] = false;
            }
        });
        let _ = w;
    });
}

/// A section of the right column: a card whose title row opens or closes it.
fn section(ui: &mut egui::Ui, d: &mut MockupDialog, k: usize, title: &str, t: &Tokens, body: impl FnOnce(&mut egui::Ui, &mut MockupDialog)) {
    card_frame(t).show(ui, |ui| {
        ui.set_width(ui.available_width());
        let open = !d.closed[k];
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::click());
        ui.painter().text(r.left_center(), Align2::LEFT_CENTER, title, crate::theme::semibold(13.0), t.text);
        let icon = if open { "chevron-down" } else { "chevron-right" };
        crate::icons::image(icon, 14.0, t.text_dim).paint_at(ui, ERect::from_center_size(r.right_center() - vec2(8.0, 0.0), vec2(14.0, 14.0)));
        resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::DisclosureTriangle, true, title));
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            d.closed[k] = open;
        }
        if open {
            ui.add_space(6.0);
            ui.spacing_mut().item_spacing.y = 6.0;
            body(ui, d);
        }
    });
    ui.add_space(8.0);
}

fn right_column(ui: &mut egui::Ui, d: &mut MockupDialog, rect: ERect, t: &Tokens, act: &mut Actions) {
    ui.painter().rect_filled(rect, 0.0, t.dock);
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 12.0))));
    egui::ScrollArea::vertical().id_salt("mockup-right").auto_shrink([false, false]).show(&mut panel, |ui| {
        section(ui, d, SECTION_VISUAL, tl!("Selected Visual"), t, |ui, d| visual_section(ui, d, t));
        section(ui, d, SECTION_RENDER, tl!("Rendering"), t, |ui, d| render_section(ui, d, t));
        section(ui, d, SECTION_EXPORT, tl!("Export"), t, |ui, d| export_section(ui, d, t, act));
    });
}

fn visual_section(ui: &mut egui::Ui, d: &mut MockupDialog, t: &Tokens) {
    let Some(i) = d.selected.filter(|i| *i < d.items.len()) else {
        ui.label(egui::RichText::new(tl!("Click a visual on the photo or in the list.")).size(11.5).color(t.text_faint));
        return;
    };
    let mut center = false;
    let mut delete = false;
    let mut changed = false;
    if let Some(it) = d.items.get_mut(i) {
        ui.label(egui::RichText::new(&it.visual.name).font(crate::theme::semibold(12.5)).color(t.text));
        let ratio = it.height_cm / it.width_cm.max(1e-6);
        let (mut w, mut h) = (it.width_cm, it.height_cm);
        if number_row(ui, tl!("Width:"), &mut w, 0.1..=500.0, "cm", 2) && (w - it.width_cm).abs() > 1e-4 {
            it.width_cm = w;
            if it.lock {
                it.height_cm = (w * ratio).clamp(0.1, 500.0);
            }
            changed = true;
        }
        if number_row(ui, tl!("Height:"), &mut h, 0.1..=500.0, "cm", 2) && (h - it.height_cm).abs() > 1e-4 {
            it.height_cm = h;
            if it.lock {
                it.width_cm = (h / ratio.max(1e-6)).clamp(0.1, 500.0);
            }
            changed = true;
        }
        switch_row(ui, tl!("Keep Proportions"), &mut it.lock);
        let mut r = it.rotation;
        if number_row(ui, tl!("Rotation:"), &mut r, -180.0..=180.0, "°", 0) && (r - it.rotation).abs() > 1e-4 {
            it.rotation = r;
            changed = true;
        }
        let (nw, nh) = cmd::natural_size_cm(&it.visual);
        if (f64::from(it.width_cm) - nw).abs() > 0.05 || (f64::from(it.height_cm) - nh).abs() > 0.05 {
            let text =
                crate::i18n::fmt(tl!("Its file prints at {w} × {h} cm: this size is the mockup's only."), &[("w", &num_text(nw, 1)), ("h", &num_text(nh, 1))]);
            ui.label(egui::RichText::new(text).size(11.0).color(t.warning));
        }
        let bw = ui.available_width();
        if widgets::secondary_button(ui, tl!("Center Horizontally"), bw).on_hover_text(tl!("On the middle of the reference line")).clicked() {
            center = true;
        }
        if widgets::secondary_button(ui, tl!("Delete"), bw).clicked() {
            delete = true;
        }
    }
    if changed {
        d.dirty = true;
    }
    if center {
        d.center(i);
    }
    if delete {
        d.remove(i);
    }
}

fn render_section(ui: &mut egui::Ui, d: &mut MockupDialog, t: &Tokens) {
    let mut changed = false;
    changed |= widgets::slider_row(ui, tl!("Fabric Shadows"), &mut d.shadows, 0.0..=100.0, "%", None).changed();
    changed |= widgets::slider_row(ui, tl!("Folds (Distortion)"), &mut d.folds, 0.0..=100.0, "%", None).changed();
    changed |= widgets::slider_row(ui, tl!("Ink Opacity"), &mut d.opacity, 0.0..=100.0, "%", None).changed();
    if changed {
        d.dirty = true;
    }
    ui.label(egui::RichText::new(tl!("Only a preview: what prints is your file.")).size(11.0).color(t.text_faint));
}

fn export_section(ui: &mut egui::Ui, d: &mut MockupDialog, t: &Tokens, act: &mut Actions) {
    ui.horizontal(|ui| {
        let w = (ui.available_width() - 8.0) / 2.0;
        if toggle_button(ui, "PNG", !d.jpg, w).clicked() {
            d.jpg = false;
        }
        if toggle_button(ui, "JPG", d.jpg, w).clicked() {
            d.jpg = true;
        }
    });
    ui.label(egui::RichText::new(tl!("At the photo's resolution, then opened as a new document.")).size(11.0).color(t.text_faint));
    let ok = ready(d);
    let w = ui.available_width();
    if ui.add_enabled_ui(ok, |ui| widgets::primary_button(ui, tl!("Export…"), w)).inner.clicked() {
        act.export = true;
    }
}

fn status_bar(ui: &mut egui::Ui, d: &MockupDialog, rect: ERect, t: &Tokens) {
    let p = ui.painter();
    p.rect_filled(rect, 0.0, Color32::WHITE);
    let text = match (d.ppcm(), &d.photo) {
        (Some(k), Some(ph)) => crate::i18n::fmt(
            tl!("Scale: 1 cm = {px} px · Photo {w} × {h} cm · {n} visuals"),
            &[
                ("px", &num_text(k, 1)),
                ("w", &num_text(f64::from(ph.width) / k, 1)),
                ("h", &num_text(f64::from(ph.height) / k, 1)),
                ("n", &d.items.len().to_string()),
            ],
        ),
        (None, Some(_)) => tl!("No scale yet").to_string(),
        _ => tl!("No photo").to_string(),
    };
    p.text(pos2(rect.left() + 14.0, rect.center().y), Align2::LEFT_CENTER, text, FontId::proportional(12.0), t.text_dim);
    if !d.status.is_empty() {
        p.text(pos2(rect.right() - 14.0, rect.center().y), Align2::RIGHT_CENTER, &d.status, FontId::proportional(12.0), t.text_dim);
    }
}

/// The ruler along `edge` (top or left of the photo, `horizontal`), a tick each cm.
fn ruler(p: &egui::Painter, photo: ERect, horizontal: bool, cm_px: f32, t: &Tokens) {
    let band = if horizontal {
        ERect::from_min_max(pos2(photo.left(), photo.top() - RULER), pos2(photo.right(), photo.top() - 2.0))
    } else {
        ERect::from_min_max(pos2(photo.left() - RULER, photo.top()), pos2(photo.left() - 2.0, photo.bottom()))
    };
    p.rect_filled(band, 3.0, Color32::from_rgba_unmultiplied(255, 255, 255, 200));
    // Thin ticks when a cm is too narrow: every 5.
    let step = if cm_px < 4.0 { 5 } else { 1 };
    let label_every = if cm_px * 10.0 < 34.0 { 20 } else { 10 };
    let len = if horizontal { photo.width() } else { photo.height() };
    let mut k = 0;
    while (k as f32) * cm_px <= len + 0.5 {
        let at = k as f32 * cm_px;
        let tick = if k % 10 == 0 {
            9.0
        } else if k % 5 == 0 {
            6.0
        } else {
            3.5
        };
        let stroke = Stroke::new(1.0, t.text_faint);
        if horizontal {
            let x = photo.left() + at;
            p.line_segment([pos2(x, band.bottom()), pos2(x, band.bottom() - tick)], stroke);
            if k % label_every == 0 && k > 0 && x + 20.0 < band.right() {
                p.text(pos2(x + 2.0, band.top() + 1.0), Align2::LEFT_TOP, k.to_string(), FontId::proportional(9.5), t.text_dim);
            }
        } else {
            let y = photo.top() + at;
            p.line_segment([pos2(band.right(), y), pos2(band.right() - tick, y)], stroke);
            if k % label_every == 0 && k > 0 && y + 14.0 < band.bottom() {
                p.text(pos2(band.left() + 1.0, y + 2.0), Align2::LEFT_TOP, k.to_string(), FontId::proportional(9.5), t.text_dim);
            }
        }
        k += step;
    }
}

/// A pill with `text` centred at `at`.
fn pill(p: &egui::Painter, at: Pos2, text: &str, fill: Color32, color: Color32) {
    let g = p.layout_no_wrap(text.to_string(), FontId::proportional(11.0), color);
    let r = ERect::from_center_size(at, g.size() + vec2(14.0, 6.0));
    p.rect_filled(r, r.height() / 2.0, fill);
    p.galley(r.center() - g.size() / 2.0, g, color);
}

/// The photo, the visuals' outlines, the reference line and the rulers; the line drawn, its ends
/// and the visuals moved with the mouse.
fn canvas(ui: &mut egui::Ui, d: &mut MockupDialog, rect: ERect, t: &Tokens) {
    let p = ui.painter_at(rect);
    p.rect_filled(rect, 0.0, t.canvas);
    let Some((pw, ph)) = d.photo.as_ref().map(|ph| (ph.width as f32, ph.height as f32)) else {
        p.text(rect.center(), Align2::CENTER_CENTER, tl!("Start with the product's photo (step 1)."), FontId::proportional(14.0), t.text_faint);
        return;
    };
    let inner = ERect::from_min_max(rect.min + vec2(RULER + 16.0, RULER + 16.0), rect.max - vec2(16.0, 16.0));
    let z = (inner.width() / pw).min(inner.height() / ph).max(1e-4);
    let photo = ERect::from_center_size(inner.center(), vec2(pw * z, ph * z));
    let to_screen = |q: [f64; 2]| pos2(photo.left() + q[0] as f32 * z, photo.top() + q[1] as f32 * z);
    let to_photo = |s: Pos2| [f64::from((s.x - photo.left()) / z), f64::from((s.y - photo.top()) / z)];
    if let Some(tex) = &d.shown {
        p.image(tex.id(), photo, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    p.rect_stroke(photo, 0.0, Stroke::new(1.0, t.card_border), StrokeKind::Outside);
    let ppcm = d.ppcm();
    if let Some(k) = ppcm {
        let cm_px = k as f32 * z;
        ruler(&p, photo, true, cm_px, t);
        ruler(&p, photo, false, cm_px, t);
    }
    // The visuals' outlines, the selected one in magenta with its size.
    for (i, it) in d.items.iter().enumerate() {
        let (w, h) = d.size_px(it);
        let (s, c) = f64::from(it.rotation).to_radians().sin_cos();
        let corners: Vec<Pos2> = [(-w / 2.0, -h / 2.0), (w / 2.0, -h / 2.0), (w / 2.0, h / 2.0), (-w / 2.0, h / 2.0)]
            .iter()
            .map(|(x, y)| to_screen([it.center[0] + x * c - y * s, it.center[1] + x * s + y * c]))
            .collect();
        let on = d.selected == Some(i);
        let stroke = if on { Stroke::new(1.5, MAGENTA) } else { Stroke::new(1.0, Color32::from_rgba_unmultiplied(120, 110, 140, 120)) };
        let mut closed = corners.clone();
        closed.push(corners[0]);
        if on {
            p.add(Shape::line(closed, stroke));
            for q in &corners {
                p.rect(ERect::from_center_size(*q, vec2(7.0, 7.0)), 1.0, Color32::WHITE, Stroke::new(1.5, MAGENTA), StrokeKind::Inside);
            }
            let bottom = corners.iter().map(|q| q.y).fold(f32::MIN, f32::max);
            let mid = corners.iter().map(|q| q.x).sum::<f32>() / 4.0;
            let size = format!("{} × {} cm", num_text(f64::from(it.width_cm), 1), num_text(f64::from(it.height_cm), 1));
            pill(&p, pos2(mid, bottom + 14.0), &size, Color32::WHITE, t.text_dim);
        } else {
            p.extend(Shape::dashed_line(&closed, stroke, 4.0, 3.0));
        }
    }
    // The reference line.
    if let Some([a, b]) = d.line {
        let (sa, sb) = (to_screen(a), to_screen(b));
        p.extend(Shape::dashed_line(&[sa, sb], Stroke::new(2.0, BLUE), 7.0, 4.0));
        for q in [sa, sb] {
            p.circle(q, 6.0, BLUE, Stroke::new(2.0, Color32::WHITE));
        }
        let label = crate::i18n::fmt(tl!("Reference: {cm} cm"), &[("cm", &num_text(f64::from(d.length_cm), 1))]);
        // Over its left end, clear of the visuals in the middle.
        let left = if sa.x <= sb.x { sa } else { sb };
        pill(&p, left + vec2(0.0, -18.0), &label, BLUE, Color32::WHITE);
    }
    if d.drawing {
        pill(
            &p,
            pos2(photo.center().x, photo.top() + 18.0),
            tl!("Drag along a length you know (the chest width, say)"),
            Color32::from_rgb(255, 248, 214),
            Color32::from_rgb(107, 86, 0),
        );
    }
    // The mouse.
    let resp = ui.interact(photo, ui.scope_id().with("mockup-photo"), Sense::click_and_drag());
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Image, true, tl!("Product Photo")));
    let resp = if d.drawing { resp.on_hover_cursor(egui::CursorIcon::Crosshair) } else { resp };
    let ends = d.line.map(|[a, b]| [to_screen(a), to_screen(b)]);
    let press = ui.input(|i| i.pointer.press_origin());
    if resp.drag_started()
        && let Some(o) = press
    {
        let q = to_photo(o);
        d.drag = if d.drawing {
            d.line = Some([q, q]);
            Some(Drag::Line)
        } else if let Some(k) = ends.and_then(|e| e.iter().position(|s| s.distance(o) <= GRAB)) {
            Some(Drag::End(k))
        } else if let Some(i) = d.hit(q) {
            d.selected = Some(i);
            d.closed[SECTION_VISUAL] = false;
            d.items.get(i).map(|it| Drag::Move { index: i, grab: [q[0] - it.center[0], q[1] - it.center[1]] })
        } else {
            None
        };
    }
    if resp.dragged()
        && let (Some(drag), Some(pos)) = (d.drag, resp.interact_pointer_pos())
    {
        let q = to_photo(pos);
        let q = [q[0].clamp(0.0, f64::from(pw)), q[1].clamp(0.0, f64::from(ph))];
        match drag {
            Drag::Line => {
                if let Some(l) = d.line.as_mut() {
                    l[1] = q;
                }
            }
            Drag::End(k) => {
                if let Some(l) = d.line.as_mut() {
                    l[k] = q;
                }
            }
            Drag::Move { index, grab } => {
                if let Some(it) = d.items.get_mut(index) {
                    it.center = [q[0] - grab[0], q[1] - grab[1]];
                }
            }
        }
        d.dirty = true;
    }
    if resp.drag_stopped() {
        if d.drag == Some(Drag::Line) {
            if d.ppcm().is_some() {
                d.drawing = false;
                d.status.clear();
            } else if d.line.is_some_and(|[a, b]| (a[0] - b[0]).hypot(a[1] - b[1]) < 2.0) {
                d.line = None;
            }
        }
        d.drag = None;
    }
    if resp.clicked()
        && !d.drawing
        && let Some(pos) = resp.interact_pointer_pos()
    {
        d.selected = d.hit(to_photo(pos));
        if d.selected.is_some() {
            d.closed[SECTION_VISUAL] = false;
        }
    }
}

/// Delete removes the selected visual, the arrows move it (a mm, a cm with Shift), Esc stops
/// drawing the line.
fn keys(d: &mut MockupDialog, ctx: &egui::Context) {
    let (del, esc, shift, arrows) = ctx.input(|i| {
        (
            i.key_pressed(egui::Key::Delete) || i.key_pressed(egui::Key::Backspace),
            i.key_pressed(egui::Key::Escape),
            i.modifiers.shift,
            [egui::Key::ArrowLeft, egui::Key::ArrowRight, egui::Key::ArrowUp, egui::Key::ArrowDown].map(|k| i.key_pressed(k)),
        )
    });
    if esc && d.drawing {
        d.drawing = false;
    }
    let Some(i) = d.selected else { return };
    if del {
        d.remove(i);
        return;
    }
    let step = d.ppcm().unwrap_or(0.0) * if shift { 1.0 } else { 0.1 };
    let (dx, dy) = (f64::from(u8::from(arrows[1])) - f64::from(u8::from(arrows[0])), f64::from(u8::from(arrows[3])) - f64::from(u8::from(arrows[2])));
    if (dx != 0.0 || dy != 0.0)
        && let Some(it) = d.items.get_mut(i)
    {
        it.center = [it.center[0] + dx * step, it.center[1] + dy * step];
        d.dirty = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, ui: Value) -> Value {
        menu(app, ctx, cmd::MOCKUP, &json!({"ui": ui})).unwrap().unwrap()
    }

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("dtf-mockup-ui-{tag}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// A flat photo (`w × h`, `color`) and a 2.54 cm square visual (300 px at 300 ppi).
    fn files(dir: &std::path::Path, w: u32, h: u32, color: &str) -> (String, String) {
        let mut s = photocraft_engine::Session::new();
        s.execute("file.new", json!({"width": w, "height": h, "resolution": 72, "background": "white"})).unwrap();
        s.execute("edit.fill", json!({"color": color})).unwrap();
        let photo = dir.join("tee.jpg").to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": photo})).unwrap();
        s.execute("file.new", json!({"width": 300, "height": 300, "resolution": 300, "background": "transparent"})).unwrap();
        s.execute("select.rect", json!({"x": 0, "y": 50, "width": 300, "height": 200})).unwrap();
        s.execute("edit.fill", json!({"color": "#c8102e"})).unwrap();
        let logo = dir.join("logo.png").to_string_lossy().into_owned();
        s.execute("file.saveACopy", json!({"path": logo})).unwrap();
        (photo, logo)
    }

    /// DTF › Mockup (beta)… opens the window with its beta notice; the visuals wait for the scale;
    /// a visual arrives at its print size; the export renders the photo with it and closes.
    #[test]
    fn the_mockup_waits_for_the_scale_then_exports() {
        let dir = temp_dir("flow");
        let (photo, logo) = files(&dir, 600, 400, "#d8d8d8");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        let ids: Vec<&str> = crate::menu_catalog::CATALOG.iter().filter(|(path, _, _, _)| path == &["DTF"]).map(|(_, _, _, id)| *id).collect();
        assert!(ids.contains(&cmd::MOCKUP), "in the DTF menu");
        assert!(crate::beta::is_beta(cmd::MOCKUP));
        crate::menus::invoke(&mut app, &ctx, cmd::MOCKUP, json!({})).unwrap();
        assert!(app.mockup.is_some());
        let v = invoke(&mut app, &ctx, json!({"photo": photo}));
        assert_eq!(v["photo"]["pixels"], json!([600, 400]));
        assert_eq!(v["drawing"], true, "the line is asked for at once");
        // No scale: the visual waits.
        let v = invoke(&mut app, &ctx, json!({"addFiles": [logo]}));
        assert_eq!(v["visuals"].as_array().unwrap().len(), 0);
        assert_eq!(v["status"], tl!("Set the scale first"));
        // 200 px = 20 cm: 10 px per cm; the 2.54 × 1.69 cm visual on the line's middle.
        let v = invoke(&mut app, &ctx, json!({"scale": {"from": [200, 150], "to": [400, 150], "cm": 20}, "addFiles": [logo]}));
        assert!((v["pixelsPerCm"].as_f64().unwrap() - 10.0).abs() < 1e-9);
        let it = &v["visuals"][0];
        assert!((it["widthCm"].as_f64().unwrap() - 2.54).abs() < 0.01, "{it}");
        assert!((it["heightCm"].as_f64().unwrap() - 2.54 * 200.0 / 300.0).abs() < 0.02, "cropped to its visible pixels: {it}");
        assert_eq!((it["x"].as_f64().unwrap(), it["y"].as_f64().unwrap()), (300.0, 150.0));
        // Proportions kept: twice as wide, twice as tall.
        let v = invoke(&mut app, &ctx, json!({"visual": {"index": 0, "widthCm": 5.08}}));
        assert!((v["visuals"][0]["heightCm"].as_f64().unwrap() - 3.387).abs() < 0.01);
        // The preview renders.
        app.mockup.as_mut().unwrap().refresh(&ctx);
        assert!(app.mockup.as_ref().unwrap().shown.is_some());
        // Hit test on the visual, not beside it.
        let d = app.mockup.as_ref().unwrap();
        assert_eq!(d.hit([300.0, 150.0]), Some(0));
        assert_eq!(d.hit([340.0, 150.0]), None);
        let out = dir.join("mockup.png").to_string_lossy().into_owned();
        invoke(&mut app, &ctx, json!({"render": {"shadows": 0, "folds": 0, "opacity": 100}, "export": out}));
        assert!(app.mockup.is_none(), "closed once exported");
        assert!(std::path::Path::new(&out).exists());
        let st = app.session.active().unwrap();
        assert_eq!((st.doc.size.width, st.doc.size.height), (600, 400));
        let px = st.doc.layers[0].surface().unwrap().read_region(photocraft_geom::Rect::new(300, 150, 301, 151));
        assert!(px[0] > 0.7 && px[1] < 0.15, "the visual's red: {px:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// The reference line drawn with the mouse sets the scale; a drag moves a visual.
    #[test]
    fn the_line_is_drawn_and_the_visual_dragged() {
        let dir = temp_dir("mouse");
        let (photo, logo) = files(&dir, 600, 400, "#d8d8d8");
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        open(&mut app);
        app.beta = Default::default();
        invoke(&mut app, &ctx, json!({"photo": photo}));
        // Drawn once the app's fonts are in.
        let mut h = egui_kittest::Harness::builder().with_size(vec2(1366.0, 900.0)).build_ui_state(
            |ui, s: &mut (PhotocraftApp, bool)| {
                if s.1 {
                    let ctx = ui.ctx().clone();
                    show(&mut s.0, &ctx);
                }
            },
            (app, false),
        );
        PhotocraftApp::setup_context(&h.ctx, Default::default());
        h.state_mut().1 = true;
        h.run_steps(2);
        // The photo's place on screen: fitted in the canvas.
        let canvas = regions(ERect::from_min_size(Pos2::ZERO, vec2(1366.0, 900.0))).canvas;
        let inner = ERect::from_min_max(canvas.min + vec2(RULER + 16.0, RULER + 16.0), canvas.max - vec2(16.0, 16.0));
        let z = (inner.width() / 600.0).min(inner.height() / 400.0);
        let photo_rect = ERect::from_center_size(inner.center(), vec2(600.0 * z, 400.0 * z));
        let at = |x: f32, y: f32| photo_rect.min + vec2(x * z, y * z);
        let drag = |h: &mut egui_kittest::Harness<'static, (PhotocraftApp, bool)>, a: Pos2, b: Pos2| {
            let button = |pos, pressed| egui::Event::PointerButton { pos, button: egui::PointerButton::Primary, pressed, modifiers: egui::Modifiers::NONE };
            h.hover_at(a);
            h.run_steps(1);
            h.event(button(a, true));
            h.run_steps(1);
            for k in 1..=6 {
                h.hover_at(a + (b - a) * (k as f32 / 6.0));
                h.run_steps(1);
            }
            h.event(button(b, false));
            h.run_steps(2);
        };
        drag(&mut h, at(100.0, 200.0), at(500.0, 200.0));
        let d = h.state().0.mockup.as_ref().unwrap();
        assert!(!d.drawing);
        let [a, b] = d.line.unwrap();
        assert!((b[0] - a[0] - 400.0).abs() < 2.0, "{a:?} {b:?}");
        // 400 px = 40 cm.
        if let Some(d) = h.state_mut().0.mockup.as_mut() {
            d.length_cm = 40.0;
        }
        add_files(&mut h.state_mut().0, &[logo]);
        h.run_steps(2);
        let c0 = h.state().0.mockup.as_ref().unwrap().items[0].center;
        drag(&mut h, at(c0[0] as f32, c0[1] as f32), at(c0[0] as f32 + 50.0, c0[1] as f32 + 30.0));
        let c1 = h.state().0.mockup.as_ref().unwrap().items[0].center;
        assert!((c1[0] - c0[0] - 50.0).abs() < 2.0 && (c1[1] - c0[1] - 30.0).abs() < 2.0, "{c0:?} → {c1:?}");
        let _ = std::fs::remove_dir_all(dir);
    }

    /// Screenshots for review (`DTF_VISUALS=<folder>`, the photo from `MOCKUP_PHOTO` if set): the
    /// window in French with a photo, its scale and two visuals, the first selected; then empty.
    #[test]
    fn capture_mockup() {
        let Some(out) = std::env::var_os("DTF_VISUALS").map(std::path::PathBuf::from) else { return };
        std::fs::create_dir_all(&out).unwrap();
        let _fr = crate::i18n::language_scope(crate::i18n::Lang::from_pref("fr"));
        let dir = temp_dir("capture");
        let (flat, _) = files(&dir, 1000, 1000, "#e9e7e2");
        let photo = std::env::var("MOCKUP_PHOTO").unwrap_or(flat);
        for (name, full) in [("mockup", true), ("mockup_vide", false)] {
            let photo = photo.clone();
            let mut h = egui_kittest::Harness::builder().with_size(vec2(1440.0, 900.0)).with_max_steps(64).wgpu().build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
                crate::dtf_panel::dtf_craft_startup(&mut app);
                let ctx = cc.egui_ctx.clone();
                open(&mut app);
                if full {
                    // A club logo: a red block with a white band, and a small blue square.
                    app.run("file.new", json!({"name": "logo-club", "width": 1200, "height": 900, "resolution": 300, "background": "transparent"})).unwrap();
                    app.run("select.rect", json!({"x": 0, "y": 0, "width": 1200, "height": 900})).unwrap();
                    app.run("edit.fill", json!({"color": "#c8102e"})).unwrap();
                    app.run("select.rect", json!({"x": 0, "y": 380, "width": 1200, "height": 140})).unwrap();
                    app.run("edit.fill", json!({"color": "#ffffff"})).unwrap();
                    app.run("select.deselect", json!({})).unwrap();
                    let w = photo_width(&photo);
                    invoke(
                        &mut app,
                        &ctx,
                        json!({"photo": photo, "scale": {"from": [w * 0.245, w * 0.44], "to": [w * 0.765, w * 0.44], "cm": 52}, "addDocument": true}),
                    );
                    invoke(&mut app, &ctx, json!({"visual": {"index": 0, "widthCm": 24, "x": w * 0.5, "y": w * 0.45}}));
                    app.run("file.new", json!({"name": "logo-coeur", "width": 600, "height": 600, "resolution": 300, "background": "transparent"})).unwrap();
                    app.run("select.rect", json!({"x": 0, "y": 0, "width": 600, "height": 600})).unwrap();
                    app.run("edit.fill", json!({"color": "#1f4fa8"})).unwrap();
                    app.run("select.deselect", json!({})).unwrap();
                    invoke(&mut app, &ctx, json!({"addDocument": true}));
                    invoke(&mut app, &ctx, json!({"visual": {"index": 1, "widthCm": 6, "x": w * 0.64, "y": w * 0.26}, "select": 0}));
                    invoke(&mut app, &ctx, json!({"select": 0}));
                }
                app.beta = Default::default();
                app
            });
            h.state_mut().session.prefs.edit(|p| p.interface.language = "fr".into());
            h.run_steps(8);
            h.render().unwrap().save(out.join(format!("{name}.png"))).unwrap();
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    fn photo_width(path: &str) -> f64 {
        f64::from(cmd::load_photo(path).map_or(1000, |p| p.width))
    }
}
