// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Color Analyzer…: a full-window dialog to read the main colours of the open document
//! and print a sheet of variants around each one.
//!
//! Left: the visual (the open document, flattened, in sRGB) with an eyedropper, and its colours:
//! each one's code (editable), its share of the visual and whether it goes on the sheet. Centre:
//! the sheet as it will print, one block of variants per colour; a click selects a patch.
//! Right: the analysis (number of colours, merging of close shades), the variants (axes, step,
//! block size), the sheet's width and the selected patch, which can become the new centre of
//! its colour for a finer second sheet. Export PNG runs `dtf.colorAnalyzer` with these colours:
//! the sheet is written as an sRGB PNG at 300 ppi and opened as a new document.
//!
//! Control channel: `ui.menu.invoke {"id":"dtf.colorAnalyzer","params":{"ui":{"set":{"maxColors":n,
//! "merge":d,"axisX":axis,"axisY":axis,"step":v,"grid":n,"width":cm},"reanalyze":true,
//! "pick":{"x":px,"y":px},"add":"#rrggbb","color":{"index":i,"hex":"#rrggbb","include":bool},
//! "remove":i,"reset":i,"select":{"color":i,"cell":n},"recenter":true,"zoom":1..8,"export":path,
//! "cancel":true}}}`;
//! without `ui`, the params go to the command itself.

use std::sync::Mutex;

use egui::{Align2, Color32, FontId, Pos2, Rect as ERect, Sense, Stroke, StrokeKind, TextureHandle, pos2, vec2};
use photocraft_algo::color_analyzer::{self as ca, Axis, Board, Cell};
use photocraft_doc::DocId;
use photocraft_engine::color_analyzer_cmds::{self as cmd, Visual};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets;

const LEFT_W: f32 = 340.0;
const RIGHT_W: f32 = 300.0;
const FOOTER_H: f32 = 48.0;
/// Longest side of the visual's preview texture.
const PREVIEW: u32 = 900;
/// Height of the visual in the left column.
const VISUAL_H: f32 = 240.0;

/// The analysis and sheet settings, remembered from one opening to the next.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Prefs {
    pub max_colors: usize,
    pub merge: f32,
    pub x: Axis,
    pub y: Axis,
    pub step: f32,
    pub grid: usize,
    pub width_cm: f32,
}

impl Default for Prefs {
    fn default() -> Self {
        Prefs {
            max_colors: cmd::DEFAULT_COLORS,
            merge: cmd::DEFAULT_MERGE as f32,
            x: Axis::Lightness,
            y: Axis::Saturation,
            step: cmd::DEFAULT_STEP as f32,
            grid: cmd::DEFAULT_GRID,
            width_cm: (Board::default().width / 10.0) as f32,
        }
    }
}

static LAST_PREFS: Mutex<Option<Prefs>> = Mutex::new(None);

/// One colour of the list.
#[derive(Clone, Debug, PartialEq)]
pub struct Row {
    /// As found in the visual (or picked).
    pub original: [u8; 3],
    /// Centre of its variants: the original, or a code chosen after a print.
    pub center: [u8; 3],
    pub share: Option<f64>,
    /// On the sheet?
    pub include: bool,
    /// The code being typed in its field.
    text: String,
}

impl Row {
    fn new(rgb: [u8; 3], share: Option<f64>) -> Self {
        Row { original: rgb, center: rgb, share, include: true, text: ca::hex(rgb) }
    }

    fn set_center(&mut self, rgb: [u8; 3]) {
        self.center = rgb;
        self.text = ca::hex(rgb);
    }
}

pub struct ColorAnalyzerDialog {
    doc: DocId,
    visual: Visual,
    preview: Option<egui::ColorImage>,
    tex: Option<TextureHandle>,
    pub prefs: Prefs,
    pub rows: Vec<Row>,
    /// Selected patch: row index and patch number (1 = top left).
    pub selected: Option<(usize, usize)>,
    /// The eyedropper is on: a click on the visual adds its colour.
    picking: bool,
    /// The code typed in the "add" field.
    add_text: String,
    /// Scroll of the sheet preview (screen points) and its zoom (1 = the blocks fit the width).
    scroll: egui::Vec2,
    zoom: f32,
    pub status: String,
}

fn preview_image(v: &Visual) -> egui::ColorImage {
    let (w, h) = (v.width.max(1) as usize, v.height.max(1) as usize);
    let k = (f64::from(PREVIEW) / w.max(h) as f64).min(1.0);
    let (pw, ph) = (((w as f64 * k).round() as usize).max(1), ((h as f64 * k).round() as usize).max(1));
    let mut px = Vec::with_capacity(pw * ph);
    for y in 0..ph {
        let sy = ((y as f64 + 0.5) / k) as usize;
        for x in 0..pw {
            let sx = ((x as f64 + 0.5) / k) as usize;
            let i = (sy.min(h - 1) * w + sx.min(w - 1)) * 4;
            px.push(v.rgba.get(i..i + 4).map_or(Color32::TRANSPARENT, |p| Color32::from_rgba_unmultiplied(p[0], p[1], p[2], p[3])));
        }
    }
    egui::ColorImage::new([pw, ph], px)
}

fn color32(rgb: [u8; 3]) -> Color32 {
    Color32::from_rgb(rgb[0], rgb[1], rgb[2])
}

/// Axis names in the UI language.
fn axis_name(a: Axis) -> &'static str {
    match a {
        Axis::Lightness => tl!("Lightness"),
        Axis::Saturation => tl!("Saturation"),
        Axis::Hue => tl!("Hue"),
        Axis::Red => tl!("Red"),
        Axis::Green => tl!("Green"),
        Axis::Blue => tl!("Blue"),
    }
}

impl ColorAnalyzerDialog {
    /// The dialog on document `doc`, its colours extracted.
    pub fn new(doc: DocId, visual: Visual) -> Self {
        // Tests start from the defaults (they run in parallel).
        let prefs = if cfg!(test) { Prefs::default() } else { LAST_PREFS.lock().ok().and_then(|p| *p).unwrap_or_default() };
        let mut d = ColorAnalyzerDialog {
            doc,
            preview: Some(preview_image(&visual)),
            visual,
            tex: None,
            prefs,
            rows: Vec::new(),
            selected: None,
            picking: false,
            add_text: String::new(),
            scroll: egui::Vec2::ZERO,
            zoom: 1.0,
            status: String::new(),
        };
        d.reanalyze();
        d
    }

    /// The colours extracted again with the current settings (edited codes are lost).
    pub fn reanalyze(&mut self) {
        let found = cmd::analyze(&self.visual, self.prefs.max_colors, f64::from(self.prefs.merge));
        self.rows = found.iter().map(|s| Row::new(s.rgb, Some(s.share))).collect();
        self.selected = None;
        self.status = if self.rows.is_empty() { tl!("No colour found: the image is empty or transparent").to_string() } else { String::new() };
    }

    fn set_prefs(&mut self, prefs: Prefs) {
        if prefs == self.prefs {
            return;
        }
        let reanalyze = prefs.max_colors != self.prefs.max_colors || prefs.merge != self.prefs.merge;
        self.prefs = prefs;
        if !cfg!(test)
            && let Ok(mut last) = LAST_PREFS.lock()
        {
            *last = Some(prefs);
        }
        if reanalyze {
            self.reanalyze();
        }
        self.selected = self.selected.filter(|(_, n)| *n <= prefs.grid * prefs.grid);
    }

    pub fn board(&self) -> Board {
        let d = Board::default();
        Board { width: f64::from(self.prefs.width_cm) * 10.0, grid: self.prefs.grid, ..d }
    }

    /// Indices of the rows on the sheet, in order.
    pub fn included(&self) -> Vec<usize> {
        self.rows.iter().enumerate().filter(|(_, r)| r.include).map(|(i, _)| i).collect()
    }

    pub fn cells(&self, row: usize) -> Vec<Cell> {
        self.rows.get(row).map_or_else(Vec::new, |r| ca::grid(r.center, self.prefs.x, self.prefs.y, f64::from(self.prefs.step), self.prefs.grid))
    }

    pub fn selected_cell(&self) -> Option<(usize, Cell)> {
        let (row, n) = self.selected?;
        self.cells(row).into_iter().find(|c| c.number == n).map(|c| (row, c))
    }

    /// Adds a colour (picked or typed) to the list; false when the list is full.
    pub fn add(&mut self, rgb: [u8; 3]) -> bool {
        if self.rows.len() >= cmd::MAX_COLORS {
            self.status = tl!("The sheet takes at most 32 colours").to_string();
            return false;
        }
        self.rows.push(Row::new(rgb, None));
        true
    }

    /// Adds the visual's colour at (`x`, `y`) (visual pixels).
    pub fn pick(&mut self, x: i64, y: i64) -> bool {
        match self.visual.sample(x, y, 1) {
            Some(rgb) => self.add(rgb),
            None => {
                self.status = tl!("Transparent here: pick a coloured spot").to_string();
                false
            }
        }
    }

    pub fn remove(&mut self, i: usize) {
        if i < self.rows.len() {
            self.rows.remove(i);
            self.selected = None;
        }
    }

    /// The selected patch becomes the centre of its colour.
    pub fn recenter(&mut self) -> bool {
        let Some((row, cell)) = self.selected_cell() else { return false };
        if let Some(r) = self.rows.get_mut(row) {
            r.set_center(cell.rgb);
        }
        let mid = self.prefs.grid * self.prefs.grid / 2 + 1;
        self.selected = Some((row, mid));
        true
    }

    /// The codes as text: one line per colour, for a job sheet or an e-mail.
    pub fn table(&self) -> String {
        let mut out = String::new();
        for (i, r) in self.rows.iter().enumerate() {
            let share = r.share.map(|s| format!("  ({})", cmd::share_text(s))).unwrap_or_default();
            let line = if r.center == r.original {
                format!("{} {} : {}{share}", tl!("Color"), i + 1, ca::hex(r.center))
            } else {
                format!("{} {} : {} → {}{share}", tl!("Color"), i + 1, ca::hex(r.original), ca::hex(r.center))
            };
            out.push_str(&line);
            out.push('\n');
        }
        out
    }

    /// `dtf.colorAnalyzer`'s params for this sheet.
    pub fn params(&self, app: &PhotocraftApp) -> Result<Value, String> {
        let index = app.session.documents().iter().position(|d| d.doc.id == self.doc).ok_or_else(|| tl!("the document was closed").to_string())?;
        let colors: Vec<Value> = self
            .rows
            .iter()
            .filter(|r| r.include)
            .map(|r| {
                let mut v = json!({"hex": ca::hex(r.center), "original": ca::hex(r.original)});
                if let Some(s) = r.share {
                    v["share"] = json!(s);
                }
                v
            })
            .collect();
        if colors.is_empty() {
            return Err(tl!("No colour on the sheet").to_string());
        }
        Ok(json!({
            "document": index,
            "colors": colors,
            "axisX": self.prefs.x.id(),
            "axisY": self.prefs.y.id(),
            "step": self.prefs.step,
            "grid": self.prefs.grid,
            "width": self.prefs.width_cm,
            "resolution": cmd::DEFAULT_RESOLUTION,
        }))
    }

    pub fn describe(&self) -> Value {
        json!({
            "maxColors": self.prefs.max_colors,
            "merge": self.prefs.merge,
            "axisX": self.prefs.x.id(),
            "axisY": self.prefs.y.id(),
            "step": self.prefs.step,
            "grid": self.prefs.grid,
            "width": self.prefs.width_cm,
            "colors": self.rows.iter().map(|r| json!({"original": ca::hex(r.original), "hex": ca::hex(r.center), "share": r.share, "include": r.include})).collect::<Vec<_>>(),
            "selected": self.selected_cell().map(|(row, c)| json!({"color": row, "cell": c.number, "hex": ca::hex(c.rgb), "offsets": ca::offset_label(self.prefs.x, self.prefs.y, f64::from(self.prefs.step), c.dx, c.dy)})),
            "heightCm": (self.board().height(self.included().len()) * 10.0).round() / 100.0,
        })
    }
}

/// Opens the dialog on the active document (kept as it is when already open).
pub fn open(app: &mut PhotocraftApp) -> Result<(), String> {
    if app.color_analyzer.is_some() {
        return Ok(());
    }
    let st = app.session.active().ok_or(tl!("Open a document first"))?;
    let v = cmd::load_document(&st.doc).map_err(|e| e.to_string())?;
    app.color_analyzer = Some(ColorAnalyzerDialog::new(st.doc.id, v));
    Ok(())
}

/// Runs the command, writing the sheet to `path` and opening it; the dialog closes on success.
fn export_to(app: &mut PhotocraftApp, path: &str) -> Result<Value, String> {
    let d = app.color_analyzer.as_ref().ok_or("Color Analyzer isn't open")?;
    let mut params = d.params(app)?;
    params["output"] = json!(path);
    match app.run(cmd::COLOR_ANALYZER, params) {
        Ok(v) => {
            app.color_analyzer = None;
            Ok(v)
        }
        Err(e) => {
            if let Some(d) = app.color_analyzer.as_mut() {
                d.status = e.clone();
            }
            Err(e)
        }
    }
}

fn export(app: &mut PhotocraftApp) {
    let Some(d) = app.color_analyzer.as_ref() else { return };
    if let Err(e) = d.params(app) {
        if let Some(d) = app.color_analyzer.as_mut() {
            d.status = e;
        }
        return;
    }
    let r = app.pick_save("Planche couleurs.png", |app, path| export_to(app, &path));
    if let Err(e) = r
        && e != crate::file_dialog::CANCELLED
        && let Some(d) = app.color_analyzer.as_mut()
    {
        d.status = e;
    }
}

fn close(app: &mut PhotocraftApp) {
    app.color_analyzer = None;
}

/// Menu / control routing. `Some` when this module handles the invocation.
pub fn menu(app: &mut PhotocraftApp, _ctx: &egui::Context, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != cmd::COLOR_ANALYZER {
        return None;
    }
    if params.as_object().is_none_or(|o| o.is_empty()) {
        return Some(open(app).map(|()| app.color_analyzer.as_ref().map(ColorAnalyzerDialog::describe).unwrap_or(Value::Null)));
    }
    let ui = params.get("ui")?;
    if let Err(e) = open(app) {
        return Some(Err(e));
    }
    Some(control(app, ui))
}

fn control(app: &mut PhotocraftApp, ui: &Value) -> Result<Value, String> {
    if ui.get("cancel").and_then(Value::as_bool) == Some(true) {
        close(app);
        return Ok(json!({"cancelled": true}));
    }
    if let Some(path) = ui.get("export").and_then(Value::as_str) {
        return export_to(app, path);
    }
    let d = app.color_analyzer.as_mut().ok_or("Color Analyzer isn't open")?;
    if let Some(set) = ui.get("set") {
        let mut p = d.prefs;
        let f = |k: &str| set.get(k).and_then(Value::as_f64).filter(|f| f.is_finite());
        if let Some(v) = f("maxColors") {
            p.max_colors = (v.round() as usize).clamp(1, cmd::MAX_COLORS);
        }
        if let Some(v) = f("merge") {
            p.merge = (v as f32).clamp(5.0, 200.0);
        }
        if let Some(v) = f("step") {
            p.step = (v as f32).clamp(0.5, 50.0);
        }
        if let Some(v) = f("grid") {
            p.grid = ((v.round() as usize).clamp(3, 7)) | 1;
        }
        if let Some(v) = f("width") {
            p.width_cm = (v as f32).clamp(10.0, 300.0);
        }
        let axis = |k: &str| set.get(k).and_then(Value::as_str).map(|s| Axis::from_id(s).ok_or_else(|| format!("unknown axis `{s}`"))).transpose();
        if let Some(a) = axis("axisX")? {
            p.x = a;
        }
        if let Some(a) = axis("axisY")? {
            p.y = a;
        }
        if p.x == p.y {
            return Err("the two axes must differ".into());
        }
        d.set_prefs(p);
    }
    if ui.get("reanalyze").and_then(Value::as_bool) == Some(true) {
        d.reanalyze();
    }
    if let Some(pk) = ui.get("pick") {
        let (Some(x), Some(y)) = (pk.get("x").and_then(Value::as_i64), pk.get("y").and_then(Value::as_i64)) else {
            return Err("`pick` needs x and y (pixels)".into());
        };
        if !d.pick(x, y) {
            return Err(d.status.clone());
        }
    }
    if let Some(hex) = ui.get("add").and_then(Value::as_str) {
        let rgb = ca::parse_hex(hex).ok_or_else(|| tl!("Not a colour code (#A43B2F)").to_string())?;
        if !d.add(rgb) {
            return Err(d.status.clone());
        }
    }
    if let Some(c) = ui.get("color") {
        let i = c.get("index").and_then(Value::as_u64).map(|v| v as usize).filter(|i| *i < d.rows.len()).ok_or("`color` needs a valid index")?;
        if let Some(hex) = c.get("hex").and_then(Value::as_str) {
            let rgb = ca::parse_hex(hex).ok_or_else(|| tl!("Not a colour code (#A43B2F)").to_string())?;
            if let Some(r) = d.rows.get_mut(i) {
                r.set_center(rgb);
            }
        }
        if let Some(on) = c.get("include").and_then(Value::as_bool)
            && let Some(r) = d.rows.get_mut(i)
        {
            r.include = on;
        }
    }
    if let Some(i) = ui.get("reset").and_then(Value::as_u64)
        && let Some(r) = d.rows.get_mut(i as usize)
    {
        let o = r.original;
        r.set_center(o);
    }
    if let Some(i) = ui.get("remove").and_then(Value::as_u64) {
        d.remove(i as usize);
    }
    if let Some(sel) = ui.get("select") {
        let row = sel.get("color").and_then(Value::as_u64).map(|v| v as usize).filter(|i| *i < d.rows.len());
        let cell = sel.get("cell").and_then(Value::as_u64).map(|v| v as usize).filter(|n| (1..=d.prefs.grid * d.prefs.grid).contains(n));
        d.selected = row.zip(cell);
    }
    if let Some(z) = ui.get("zoom").and_then(Value::as_f64).filter(|z| z.is_finite()) {
        d.zoom = (z as f32).clamp(1.0, 8.0);
    }
    if ui.get("recenter").and_then(Value::as_bool) == Some(true) && !d.recenter() {
        return Err(tl!("Select a patch first").to_string());
    }
    Ok(d.describe())
}

/// What the frame's widgets asked for, done once the dialog is no longer borrowed.
#[derive(Default)]
struct Actions {
    export: bool,
    cancel: bool,
    copy: Option<String>,
}

pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if app.color_analyzer.is_none() {
        return;
    }
    let t = Tokens::get(ctx);
    let screen = ctx.content_rect();
    let mut act = Actions::default();
    egui::Area::new(egui::Id::new("color-analyzer-dialog")).order(egui::Order::Foreground).fixed_pos(screen.min).show(ctx, |ui| {
        let Some(d) = app.color_analyzer.as_mut() else { return };
        if let Some(img) = d.preview.take() {
            d.tex = Some(ctx.load_texture("color-analyzer-visual", img, egui::TextureOptions::LINEAR));
        }
        let (full, _) = ui.allocate_exact_size(screen.size(), Sense::click());
        let painter = ui.painter().clone();
        painter.rect_filled(full, 0.0, t.chrome);
        let title = ERect::from_min_size(full.min, vec2(full.width(), 30.0));
        painter.rect_filled(title, 0.0, t.dock);
        painter.text(title.center(), Align2::CENTER_CENTER, tl!("Color Analyzer"), FontId::proportional(13.0), t.text);
        let body = ERect::from_min_max(pos2(full.left(), title.bottom()), pos2(full.right(), full.bottom() - FOOTER_H));
        let left = ERect::from_min_max(body.min, pos2(body.left() + LEFT_W, body.bottom()));
        let right = ERect::from_min_max(pos2(body.right() - RIGHT_W, body.top()), body.max);
        let view = ERect::from_min_max(pos2(left.right(), body.top()), pos2(right.left(), body.bottom())).shrink(12.0);
        sheet_view(ui, d, view, &t);
        colors_panel(ui, d, left, &t);
        settings_panel(ui, d, right, &t, &mut act);
        let foot = ERect::from_min_max(pos2(full.left(), full.bottom() - FOOTER_H), full.max);
        painter.rect_filled(foot, 0.0, t.dock);
        let mut fu = ui.new_child(egui::UiBuilder::new().max_rect(foot.shrink2(vec2(16.0, 9.0))).layout(egui::Layout::right_to_left(egui::Align::Center)));
        let any = d.rows.iter().any(|r| r.include);
        if let Some(role) = widgets::dialog_buttons(
            &mut fu,
            &[
                widgets::DialogButton::new(widgets::ButtonRole::Default, tl!("Export PNG…"), 120.0).enabled(any),
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
    if !ctx.text_edit_focused()
        && let Some(d) = app.color_analyzer.as_mut()
    {
        keys(d, ctx, &mut act);
    }
    if let Some(text) = act.copy {
        ctx.copy_text(text);
        if let Some(d) = app.color_analyzer.as_mut() {
            d.status = tl!("Copied").to_string();
        }
    }
    if act.export {
        export(app);
    }
    if act.cancel {
        close(app);
    }
}

/// Esc deselects, then closes; the arrows move the selection within its block.
fn keys(d: &mut ColorAnalyzerDialog, ctx: &egui::Context, act: &mut Actions) {
    use egui::Key;
    let pressed = ctx.input(|i| {
        let p = |k: Key| i.key_pressed(k);
        [p(Key::ArrowLeft), p(Key::ArrowRight), p(Key::ArrowUp), p(Key::ArrowDown), p(Key::Escape)]
    });
    let [left, right, up, down, esc] = pressed;
    if esc {
        if d.selected.is_some() {
            d.selected = None;
        } else if d.picking {
            d.picking = false;
        } else {
            act.cancel = true;
        }
        return;
    }
    let Some((row, n)) = d.selected else { return };
    let g = d.prefs.grid as i64;
    let (col, r) = ((n as i64 - 1) % g, (n as i64 - 1) / g);
    let (col, r) = ((col + i64::from(right) - i64::from(left)).clamp(0, g - 1), (r + i64::from(down) - i64::from(up)).clamp(0, g - 1));
    d.selected = Some((row, (r * g + col + 1) as usize));
}

fn colors_panel(ui: &mut egui::Ui, d: &mut ColorAnalyzerDialog, rect: ERect, t: &Tokens) {
    ui.painter().rect_filled(rect, 0.0, t.dock);
    ui.painter().line_segment([rect.right_top(), rect.right_bottom()], Stroke::new(1.0, t.separator));
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(12.0, 10.0))));
    panel.spacing_mut().item_spacing.y = 6.0;
    let w = rect.width() - 24.0;
    panel.set_max_width(w);
    widgets::section_label(&mut panel, tl!("Visual"));
    // The visual, fitted; with the eyedropper on, a click adds the colour under the pointer.
    let (area, resp) = panel.allocate_exact_size(vec2(w, VISUAL_H), Sense::click());
    widgets::checker(panel.painter(), area, 8.0);
    let (vw, vh) = (d.visual.width.max(1) as f32, d.visual.height.max(1) as f32);
    let k = (area.width() / vw).min(area.height() / vh);
    let img = ERect::from_center_size(area.center(), vec2(vw * k, vh * k));
    if let Some(tex) = &d.tex {
        panel.painter().image(tex.id(), img, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    panel.painter().rect_stroke(area, 0.0, Stroke::new(1.0, if d.picking { t.accent } else { t.separator }), StrokeKind::Inside);
    if d.picking && resp.hovered() {
        panel.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
    }
    if d.picking
        && resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
        && img.contains(p)
    {
        let (x, y) = (((p.x - img.left()) / k) as i64, ((p.y - img.top()) / k) as i64);
        d.pick(x, y);
    }
    let label = if d.picking { tl!("Eyedropper: on") } else { tl!("Eyedropper") };
    let r = if d.picking { widgets::primary_button(&mut panel, label, 120.0) } else { widgets::secondary_button(&mut panel, label, 120.0) };
    if r.clicked() {
        d.picking = !d.picking;
    }
    panel.add(egui::Label::new(egui::RichText::new(tl!("Click the visual to add a colour")).color(t.text_faint).size(11.0)).wrap());
    widgets::hairline(&mut panel);
    widgets::section_label(&mut panel, tl!("Colors"));
    let mut remove = None;
    let mut reset = None;
    egui::ScrollArea::vertical()
        .id_salt("color-analyzer-rows")
        .auto_shrink([false, false])
        .max_height((rect.bottom() - panel.cursor().top() - 70.0).max(80.0))
        .show(&mut panel, |ui| {
            for (i, r) in d.rows.iter_mut().enumerate() {
                ui.horizontal(|ui| {
                    widgets::checkbox(ui, &mut r.include, "");
                    let (sw, _) = ui.allocate_exact_size(vec2(26.0, 26.0), Sense::hover());
                    ui.painter().rect_filled(sw, 3.0, color32(r.center));
                    ui.painter().rect_stroke(sw, 3.0, Stroke::new(1.0, t.separator), StrokeKind::Inside);
                    ui.vertical(|ui| {
                        ui.horizontal(|ui| {
                            ui.label(egui::RichText::new(format!("{}", i + 1)).color(t.text_dim));
                            let id = ui.id().with(("ca-hex", i));
                            let resp = widgets::text_field_ex(ui, id, &mut r.text, "#A43B2F", 84.0, true, Some(7));
                            let parsed = ca::parse_hex(&r.text);
                            if resp.changed()
                                && let Some(rgb) = parsed
                            {
                                r.center = rgb;
                            }
                            if resp.lost_focus() {
                                r.text = ca::hex(r.center);
                            }
                            if parsed.is_none() {
                                ui.label(egui::RichText::new("!").color(t.danger));
                            }
                            if let Some(s) = r.share {
                                ui.label(egui::RichText::new(cmd::share_text(s)).color(t.text_dim).size(11.5));
                            }
                        });
                        if r.center != r.original {
                            ui.horizontal(|ui| {
                                let (o, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                                ui.painter().rect_filled(o, 2.0, color32(r.original));
                                ui.label(
                                    egui::RichText::new(crate::i18n::fmt(tl!("original {code}"), &[("code", &ca::hex(r.original))]))
                                        .color(t.text_faint)
                                        .size(11.0),
                                );
                            });
                        }
                        ui.horizontal(|ui| {
                            if r.center != r.original && widgets::secondary_button(ui, tl!("Restore"), 54.0).clicked() {
                                reset = Some(i);
                            }
                            if widgets::secondary_button(ui, tl!("Remove"), 54.0).clicked() {
                                remove = Some(i);
                            }
                        });
                    });
                });
                widgets::hairline(ui);
            }
        });
    if let Some(i) = reset
        && let Some(r) = d.rows.get_mut(i)
    {
        let o = r.original;
        r.set_center(o);
    }
    if let Some(i) = remove {
        d.remove(i);
    }
    panel.horizontal(|ui| {
        let id = ui.id().with("ca-add");
        let resp = widgets::text_field_ex(ui, id, &mut d.add_text, "#A43B2F", 100.0, true, Some(7));
        let enter = resp.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
        if widgets::secondary_button(ui, tl!("Add Code"), 100.0).clicked() || enter {
            match ca::parse_hex(&d.add_text) {
                Some(rgb) => {
                    if d.add(rgb) {
                        d.add_text.clear();
                    }
                }
                None => d.status = tl!("Not a colour code (#A43B2F)").to_string(),
            }
        }
    });
}

fn settings_panel(ui: &mut egui::Ui, d: &mut ColorAnalyzerDialog, rect: ERect, t: &Tokens, act: &mut Actions) {
    ui.painter().rect_filled(rect, 0.0, t.dock);
    ui.painter().line_segment([rect.left_top(), rect.left_bottom()], Stroke::new(1.0, t.separator));
    let mut panel = ui.new_child(egui::UiBuilder::new().max_rect(rect.shrink2(vec2(14.0, 10.0))));
    panel.spacing_mut().item_spacing.y = 6.0;
    let mut p = d.prefs;
    let row = |ui: &mut egui::Ui, label: &str, add: &mut dyn FnMut(&mut egui::Ui)| {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(120.0, 20.0), egui::Label::new(egui::RichText::new(label).color(t.text_dim)));
            add(ui);
        });
    };
    widgets::section_label(&mut panel, tl!("Analysis"));
    row(&mut panel, tl!("Colors"), &mut |ui| {
        let mut n = p.max_colors as f32;
        if widgets::value_field_prec(ui, &mut n, 1.0..=cmd::MAX_COLORS as f32, "", 60.0, 0).changed() {
            p.max_colors = (n.round() as usize).clamp(1, cmd::MAX_COLORS);
        }
    });
    row(&mut panel, tl!("Merge Close Shades"), &mut |ui| {
        widgets::value_field_prec(ui, &mut p.merge, 5.0..=200.0, "", 60.0, 0);
    });
    panel.label(egui::RichText::new(tl!("Higher merges more shades into fewer colours.")).color(t.text_faint).size(11.0));
    if widgets::secondary_button(&mut panel, tl!("Analyze Again"), 120.0).clicked() {
        d.reanalyze();
    }
    widgets::hairline(&mut panel);
    widgets::section_label(&mut panel, tl!("Variants"));
    let axes: Vec<(Axis, &str)> = Axis::ALL.iter().map(|a| (*a, axis_name(*a))).collect();
    let (old_x, old_y) = (p.x, p.y);
    row(&mut panel, tl!("Horizontal Axis"), &mut |ui| {
        widgets::dropdown(ui, "ca-axis-x", &mut p.x, &axes, 130.0);
    });
    row(&mut panel, tl!("Vertical Axis"), &mut |ui| {
        widgets::dropdown(ui, "ca-axis-y", &mut p.y, &axes, 130.0);
    });
    // The two axes always differ: picking the other one's swaps them.
    if p.x == p.y {
        if p.x != old_x {
            p.y = old_x;
        } else {
            p.x = old_y;
        }
    }
    row(&mut panel, tl!("Step"), &mut |ui| {
        widgets::value_field_prec(ui, &mut p.step, 0.5..=50.0, "", 60.0, 1);
    });
    panel.label(egui::RichText::new(tl!("Lightness and saturation: % ; hue: degrees ; red, green, blue: % of 255.")).color(t.text_faint).size(11.0));
    let grids = [(3usize, "3 × 3"), (5, "5 × 5"), (7, "7 × 7")];
    row(&mut panel, tl!("Block Size"), &mut |ui| {
        widgets::dropdown(ui, "ca-grid", &mut p.grid, &grids, 90.0);
    });
    widgets::hairline(&mut panel);
    widgets::section_label(&mut panel, tl!("Sheet"));
    row(&mut panel, tl!("Width"), &mut |ui| {
        widgets::value_field_prec(ui, &mut p.width_cm, 10.0..=300.0, "cm", 80.0, 1);
    });
    d.set_prefs(p);
    let n = d.included().len();
    let b = d.board();
    let fits = b.fits();
    panel.label(
        egui::RichText::new(crate::i18n::fmt(
            tl!("{w} × {h} cm, sRGB, 300 ppi, transparent background"),
            &[("w", &widgets::fmt_num_prec(b.width / 10.0, 1)), ("h", &widgets::fmt_num_prec(b.height(n.max(1)) / 10.0, 1))],
        ))
        .color(t.text_dim)
        .size(11.5),
    );
    if !fits {
        panel.label(egui::RichText::new(tl!("Too narrow for one block of variants")).color(t.danger));
    }
    widgets::hairline(&mut panel);
    widgets::section_label(&mut panel, tl!("Selected Patch"));
    match d.selected_cell() {
        Some((row_i, c)) => {
            let (sw, _) = panel.allocate_exact_size(vec2(RIGHT_W - 30.0, 34.0), Sense::hover());
            panel.painter().rect_filled(sw, 3.0, color32(c.rgb));
            let offsets = if c.is_center() { tl!("centre of the block").to_string() } else { ca::offset_label(p.x, p.y, f64::from(p.step), c.dx, c.dy) };
            panel.label(egui::RichText::new(format!("{} {} · {} {} · {}", tl!("Color"), row_i + 1, tl!("patch"), c.number, ca::hex(c.rgb))).strong());
            panel.label(egui::RichText::new(offsets).color(t.text_dim).size(11.5));
            panel.horizontal_wrapped(|ui| {
                if widgets::primary_button(ui, tl!("Use as Centre"), 110.0).clicked() {
                    d.recenter();
                }
                if widgets::secondary_button(ui, tl!("Copy Code"), 90.0).clicked() {
                    act.copy = Some(ca::hex(c.rgb));
                }
            });
            panel.label(
                egui::RichText::new(tl!("Use as Centre: this patch becomes its colour's new centre, for a finer second sheet (lower the step)."))
                    .color(t.text_faint)
                    .size(11.0),
            );
        }
        None => {
            panel.label(egui::RichText::new(tl!("Click a patch on the sheet.")).color(t.text_faint));
        }
    }
    widgets::hairline(&mut panel);
    if widgets::secondary_button(&mut panel, tl!("Copy the Codes"), 140.0).clicked() {
        act.copy = Some(d.table());
    }
    widgets::hairline(&mut panel);
    panel.horizontal_wrapped(|ui| {
        ui.label(egui::RichText::new(tl!("Zoom")).color(t.text_dim));
        if widgets::secondary_button(ui, "−", 28.0).clicked() {
            d.zoom = (d.zoom / 1.25).max(1.0);
        }
        if widgets::secondary_button(ui, tl!("Fit on Screen"), 90.0).clicked() {
            d.zoom = 1.0;
            d.scroll = egui::Vec2::ZERO;
        }
        if widgets::secondary_button(ui, "+", 28.0).clicked() {
            d.zoom = (d.zoom * 1.25).min(8.0);
        }
    });
}

/// Which patch (row index, patch number) is at `p`, for the sheet drawn from `origin` at `s`
/// points per millimetre.
fn hit(d: &ColorAnalyzerDialog, b: &Board, origin: Pos2, s: f32, p: Pos2) -> Option<(usize, usize)> {
    let (mx, my) = (f64::from((p.x - origin.x) / s), f64::from((p.y - origin.y) / s));
    for (k, row) in d.included().into_iter().enumerate() {
        let o = b.block_origin(k);
        for c in d.cells(row) {
            let (x, y, side) = b.patch_rect(o, c.col, c.row);
            if mx >= x && mx < x + side && my >= y && my < y + side {
                return Some((row, c.number));
            }
        }
    }
    None
}

/// The sheet as it will print: header, then one block per colour.
fn sheet_view(ui: &mut egui::Ui, d: &mut ColorAnalyzerDialog, view: ERect, t: &Tokens) {
    let painter = ui.painter_at(view);
    painter.rect_filled(view, 0.0, t.canvas);
    let b = d.board();
    let included = d.included();
    if included.is_empty() || !b.fits() {
        painter.text(view.center(), Align2::CENTER_CENTER, tl!("No colour on the sheet"), FontId::proportional(13.0), t.text_faint);
        return;
    }
    let (bw, bh) = (b.width as f32, b.height(included.len()) as f32);
    // Fit: the blocks in use across the view (a short list leaves the sheet's right side empty).
    let used = included.len().min(b.columns()) as f64;
    let content_w = (2.0 * ca::BOARD_MARGIN + used * b.block_size().0 + (used - 1.0).max(0.0) * ca::BLOCK_GAP).min(b.width) as f32;
    let fit = ((view.width() - 24.0) / content_w).max(0.05);
    let resp = ui.interact(view, ui.id().with("color-analyzer-sheet"), Sense::click());
    // Ctrl + wheel (or a pinch) zooms about the pointer; the wheel scrolls.
    if resp.hovered() {
        let (zoom_delta, scroll) = ui.input(|i| (i.zoom_delta(), i.smooth_scroll_delta));
        if (zoom_delta - 1.0).abs() > 1e-4 {
            let old = fit * d.zoom;
            d.zoom = (d.zoom * zoom_delta).clamp(1.0, 8.0);
            if let Some(p) = resp.hover_pos() {
                let k = fit * d.zoom / old;
                let o = view.min + vec2(12.0, 12.0) + d.scroll;
                d.scroll = (p - (p - o) * k) - view.min - vec2(12.0, 12.0);
            }
        } else {
            d.scroll += scroll;
        }
    }
    let s = fit * d.zoom;
    let size = vec2(bw * s, bh * s);
    let avail = view.size() - vec2(24.0, 24.0);
    let clamp_axis = |off: f32, content: f32, room: f32| if content <= room { 0.0 } else { off.clamp(room - content, 0.0) };
    d.scroll = vec2(clamp_axis(d.scroll.x, size.x, avail.x), clamp_axis(d.scroll.y, size.y, avail.y));
    let origin = view.min + vec2(12.0, 12.0) + d.scroll;
    let to = |x: f64, y: f64| pos2(origin.x + x as f32 * s, origin.y + y as f32 * s);
    let sheet = ERect::from_min_size(origin, size);
    widgets::checker(&painter, sheet, 8.0);
    painter.rect_stroke(sheet, 0.0, Stroke::new(1.0, t.separator), StrokeKind::Outside);
    // Text on the sheet: black, as printed, when large enough to read.
    let ink = Color32::BLACK;
    let mm_font = |mm: f32| FontId::proportional((mm * s).max(1.0));
    let m = ca::BOARD_MARGIN;
    if 4.0 * s >= 7.0 {
        painter.text(to(m, m + 6.0), Align2::LEFT_BOTTOM, format!("ANALYSEUR COULEURS — {}", d.visual.name), mm_font(4.9), ink);
        painter.text(
            to(m, m + 13.0),
            Align2::LEFT_BOTTOM,
            // As printed: in French.
            format!("Horizontal : {} (pas {}) · Vertical : {} (pas {})", d.prefs.x.name_fr(), d.prefs.step, d.prefs.y.name_fr(), d.prefs.step),
            mm_font(3.2),
            ink,
        );
    }
    if let Some(tex) = &d.tex {
        let (vw, vh) = (d.visual.width.max(1) as f32, d.visual.height.max(1) as f32);
        let k = (70.0 / vw).min(24.0 / vh);
        let (tw, th) = (vw * k, vh * k);
        let r = ERect::from_min_size(to(b.width - m - f64::from(tw), m), vec2(tw * s, th * s));
        painter.image(tex.id(), r, ERect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
    }
    let pointer = resp.hover_pos();
    let hovered = pointer.and_then(|p| hit(d, &b, origin, s, p));
    for (k, row) in included.iter().enumerate() {
        let o = b.block_origin(k);
        let Some(r) = d.rows.get(*row) else { continue };
        if 3.0 * s >= 6.0 {
            painter.text(to(o.0, o.1 + 4.5), Align2::LEFT_BOTTOM, format!("COULEUR {} — {}", row + 1, ca::hex(r.center)), mm_font(3.2), ink);
        }
        for c in d.cells(*row) {
            let (x, y, side) = b.patch_rect(o, c.col, c.row);
            let pr = ERect::from_min_max(to(x, y), to(x + side, y + side));
            if !pr.intersects(view) {
                continue;
            }
            painter.rect_filled(pr, 0.0, color32(c.rgb));
            if c.is_center() {
                painter.rect_stroke(pr, 0.0, Stroke::new((0.5 * s).max(1.5), ink), StrokeKind::Outside);
            }
            if d.selected == Some((*row, c.number)) {
                painter.rect_stroke(pr.expand((0.5 * s).max(1.5) + 1.0), 0.0, Stroke::new(2.5, t.accent), StrokeKind::Outside);
            } else if hovered == Some((*row, c.number)) {
                painter.rect_stroke(pr, 0.0, Stroke::new(1.5, t.accent.gamma_multiply(0.7)), StrokeKind::Outside);
            }
            if 2.1 * s >= 7.0 {
                painter.text(pos2(pr.center().x, pr.bottom() + 2.9 * s), Align2::CENTER_BOTTOM, format!("{} {}", c.number, ca::hex(c.rgb)), mm_font(2.1), ink);
                let label = if c.is_center() { "ORIGINE".to_string() } else { ca::offset_label(d.prefs.x, d.prefs.y, f64::from(d.prefs.step), c.dx, c.dy) };
                painter.text(pos2(pr.center().x, pr.bottom() + 5.5 * s), Align2::CENTER_BOTTOM, label, mm_font(2.1), ink);
            }
        }
    }
    if let Some((row, n)) = hovered
        && let Some(c) = d.cells(row).into_iter().find(|c| c.number == n)
    {
        let offsets =
            if c.is_center() { String::new() } else { format!(" · {}", ca::offset_label(d.prefs.x, d.prefs.y, f64::from(d.prefs.step), c.dx, c.dy)) };
        resp.clone().on_hover_text(format!("{} {} · {} {} · {}{offsets}", tl!("Color"), row + 1, tl!("patch"), n, ca::hex(c.rgb)));
    }
    if resp.clicked()
        && let Some(p) = resp.interact_pointer_pos()
    {
        d.selected = hit(d, &b, origin, s, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn invoke(app: &mut PhotocraftApp, ctx: &egui::Context, ui: Value) -> Value {
        menu(app, ctx, cmd::COLOR_ANALYZER, &json!({"ui": ui})).unwrap().unwrap()
    }

    fn app_with_two_colours() -> PhotocraftApp {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 100, "resolution": 300, "background": "transparent"})).unwrap();
        // Red larger than blue, so red comes first.
        app.run("select.rect", json!({"x": 0, "y": 0, "width": 120, "height": 100})).unwrap();
        app.run("edit.fill", json!({"color": "#c03020"})).unwrap();
        app.run("select.rect", json!({"x": 120, "y": 0, "width": 80, "height": 100})).unwrap();
        app.run("edit.fill", json!({"color": "#2040c0"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
        app
    }

    #[test]
    fn colours_are_found_edited_and_exported() {
        let mut app = app_with_two_colours();
        let ctx = egui::Context::default();
        let r = menu(&mut app, &ctx, cmd::COLOR_ANALYZER, &json!({})).unwrap().unwrap();
        assert_eq!(r["maxColors"], 6);
        assert_eq!(r["grid"], 5);
        assert_eq!(r["step"], 5.0);
        assert_eq!(r["colors"].as_array().unwrap().len(), 2);
        // Pick, type, edit, select, recentre.
        let r = invoke(&mut app, &ctx, json!({"pick": {"x": 150, "y": 50}, "add": "#ffcc00"}));
        assert_eq!(r["colors"].as_array().unwrap().len(), 4);
        assert_eq!(r["colors"][2]["hex"], "#2040C0");
        assert!(menu(&mut app, &ctx, cmd::COLOR_ANALYZER, &json!({"ui": {"add": "#zz"}})).unwrap().is_err());
        let r = invoke(&mut app, &ctx, json!({"remove": 3, "color": {"index": 2, "include": false}}));
        assert_eq!(r["colors"].as_array().unwrap().len(), 3);
        let r = invoke(&mut app, &ctx, json!({"select": {"color": 0, "cell": 14}}));
        let picked = r["selected"]["hex"].as_str().unwrap().to_string();
        assert_eq!(r["selected"]["offsets"], "L+5");
        let r = invoke(&mut app, &ctx, json!({"recenter": true}));
        assert_eq!(r["colors"][0]["hex"], picked.as_str());
        assert_eq!(r["colors"][0]["original"], "#C03020");
        assert_eq!(r["selected"]["cell"], 13);
        let r = invoke(&mut app, &ctx, json!({"reset": 0}));
        assert_eq!(r["colors"][0]["hex"], "#C03020");
        let r = invoke(&mut app, &ctx, json!({"color": {"index": 0, "hex": "#b04436"}}));
        assert_eq!(r["colors"][0]["hex"], "#B04436");
        let table = app.color_analyzer.as_ref().unwrap().table();
        assert!(table.contains("#C03020 → #B04436"), "{table}");
        // Axes must differ; settings are taken.
        assert!(menu(&mut app, &ctx, cmd::COLOR_ANALYZER, &json!({"ui": {"set": {"axisX": "saturation"}}})).unwrap().is_err());
        let r = invoke(&mut app, &ctx, json!({"set": {"axisX": "hue", "step": 3, "grid": 3, "width": 30}}));
        assert_eq!((r["axisX"].as_str(), r["grid"].as_u64()), (Some("hue"), Some(3)));
        // Export: the sheet is written and opened; only the included colours go on it.
        let dir = std::env::temp_dir().join(format!("ca-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let out = dir.join("planche.png").to_string_lossy().into_owned();
        let r = invoke(&mut app, &ctx, json!({"export": out}));
        assert_eq!(r["colors"].as_array().unwrap().len(), 2);
        assert_eq!(r["colors"][0]["hex"], "#B04436");
        assert!(app.color_analyzer.is_none(), "closed once exported");
        assert_eq!(app.session.documents().len(), 2);
        assert!(std::path::Path::new(&out).exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn re_analysis_follows_the_count_and_cancel_closes() {
        let mut app = app_with_two_colours();
        let ctx = egui::Context::default();
        let r = invoke(&mut app, &ctx, json!({"set": {"maxColors": 1}}));
        assert_eq!(r["colors"].as_array().unwrap().len(), 1);
        let r = invoke(&mut app, &ctx, json!({"set": {"maxColors": 6}}));
        assert_eq!(r["colors"].as_array().unwrap().len(), 2);
        invoke(&mut app, &ctx, json!({"cancel": true}));
        assert!(app.color_analyzer.is_none());
        // Params other than `ui` run the command itself.
        assert!(menu(&mut app, &ctx, cmd::COLOR_ANALYZER, &json!({"colors": [{"hex": "#000000"}]})).is_none());
    }

    #[test]
    fn the_dialog_draws() {
        let mut app = app_with_two_colours();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        invoke(&mut app, &ctx, json!({"select": {"color": 0, "cell": 1}}));
        ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx())).textures_delta.clear();
        assert!(app.color_analyzer.as_ref().is_some_and(|d| d.tex.is_some()));
    }
}
