// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF Craft's DTF tools as windows opened from the DTF menu (Print Conformity…, Remove White
//! Background…, …), one at a time, like DTF Halftone's: each shows the tool's settings, its help
//! and an Apply button; moving a setting previews the result on the canvas, through the
//! filter-dialog preview (`canvas::ensure_filter_preview`), until Apply, Cancel or closing it.
//!
//! The cut-out's window leaves the image clickable (Add Holes, the background eyedropper); the
//! others are modal, as a filter dialog is: the canvas and the other menu commands wait.

use std::collections::BTreeMap;

use egui::RichText;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

/// The preview key's dialog id for the panel (no dialog id is that large).
pub const PREVIEW_DIALOG: u64 = u64::MAX - 0x0D7F;

/// The tools, top to bottom. Control Background's tab also removes it
/// (`dtf.removeControlBackground`).
pub const TOOLS: [&str; 7] = [
    "dtf.conformity",
    "filter.dtf.removeWhiteBackground",
    "dtf.controlBackground",
    "filter.dtf.removeSemiTransparent",
    "dtf.defringe",
    "dtf.sharpen",
    "dtf.saturation",
];

/// The tabs' labels (and dock panel names), in [`TOOLS`] order.
pub const TABS: [&str; 7] = ["Conformity", "Cutout", "Control Background", "Transparent Pixels", "Remove Fringe", "Sharpness", "Saturation"];

/// Tools applied on Apply only: Print Conformity resizes the image.
const WITHOUT_PREVIEW: [&str; 1] = ["dtf.conformity"];

/// The retouching tabs: their preview starts as soon as the tab is opened (or clicked), and
/// "Show Before" hides it to compare.
const RETOUCH: [&str; 4] = ["dtf.sharpen", "dtf.saturation", "filter.dtf.removeSemiTransparent", "dtf.defringe"];

/// The Remove White Background preview's control colour, when none was picked.
pub const PREVIEW_COLOR: &str = "#231c1c";

/// The last colour picked in Control Background or the cut-out's Preview Background (remembered
/// with the preferences): the cut-out previews on it.
const LAST_COLOR_PREF: &str = "dtfCraft.lastColor";

fn last_color(app: &PhotocraftApp) -> Option<String> {
    app.session.prefs().dialogs.get(LAST_COLOR_PREF).and_then(Value::as_str).map(str::to_string)
}

fn set_last_color(app: &mut PhotocraftApp, hex: &str) {
    if last_color(app).as_deref() != Some(hex) {
        app.session.prefs.edit(|prefs| prefs.dialogs.insert(LAST_COLOR_PREF.into(), json!(hex)));
    }
}

/// A tab's own control's tooltip (`dtf_help::tip`).
fn help(response: egui::Response, cmd: &str, key: &str) -> egui::Response {
    match crate::dtf_help::tip(cmd, key) {
        Some(text) => response.on_hover_text(text),
        None => response,
    }
}

/// The DTF tabs' state. The settings live in `fields` while the app runs and are saved with the
/// dialogs' remembered values when applied.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DtfPanelUi {
    #[serde(skip)]
    pub fields: BTreeMap<String, Map<String, Value>>,
    /// The section whose settings are previewed on the canvas.
    #[serde(skip)]
    pub previewing: Option<String>,
    /// Print Conformity's sizes were read from this document state: (document, revision, trim).
    #[serde(skip)]
    conformity_source: Option<(photocraft_doc::DocId, u64, bool)>,
    /// The visual Print Conformity resizes, in px (trimmed when Trim is on).
    #[serde(skip)]
    visual_px: Option<(u32, u32)>,
    /// The pass each tool's tab was last drawn in (a tab shown again opens its preview).
    #[serde(skip)]
    drawn: BTreeMap<String, u64>,
    /// The first pass a DTF tab was drawn in (the tabs on screen at start don't preview).
    #[serde(skip)]
    first_pass: Option<u64>,
    /// The interior-hole Magic Eraser is on: the tool it replaced, given back when it goes off.
    #[serde(skip)]
    pub hole_eraser: Option<crate::state::Tool>,
    /// The cut-out's background eyedropper is on: the tool it replaced.
    #[serde(skip)]
    pub background_picker: Option<crate::state::Tool>,
    /// The cut-out's settings before its last change, for Toggle Last State (one level, until
    /// Apply or Cancel).
    #[serde(skip)]
    cutout_last: Option<Map<String, Value>>,
    /// The settings before a change still being dragged (a slider): the last state once released.
    #[serde(skip)]
    cutout_pending: Option<Map<String, Value>>,
    /// Whether a plain background was detected, for (document, revision, layer, tolerance × 10).
    #[serde(skip)]
    detected: Option<((photocraft_doc::DocId, u64, u64, i64), bool)>,
    /// The DTF tool whose window is open (its command id).
    #[serde(skip)]
    pub window: Option<String>,
}

/// The layout version DTF Craft last set up (remembered with the preferences): raising it
/// brings the DTF default layout back once, over a layout saved by an older version.
const LAYOUT_VERSION: u64 = 6;
const LAYOUT_PREF: &str = "dtfCraft.layoutVersion";

/// The DTF workspace (Window › Workspace › DTF): Layers alone in the right column (the DTF tools
/// open from the DTF menu); every other panel (Channels and Paths included) in the Window menu.
pub(crate) fn dtf_layout() -> craft_ui::docking::Layout<String> {
    use craft_ui::docking::{Layout, Node};
    Layout { root: Some(Node::Tabs { panels: vec!["layers".to_string()], active: 0 }), floating: Vec::new() }
}

/// DTF Craft's start: the DTF tools, and on the first launch of this layout the DTF workspace.
pub fn dtf_craft_startup(app: &mut PhotocraftApp) {
    app.dtf_craft = true;
    // The DTF tools are windows of the DTF menu: never a dock group on the right.
    app.ui.panels.dtf = false;
    let seen = app.session.prefs().dialogs.get(LAYOUT_PREF).and_then(Value::as_u64).unwrap_or(0);
    if seen < LAYOUT_VERSION {
        app.ui.workspace = "DTF".into();
        crate::menus::apply_workspace(app);
        app.session.prefs.edit(|prefs| prefs.dialogs.insert(LAYOUT_PREF.into(), json!(LAYOUT_VERSION)));
    }
}

/// A tool's settings: schema defaults, then the values last applied (from the panel or its dialog).
fn initial_fields(app: &PhotocraftApp, command: &str) -> Map<String, Value> {
    let mut f = Map::new();
    f.insert("__command".into(), json!(command));
    f.insert("__filter".into(), json!(true));
    let Some(spec) = photocraft_engine::commands::find(command) else { return f };
    let params = crate::filter_dialog::parse_spec(spec.params);
    for p in &params {
        if let Some(v) = crate::filter_dialog::default_value(app, command, p) {
            f.insert(p.key.clone(), v);
        }
    }
    if let Some(Value::Object(saved)) = app.session.prefs().dialogs.get(command) {
        // A remembered value of another type (a name saved as a number) is left out.
        let same_kind = |a: &Value, b: &Value| std::mem::discriminant(a) == std::mem::discriminant(b) || (a.is_number() && b.is_number());
        for p in &params {
            if let Some(v) = saved.get(&p.key)
                && f.get(&p.key).is_none_or(|d| same_kind(d, v))
            {
                f.insert(p.key.clone(), v.clone());
            }
        }
        // Remove White Background's preview colour is the panel's own setting.
        if let Some(c) = saved.get("previewColor").filter(|v| v.is_string()) {
            f.insert("previewColor".into(), c.clone());
        }
    }
    f
}

/// Is a section previewed on the canvas now? Gives the preview's dialog id and fields to
/// `canvas::ensure_filter_preview` when no command dialog is open.
pub(crate) fn preview_source(app: &PhotocraftApp) -> Option<(u64, Map<String, Value>)> {
    let cmd = app.ui.dtf_panel.previewing.as_ref()?;
    // Only while the tool's window is open.
    if app.ui.dtf_panel.window.as_ref() != Some(cmd) {
        return None;
    }
    let f = app.ui.dtf_panel.fields.get(cmd)?;
    // "Show Before": the canvas shows the image as it is.
    if f.get("__before").and_then(Value::as_bool) == Some(true) {
        return None;
    }
    Some((PREVIEW_DIALOG, f.clone()))
}

/// Stop the canvas preview (Cancel, Apply, another tool's tab, hiding the DTF tools).
pub(crate) fn stop_preview(app: &mut PhotocraftApp) {
    app.ui.dtf_panel.previewing = None;
    if app.filter_preview.as_ref().is_some_and(|p| p.key.dialog == PREVIEW_DIALOG && !p.committing) {
        app.filter_preview = None;
    }
}

/// Run a section's tool on the active document with its settings.
fn apply(app: &mut PhotocraftApp, command: &str) {
    let Some(f) = app.ui.dtf_panel.fields.get(command).cloned() else { return };
    let mut params = app.with_mask_target(command, crate::filter_dialog::params_of(&f));
    if command == "dtf.conformity" {
        conformity_params(&f, &mut params);
    }
    // The preview colour is the panel's: the command doesn't take it.
    let preview_color = params.as_object_mut().and_then(|o| o.remove("previewColor"));
    let previewed = app.ui.dtf_panel.previewing.as_deref() == Some(command);
    app.ui.dtf_panel.previewing = None;
    if command == "dtf.controlBackground"
        && let Some(hex) = params.get("color").and_then(Value::as_str).map(str::to_string)
    {
        set_last_color(app, &hex);
    }
    let result = app.run(command, params.clone());
    // A filter that runs as a background job keeps its preview until the job lands, as when a
    // dialog is confirmed (`dialogs::confirm`).
    let running = app
        .session
        .active()
        .is_some_and(|st| app.filter_preview.as_ref().is_some_and(|p| p.key.doc == st.doc.id) && app.session.job_on(st.doc.id).is_some());
    match app.filter_preview.as_mut() {
        Some(p) if result.is_ok() && running && previewed && p.key.dialog == PREVIEW_DIALOG => p.committing = true,
        Some(p) if p.key.dialog == PREVIEW_DIALOG => app.filter_preview = None,
        _ => {}
    }
    match result {
        Ok(_) => {
            // Remembered for the next session (and the DTF menu dialog), as a dialog's OK does.
            if let Value::Object(saved) = params
                && !saved.is_empty()
            {
                // The holes belong to this image: not remembered, and cut now.
                let mut saved: Map<String, Value> = saved.into_iter().filter(|(k, _)| k != "layer" && k != "channel" && k != HOLES).collect();
                if let Some(f) = app.ui.dtf_panel.fields.get_mut(command) {
                    f.remove(HOLES);
                }
                if let Some(c) = preview_color {
                    saved.insert("previewColor".into(), c);
                }
                app.session.prefs.edit(|prefs| prefs.dialogs.insert(command.into(), Value::Object(saved)));
            }
        }
        Err(error) => {
            let error = crate::i18n::t(&error).to_string();
            crate::notices::error(app, error);
        }
    }
}

/// A colour row (Control Background's colour, the cut-out's preview colour): a swatch with egui's
/// colour picker and the hex value, stored as `#rrggbb` under `key`.
fn color_row(ui: &mut egui::Ui, f: &mut Map<String, Value>, key: &str, label: &str) -> bool {
    let t = Tokens::get(ui.ctx());
    let hex = f.get(key).and_then(Value::as_str).unwrap_or(PREVIEW_COLOR).to_string();
    let rgb = crate::color_picker_ui::parse_hex(&hex).unwrap_or([0.137, 0.11, 0.11]);
    let mut bytes = rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(label).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if egui::color_picker::color_edit_button_srgb(ui, &mut bytes).changed() {
                changed = true;
            }
            ui.label(RichText::new(&hex).color(t.text_dim).monospace());
        });
    });
    if changed {
        f.insert(key.into(), json!(format!("#{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2])));
    }
    changed
}

/// Print Conformity's visual, in px: the document, or its non-transparent bounds when Trim is on
/// (as `image.trim` finds them).
fn visual_size(doc: &photocraft_doc::Document, trim: bool) -> (u32, u32) {
    let (w, h) = (doc.size.width, doc.size.height);
    if !trim {
        return (w, h);
    }
    let buf = photocraft_compose::flatten(doc);
    let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
    for (i, q) in buf.px.iter().enumerate() {
        if q[3] > 0.0 {
            let (x, y) = ((i % w as usize) as u32, (i / w as usize) as u32);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1));
        }
    }
    if x1 <= x0 || y1 <= y0 { (w, h) } else { (x1 - x0, y1 - y0) }
}

fn cm(px: u32, dpi: f64) -> f64 {
    f64::from(px) / dpi.max(1.0) * 2.54
}

fn round1(v: f64) -> f64 {
    (v * 10.0).round() / 10.0
}

/// Print Conformity starts from the visual as it is: its size in cm and the document's ppi. Read
/// again whenever the document changes; Trim switched keeps the size last typed.
fn sync_conformity(app: &mut PhotocraftApp, f: &mut Map<String, Value>) {
    let Some(st) = app.session.active() else { return };
    let trim = f.get("trim").and_then(Value::as_bool).unwrap_or(true);
    let key = (st.doc.id, st.revision, trim);
    let panel = &app.ui.dtf_panel;
    if panel.conformity_source == Some(key) {
        return;
    }
    let same_doc = panel.conformity_source.is_some_and(|(d, r, _)| d == key.0 && r == key.1);
    let (w, h) = visual_size(&st.doc, trim);
    let dpi = f64::from(st.doc.resolution_dpi).max(1.0);
    if same_doc && f.contains_key("__lastDim") {
        // Trim switched: the size typed stays, the other side follows the new visual.
        let (wc, hc) = (f.get("width").and_then(Value::as_f64).unwrap_or(0.0), f.get("height").and_then(Value::as_f64).unwrap_or(0.0));
        if f.get("__lastDim").and_then(Value::as_str) == Some("height") {
            f.insert("width".into(), json!(round1(hc * f64::from(w) / f64::from(h.max(1)))));
        } else {
            f.insert("height".into(), json!(round1(wc * f64::from(h) / f64::from(w.max(1)))));
        }
    } else {
        f.insert("width".into(), json!(round1(cm(w, dpi))));
        f.insert("height".into(), json!(round1(cm(h, dpi))));
        f.insert("resolution".into(), json!(dpi.round()));
        f.remove("__lastDim");
    }
    let panel = &mut app.ui.dtf_panel;
    panel.conformity_source = Some(key);
    panel.visual_px = Some((w, h));
}

/// Width and height stay in proportion: the side changed sets the other. A new resolution keeps
/// the pixels: the print size in cm follows it.
fn link_dimensions(app: &PhotocraftApp, before: &Map<String, Value>, f: &mut Map<String, Value>) {
    let Some((w, h)) = app.ui.dtf_panel.visual_px else { return };
    let get = |m: &Map<String, Value>, k: &str| m.get(k).and_then(Value::as_f64).unwrap_or(0.0);
    let (bw, bh, nw, nh) = (get(before, "width"), get(before, "height"), get(f, "width"), get(f, "height"));
    let ratio = f64::from(h) / f64::from(w.max(1));
    let dpi = get(f, "resolution");
    if (dpi - get(before, "resolution")).abs() > 1e-9 && dpi > 0.0 {
        f.insert("width".into(), json!(round1(cm(w, dpi))));
        f.insert("height".into(), json!(round1(cm(h, dpi))));
        f.remove("__lastDim");
    } else if (nw - bw).abs() > 1e-9 {
        f.insert("height".into(), json!(round1(nw * ratio)));
        f.insert("__lastDim".into(), json!("width"));
    } else if (nh - bh).abs() > 1e-9 {
        f.insert("width".into(), json!(round1(nh / ratio)));
        f.insert("__lastDim".into(), json!("height"));
    }
}

/// Print Conformity's command: the side typed last (the other follows the proportions); sizes left
/// as read keep the pixels, a new ppi included (only the print size changes).
fn conformity_params(f: &Map<String, Value>, params: &mut Value) {
    let Some(o) = params.as_object_mut() else { return };
    match f.get("__lastDim").and_then(Value::as_str) {
        Some("height") => {
            o.insert("width".into(), json!(0));
        }
        Some(_) => {
            o.insert("height".into(), json!(0));
        }
        None => {
            o.insert("width".into(), json!(0));
            o.insert("height".into(), json!(0));
        }
    }
}

/// Print Conformity's AI Upscale buttons: ×2, ×3, ×4 of the document, each with the size it gives
/// at the tab's resolution. A click enlarges a copy (`dtf.upscale`) at that resolution.
fn upscale_buttons(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &Map<String, Value>) -> bool {
    let Some(st) = app.session.active() else { return false };
    let (w, h) = (st.doc.size.width, st.doc.size.height);
    let dpi = f.get("resolution").and_then(Value::as_f64).unwrap_or(f64::from(st.doc.resolution_dpi)).clamp(72.0, 1200.0);
    let enabled = app.session.is_enabled(photocraft_engine::dtf_cmds::UPSCALE) && app.session.job_on(st.doc.id).is_none();
    let t = Tokens::get(ui.ctx());
    ui.add_space(8.0);
    ui.label(RichText::new(tl!("AI Upscale")).color(t.text_dim));
    let mut run = None;
    for k in [2u32, 3, 4] {
        let (pw, ph) = (w * k, h * k);
        let text = crate::i18n::fmt(
            tl!("×{scale}: {width} × {height} cm ({pxw} × {pxh} px) at {resolution} ppi"),
            &[
                ("scale", &k.to_string()),
                ("width", &format!("{:.1}", cm(pw, dpi))),
                ("height", &format!("{:.1}", cm(ph, dpi))),
                ("pxw", &pw.to_string()),
                ("pxh", &ph.to_string()),
                ("resolution", &format!("{dpi:.0}")),
            ],
        );
        if help(ui.add_enabled(enabled, egui::Button::new(text)), "dtf.conformity", "upscale").clicked() {
            run = Some(k);
        }
    }
    if let Some(k) = run
        && let Err(error) = app.run(photocraft_engine::dtf_cmds::UPSCALE, json!({"scale": k.to_string(), "resolution": dpi.round()}))
    {
        crate::notices::error(app, error);
    }
    run.is_some()
}

/// The cut-out's settings without the panel's own keys (`__…`): what Toggle Last State compares
/// and restores.
fn cutout_state(f: &Map<String, Value>) -> Map<String, Value> {
    f.iter().filter(|(k, _)| !k.starts_with("__")).map(|(k, v)| (k.clone(), v.clone())).collect()
}

/// The cut-out's settings changed from `before`: it becomes the last state (once a drag ends).
fn record_cutout(app: &mut PhotocraftApp, before: Map<String, Value>, dragging: bool) {
    let p = &mut app.ui.dtf_panel;
    if dragging {
        p.cutout_pending.get_or_insert(before);
    } else {
        p.cutout_last = Some(p.cutout_pending.take().unwrap_or(before));
    }
}

/// Forget the cut-out's last state (Apply, Cancel).
fn clear_cutout_history(app: &mut PhotocraftApp) {
    app.ui.dtf_panel.cutout_last = None;
    app.ui.dtf_panel.cutout_pending = None;
}

/// Is Edit › Toggle Last State the cut-out's (its preview on screen, a last state kept)?
fn cutout_toggles(app: &PhotocraftApp) -> bool {
    app.ui.dtf_panel.previewing.as_deref() == Some(photocraft_engine::dtf_cmds::REMOVE_WHITE) && app.ui.dtf_panel.cutout_last.is_some()
}

/// Edit › Toggle Last State (Ctrl+Alt+Z) while the cut-out is previewed, before Apply: its
/// settings go back to their last state (a hole added by mistake goes), and again forward. One
/// level, as for the document. `None` otherwise: the document's history.
pub(crate) fn intercept(app: &mut PhotocraftApp, id: &str) -> Option<Result<Value, String>> {
    if id != "edit.toggleLastState" || !cutout_toggles(app) {
        return None;
    }
    let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
    let last = app.ui.dtf_panel.cutout_last.take()?;
    let current = app.ui.dtf_panel.fields.get(cmd).cloned().unwrap_or_default();
    // The panel's own keys (the help sheet open…) stay as they are.
    let mut restored: Map<String, Value> = current.iter().filter(|(k, _)| k.starts_with("__")).map(|(k, v)| (k.clone(), v.clone())).collect();
    restored.extend(last);
    app.ui.dtf_panel.cutout_last = Some(cutout_state(&current));
    app.ui.dtf_panel.fields.insert(cmd.into(), restored);
    Some(Ok(json!({"toggled": cmd})))
}

/// Toggle Last State's menu item while the cut-out owns it.
pub(crate) fn is_enabled(app: &PhotocraftApp, id: &str) -> Option<bool> {
    (id == "edit.toggleLastState" && cutout_toggles(app)).then_some(true)
}

/// While the cut-out is previewed with neither Add Holes nor the background eyedropper on, a
/// click on the visual does nothing (the Hand and Zoom tools still work).
/// A DTF window other than the cut-out's is modal: the canvas waits too.
pub(crate) fn blocks_canvas(app: &PhotocraftApp) -> bool {
    if matches!(app.ui.tool, crate::state::Tool::Hand | crate::state::Tool::Zoom) {
        return false;
    }
    if modal_window(app) {
        return true;
    }
    app.ui.dtf_panel.previewing.as_deref() == Some(photocraft_engine::dtf_cmds::REMOVE_WHITE)
        && app.ui.dtf_panel.hole_eraser.is_none()
        && app.ui.dtf_panel.background_picker.is_none()
}

/// Is a modal DTF window open (any but the cut-out's)?
pub(crate) fn modal_window(app: &PhotocraftApp) -> bool {
    app.ui.dtf_panel.window.as_deref().is_some_and(|w| w != photocraft_engine::dtf_cmds::REMOVE_WHITE)
}

/// DTF menu: a DTF tool opens its window (with no parameters; with parameters the command runs,
/// for automation). `None` for other commands.
pub fn menu(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if !TOOLS.contains(&id) || params.as_object().is_some_and(|o| !o.is_empty()) {
        return None;
    }
    open_window(app, id);
    Some(Ok(json!({"window": id})))
}

/// Opens tool `cmd`'s window, closing another DTF window first. The retouching tools and the
/// cut-out preview at once.
pub fn open_window(app: &mut PhotocraftApp, cmd: &str) {
    if app.ui.dtf_panel.window.as_deref() == Some(cmd) {
        return;
    }
    close_window(app);
    if !app.ui.dtf_panel.fields.contains_key(cmd) {
        let f = initial_fields(app, cmd);
        app.ui.dtf_panel.fields.insert(cmd.to_string(), f);
    }
    app.ui.dtf_panel.window = Some(cmd.to_string());
    let auto = RETOUCH.contains(&cmd) || cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE;
    if auto && app.session.active().is_some() && app.session.is_enabled(cmd) {
        app.ui.dtf_panel.previewing = Some(cmd.to_string());
    }
}

/// Closes the DTF window, as its Cancel does: the preview stops, the cut-out's holes and its
/// modes (Add Holes, eyedropper) go.
pub fn close_window(app: &mut PhotocraftApp) {
    let Some(cmd) = app.ui.dtf_panel.window.take() else { return };
    if app.ui.dtf_panel.previewing.as_deref() == Some(cmd.as_str()) {
        stop_preview(app);
    }
    if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE {
        if let Some(f) = app.ui.dtf_panel.fields.get_mut(&cmd) {
            f.remove(HOLES);
        }
        set_hole_eraser(app, false);
        set_background_picker(app, false);
        clear_cutout_history(app);
    }
}

/// No DTF tool on the right, whatever a saved layout or panel state says: the DTF dock group is
/// off and the old DTF tabs (`dtf…` panels) leave a docking layout. Run every frame (cheap).
pub fn keep_dtf_out_of_the_dock(app: &mut PhotocraftApp) {
    app.ui.panels.dtf = false;
    let Some(layout) = app.ui.docking.as_mut() else { return };
    let old: Vec<String> = layout.panels().into_iter().filter(|p| p.starts_with("dtf")).cloned().collect();
    for panel in old {
        let _ = layout.apply(craft_ui::docking::Action::Close { panel });
    }
}

/// The DTF window, when one is open: the tool's title, its settings (scrolled when taller than
/// the screen), closed by its × or Esc, and after Apply or Cancel.
pub fn window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    keep_dtf_out_of_the_dock(app);
    let Some(cmd) = app.ui.dtf_panel.window.clone() else { return };
    let Some(tab) = TOOLS.iter().position(|t| *t == cmd) else {
        app.ui.dtf_panel.window = None;
        return;
    };
    let mut open = true;
    let mut done = false;
    let max_h = (ctx.content_rect().height() - 120.0).max(240.0);
    // The window's title is the tool's name, as in the DTF menu.
    egui::Window::new(tl!(TABS[tab]))
        .id(egui::Id::new("dtf-tool-window"))
        .open(&mut open)
        .collapsible(false)
        .resizable(false)
        .default_width(340.0)
        .default_pos(ctx.content_rect().right_top() + egui::vec2(-380.0, 60.0))
        .show(ctx, |ui| {
            ui.set_width(330.0);
            // As tall as its settings, scrolled only past the screen's height.
            egui::ScrollArea::vertical().max_height(max_h).min_scrolled_height(max_h).auto_shrink([false, true]).show(ui, |ui| {
                done = tool_body(app, ui, tab);
            });
        });
    let escape = ctx.memory(|m| m.focused().is_none()) && ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape));
    if !open || escape {
        close_window(app);
    } else if done {
        // Applied (the preview became the result) or cancelled: the window closes.
        let cmd = app.ui.dtf_panel.window.take();
        if cmd.as_deref() == Some(photocraft_engine::dtf_cmds::REMOVE_WHITE) {
            set_hole_eraser(app, false);
            set_background_picker(app, false);
        }
    }
}

/// The cut-out's holes clicked by hand, kept in its settings (`holePoints`, document px): they
/// are part of its preview and are cut with the background on Apply.
const HOLES: &str = "holePoints";

fn holes(f: &Map<String, Value>) -> usize {
    f.get(HOLES).and_then(Value::as_array).map_or(0, Vec::len)
}

/// Turn Add Holes on or off (Remove White Background tab). On, the Magic Eraser's cursor is
/// active and a click on the canvas adds a hole to the cut-out ([`hole_click`]); the background
/// eyedropper goes off. Off, the previous tool comes back.
pub(crate) fn set_hole_eraser(app: &mut PhotocraftApp, on: bool) {
    match (on, app.ui.dtf_panel.hole_eraser) {
        (true, None) => {
            set_background_picker(app, false);
            app.ui.dtf_panel.hole_eraser = Some(app.ui.tool);
            app.ui.tool = crate::state::Tool::MagicEraser;
            app.ui.status = tl!("Click an enclosed area to make it transparent.").into();
        }
        (false, Some(previous)) => {
            app.ui.dtf_panel.hole_eraser = None;
            if app.ui.tool == crate::state::Tool::MagicEraser {
                app.ui.tool = previous;
            }
        }
        _ => {}
    }
}

/// Turn the cut-out's background eyedropper on or off. On, the Eyedropper's cursor is active and
/// a click on the canvas takes the colour to remove ([`pick_background`]); Add Holes goes off.
pub(crate) fn set_background_picker(app: &mut PhotocraftApp, on: bool) {
    match (on, app.ui.dtf_panel.background_picker) {
        (true, None) => {
            set_hole_eraser(app, false);
            app.ui.dtf_panel.background_picker = Some(app.ui.tool);
            app.ui.tool = crate::state::Tool::Eyedropper;
            app.ui.status = tl!("Click the background colour to remove.").into();
        }
        (false, Some(previous)) => {
            app.ui.dtf_panel.background_picker = None;
            if app.ui.tool == crate::state::Tool::Eyedropper {
                app.ui.tool = previous;
            }
        }
        _ => {}
    }
}

/// The cut-out's settings (from the panel, else as they start).
fn cutout_fields(app: &mut PhotocraftApp) -> Map<String, Value> {
    let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
    match app.ui.dtf_panel.fields.get(cmd) {
        Some(f) => f.clone(),
        None => initial_fields(app, cmd),
    }
}

/// A click with Add Holes on (`x`, `y` in document pixels): a hole is added to the cut-out's
/// preview, cut with the background on Apply. False when Add Holes is off.
pub(crate) fn hole_click(app: &mut PhotocraftApp, x: f64, y: f64) -> bool {
    if app.ui.dtf_panel.hole_eraser.is_none() {
        return false;
    }
    let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
    let mut f = cutout_fields(app);
    record_cutout(app, cutout_state(&f), false);
    let mut points = f.get(HOLES).and_then(Value::as_array).cloned().unwrap_or_default();
    if points.len() < photocraft_algo::dtf::MAX_HOLES {
        points.push(json!([x.floor(), y.floor()]));
    }
    f.insert(HOLES.into(), Value::Array(points));
    app.ui.dtf_panel.fields.insert(cmd.into(), f);
    app.ui.dtf_panel.previewing = Some(cmd.into());
    true
}

/// "Remove Last Hole": the last hole clicked leaves the preview.
fn remove_last_hole(f: &mut Map<String, Value>) {
    if let Some(Value::Array(points)) = f.get_mut(HOLES) {
        points.pop();
        if points.is_empty() {
            f.remove(HOLES);
        }
    }
}

/// A click with the background eyedropper on: the clicked pixel of the active layer becomes the
/// colour to remove, its detection goes off and the cut-out previews again; the eyedropper goes
/// off. False when it is off.
pub(crate) fn pick_background(app: &mut PhotocraftApp, x: f64, y: f64) -> bool {
    if app.ui.dtf_panel.background_picker.is_none() {
        return false;
    }
    let colour = app.session.active().and_then(|st| {
        let surf = st.doc.layer(st.active_layer?)?.surface()?;
        let px = surf.pixel(x.floor() as i32, y.floor() as i32);
        let rgba = photocraft_raster::to_rgba(&surf.format(), &px);
        (rgba[3] >= 0.5).then(|| crate::color_picker_ui::hex([rgba[0], rgba[1], rgba[2]]))
    });
    let Some(hex) = colour else {
        app.ui.status = tl!("Click a visible pixel of the background.").into();
        return true;
    };
    let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
    let mut f = cutout_fields(app);
    record_cutout(app, cutout_state(&f), false);
    f.insert("color".into(), json!(hex));
    f.insert("detectColor".into(), json!(false));
    app.ui.dtf_panel.fields.insert(cmd.into(), f);
    app.ui.dtf_panel.previewing = Some(cmd.into());
    set_background_picker(app, false);
    true
}

/// Is a plain background detected on the active layer (for the cut-out's warning)? Read once per
/// document state and tolerance.
fn background_detected(app: &mut PhotocraftApp, tolerance: f64) -> bool {
    let Some(st) = app.session.active() else { return true };
    let Some(layer) = st.active_layer else { return true };
    let key = (st.doc.id, st.revision, layer.0, (tolerance * 10.0).round() as i64);
    if let Some((k, found)) = app.ui.dtf_panel.detected
        && k == key
    {
        return found;
    }
    // A layer without pixels of its own (a smart object) is left to the command.
    let found = st.doc.layer(layer).is_none_or(|l| l.surface().is_none()) || photocraft_engine::dtf_cmds::detected_background(&st.doc, layer, tolerance).is_some();
    app.ui.dtf_panel.detected = Some((key, found));
    found
}

/// Print Conformity's note: the active document's print size now, in cm at its resolution.
fn current_size_note(app: &PhotocraftApp) -> Option<String> {
    let doc = &app.session.active()?.doc;
    let dpi = f64::from(doc.resolution_dpi).max(1.0);
    let cm = |px: u32| format!("{:.1}", f64::from(px) / dpi * 2.54);
    Some(crate::i18n::fmt(
        tl!("Current size: {width} × {height} cm at {resolution} ppi"),
        &[("width", &cm(doc.size.width)), ("height", &cm(doc.size.height)), ("resolution", &format!("{:.0}", dpi))],
    ))
}

/// A toggle button with an icon and a label, the panel's width, pressed while `on` (the
/// cut-out's Add Holes and background eyedropper, like the toolbar's tools).
fn mode_button(ui: &mut egui::Ui, icon: &str, label: &str, on: bool) -> egui::Response {
    let tint = if on { ui.visuals().selection.stroke.color } else { Tokens::get(ui.ctx()).text };
    let button = egui::Button::image_and_text(crate::icons::image(icon, 16.0, tint), RichText::new(label).color(tint))
        .selected(on)
        .min_size(egui::vec2(ui.available_width(), 26.0));
    ui.add(button)
}

/// One DTF tool's settings (`tab` indexes [`TOOLS`]), in its window. True when the window is
/// done: applied, cancelled, or an AI upscale started.
pub(crate) fn tool_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, tab: usize) -> bool {
    let Some(&cmd) = TOOLS.get(tab) else { return false };
    if !app.ui.dtf_panel.fields.contains_key(cmd) {
        let f = initial_fields(app, cmd);
        app.ui.dtf_panel.fields.insert(cmd.to_string(), f);
    }
    // The preview stays with the tool whose setting moved last, while its tab is on screen
    // (`preview_source`): drawing another DTF tab, in another group, leaves it be.
    let have_doc = app.session.active().is_some();
    let enabled = have_doc && app.session.is_enabled(cmd);
    let retouch = RETOUCH.contains(&cmd);
    // A retouching tab, or the cut-out's, opened (shown again after a pass without it) or clicked
    // previews at once.
    let auto_preview = retouch || cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE;
    let pass = ui.ctx().cumulative_pass_nr();
    let first = *app.ui.dtf_panel.first_pass.get_or_insert(pass);
    let last = app.ui.dtf_panel.drawn.insert(cmd.to_string(), pass);
    let opened = pass > first + 1 && last.is_none_or(|l| l + 1 < pass);
    let area = ui.max_rect().intersect(ui.clip_rect());
    let clicked = ui.input(|i| i.pointer.primary_pressed() && i.pointer.interact_pos().is_some_and(|p| area.contains(p)));
    if auto_preview && enabled && (opened || clicked) {
        app.ui.dtf_panel.previewing = Some(cmd.to_string());
    }
    let note = if cmd == "dtf.conformity" { current_size_note(app) } else { None };
    let Some(mut f) = app.ui.dtf_panel.fields.remove(cmd) else { return false };
    if cmd == "dtf.conformity" {
        sync_conformity(app, &mut f);
    }
    match note {
        Some(n) => f.insert("__note".into(), json!(n)),
        None => f.remove("__note"),
    };
    let (mut apply_it, mut cancel) = (false, false);
    ui.add_space(4.0);
    if !have_doc {
        ui.weak(tl!("Open a document to use the DTF settings."));
        ui.add_space(4.0);
    }
    ui.spacing_mut().item_spacing.y = 4.0;
    // The cut-out previews on the last colour picked (here or in Control Background).
    if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE {
        let color = last_color(app).or_else(|| app.ui.dtf_panel.fields.get("dtf.controlBackground").and_then(|c| c.get("color")).and_then(Value::as_str).map(str::to_string));
        f.insert("previewColor".into(), json!(color.unwrap_or_else(|| PREVIEW_COLOR.to_string())));
    }
    let before = f.clone();
    if cmd == "dtf.controlBackground" {
        crate::dtf_help::sheet_ui(ui, &mut f, cmd);
        let top = ui.cursor().top();
        if color_row(ui, &mut f, "color", &crate::filter_dialog::label("color"))
            && let Some(hex) = f.get("color").and_then(Value::as_str).map(str::to_string)
        {
            set_last_color(app, &hex);
        }
        crate::dtf_help::tip_over(ui, cmd, "color", top);
    } else {
        crate::filter_dialog::body(ui, &mut f);
    }
    let mut upscaled = false;
    if cmd == "dtf.conformity" {
        link_dimensions(app, &before, &mut f);
        if have_doc {
            upscaled = upscale_buttons(app, ui, &f);
        }
    }
    let mut eraser = app.ui.dtf_panel.hole_eraser.is_some();
    let mut picker = app.ui.dtf_panel.background_picker.is_some();
    if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE {
        // Detection on and nothing plain on the edges: nothing would be removed.
        let detect = f.get("detectColor").and_then(Value::as_bool).unwrap_or(true);
        let tolerance = f.get("tolerance").and_then(Value::as_f64).unwrap_or(10.0);
        if have_doc && detect && holes(&f) == 0 && !background_detected(app, tolerance) {
            ui.label(RichText::new(tl!("No plain background detected: the background is already transparent, or pick its colour with the eyedropper.")).color(Tokens::get(ui.ctx()).warning));
        }
        // The control colour the cut-out previews on (the garment's).
        let top = ui.cursor().top();
        if color_row(ui, &mut f, "previewColor", tl!("Preview Background"))
            && let Some(hex) = f.get("previewColor").and_then(Value::as_str).map(str::to_string)
        {
            set_last_color(app, &hex);
        }
        crate::dtf_help::tip_over(ui, cmd, "previewColor", top);
        // The background colour taken from the image with the eyedropper: a toggle button,
        // exclusive with Add Holes (below).
        ui.add_enabled_ui(have_doc, |ui| {
            if help(mode_button(ui, "pipette", tl!("Pick the Background Colour"), picker), cmd, "backgroundPicker").clicked() {
                picker = !picker;
                eraser &= !picker;
            }
        });
        // Interior holes by hand: a toggle button, pressed while on; each click on the visual
        // adds a hole to the preview.
        ui.add_enabled_ui(have_doc, |ui| {
            if help(mode_button(ui, "eraser-magic", tl!("Add Holes"), eraser), cmd, "holeEraser").clicked() {
                eraser = !eraser;
                picker &= !eraser;
            }
        });
        let n = holes(&f);
        if n > 0 {
            ui.horizontal(|ui| {
                ui.label(RichText::new(crate::i18n::fmt(tl!("Holes: {n}"), &[("n", &n.to_string())])).color(Tokens::get(ui.ctx()).text_dim));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if help(crate::widgets::secondary_button(ui, tl!("Remove Last Hole"), 150.0), cmd, "removeLastHole").clicked() {
                        remove_last_hole(&mut f);
                    }
                });
            });
        }
    }
    if retouch {
        let mut show_before = f.get("__before").and_then(Value::as_bool).unwrap_or(false);
        help(crate::widgets::checkbox(ui, &mut show_before, tl!("Show Before")), cmd, "showBefore");
        f.insert("__before".into(), json!(show_before));
    }
    let changed = f.iter().any(|(k, v)| !k.starts_with("__") && before.get(k) != Some(v)) || before.iter().any(|(k, _)| !k.starts_with("__") && !f.contains_key(k));
    // The cut-out's last state for Toggle Last State: the settings before this change (before a
    // whole slider drag), or the drag's start once the pointer is released.
    if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE {
        let dragging = ui.input(|i| i.pointer.any_down());
        if changed {
            record_cutout(app, cutout_state(&before), dragging);
        } else if !dragging && let Some(start) = app.ui.dtf_panel.cutout_pending.take() {
            app.ui.dtf_panel.cutout_last = Some(start);
        }
    }
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if cmd == "dtf.controlBackground" {
            // Show (or recolour) the control background, and remove it, from the same tab.
            ui.add_enabled_ui(enabled, |ui| {
                apply_it = help(crate::widgets::primary_button(ui, tl!("Show Background"), 130.0), cmd, "showBackground").clicked();
            });
        } else {
            ui.add_enabled_ui(enabled, |ui| {
                apply_it = help(crate::widgets::primary_button(ui, tl!("Apply"), 110.0), cmd, "apply").clicked();
            });
        }
        // The window's Cancel: stops a preview and closes it.
        cancel = help(crate::widgets::secondary_button(ui, tl!("Cancel"), 110.0), cmd, "cancel").clicked();
    });
    app.ui.dtf_panel.fields.insert(cmd.to_string(), f);
    if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE {
        if eraser != app.ui.dtf_panel.hole_eraser.is_some() {
            set_hole_eraser(app, eraser);
        }
        if picker != app.ui.dtf_panel.background_picker.is_some() {
            set_background_picker(app, picker);
        }
    }
    if changed && enabled && !WITHOUT_PREVIEW.contains(&cmd) {
        app.ui.dtf_panel.previewing = Some(cmd.to_string());
    }
    if apply_it {
        if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE {
            clear_cutout_history(app);
        }
        apply(app, cmd);
    } else if cancel {
        stop_preview(app);
        // Cancel drops the holes clicked for this cut-out.
        if let Some(f) = app.ui.dtf_panel.fields.get_mut(cmd) {
            f.remove(HOLES);
        }
        if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE {
            clear_cutout_history(app);
        }
    }
    apply_it || cancel || upscaled
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    #[test]
    fn every_tool_is_a_command_with_a_title() {
        for cmd in TOOLS {
            assert!(photocraft_engine::commands::find(cmd).is_some(), "{cmd}");
        }
    }

    #[test]
    fn dtf_craft_opens_on_layers_alone_and_the_tools_open_in_windows_from_the_menu() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        dtf_craft_startup(&mut app);
        assert_eq!(app.ui.workspace, "DTF");
        let layout = app.ui.docking.clone().expect("the DTF workspace's layout");
        assert_eq!(layout, dtf_layout());
        assert!(crate::panel_docking::valid(&layout));
        assert!(matches!(&layout.root, Some(craft_ui::docking::Node::Tabs { panels, .. }) if panels == &["layers".to_string()]), "{layout:?}");
        let p = &app.ui.panels;
        assert!(!p.dtf && p.layers && !p.color && !p.properties && !p.history && !p.navigator);
        // The Window menu no longer lists the DTF tools.
        assert!(!crate::menu_catalog::CATALOG.iter().any(|(path, _, _, id)| path.first() == Some(&"Window") && id.starts_with("window.panel.dtf")));
        // Essentials keeps Channels and Paths out too.
        crate::menus::invoke(&mut app, &ctx, "window.workspace.essentials", json!({})).unwrap();
        let shown: Vec<&str> = app.ui.dock.visible_tabs(crate::dock::Group::Layers, false).into_iter().map(|(_, n)| n).collect();
        assert_eq!(shown, ["Layers"]);
        crate::menus::invoke(&mut app, &ctx, "window.workspace.dtf", json!({})).unwrap();
        assert_eq!(app.ui.docking, Some(dtf_layout()));
        // DTF › Saturation… opens its window, which previews at once and is modal.
        app.run("file.new", json!({"width": 20, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        crate::menus::invoke(&mut app, &ctx, "dtf.saturation", json!({})).unwrap();
        assert_eq!(app.ui.dtf_panel.window.as_deref(), Some("dtf.saturation"));
        assert_eq!(preview_source(&app).map(|p| p.0), Some(PREVIEW_DIALOG));
        assert!(!crate::menus::modal_allows(&app, "image.imageSize") && crate::menus::modal_allows(&app, "view.zoomIn"));
        // Another DTF tool takes its place; the cut-out's window leaves the menus free.
        close_window(&mut app);
        assert!(preview_source(&app).is_none() && app.ui.dtf_panel.previewing.is_none(), "closing stops the preview");
        crate::menus::invoke(&mut app, &ctx, photocraft_engine::dtf_cmds::REMOVE_WHITE, json!({})).unwrap();
        assert!(crate::menus::modal_allows(&app, "image.imageSize"));
        // With parameters the command still runs (automation), without a window.
        close_window(&mut app);
        crate::menus::invoke(&mut app, &ctx, "dtf.saturation", json!({"saturation": 10})).unwrap();
        assert!(app.ui.dtf_panel.window.is_none());
    }

    /// Every DTF tool is in the DTF menu under its window's title, and a click opens that window
    /// (never the old settings dialog), as the menu bar runs it.
    #[test]
    fn the_dtf_menu_opens_each_tool_window_under_its_title() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 20, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        let ctx = egui::Context::default();
        let dtf: Vec<(&str, &str)> = crate::menu_catalog::CATALOG.iter().filter(|(path, _, _, id)| path == &["DTF"] && *id != "---").map(|(_, label, _, id)| (*label, *id)).collect();
        for (cmd, tab) in TOOLS.iter().zip(TABS) {
            let label = dtf.iter().find(|(_, id)| id == cmd).map(|(l, _)| *l);
            assert_eq!(label, Some(format!("{tab}…").as_str()), "{cmd}");
            crate::shortcut_dispatch::dispatch(&mut app, &ctx, cmd);
            assert_eq!(app.ui.dtf_panel.window.as_deref(), Some(*cmd), "{cmd}");
            assert!(app.ui.dialogs.is_empty(), "{cmd}: no settings dialog");
            close_window(&mut app);
        }
        // The right column never shows the DTF tools again, even from an old saved layout.
        app.ui.panels.dtf = true;
        app.ui.docking = Some(craft_ui::docking::Layout {
            root: Some(craft_ui::docking::Node::Tabs { panels: vec!["layers".into(), "dtfConformity".into()], active: 1 }),
            floating: Vec::new(),
        });
        keep_dtf_out_of_the_dock(&mut app);
        assert!(!app.ui.panels.dtf);
        assert_eq!(app.ui.docking.as_ref().unwrap().panels(), vec![&"layers".to_string()]);
    }

    /// The DTF specs carry descriptions in parentheses (with commas): their keys and defaults
    /// still read right, so Print Conformity opens at 27 cm high, in cm.
    #[test]
    fn dtf_settings_read_their_defaults() {
        use crate::filter_dialog::{Kind, parse_spec};
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let f = initial_fields(&app, "dtf.conformity");
        assert_eq!((f.get("width"), f.get("height")), (Some(&json!(0.0)), Some(&json!(27.0))), "{f:?}");
        assert_eq!(f.get("trim"), Some(&json!(true)));
        let ht = parse_spec(photocraft_engine::commands::find("filter.dtf.halftone").unwrap().params);
        let keys: Vec<&str> = ht.iter().map(|p| p.key.as_str()).collect();
        assert_eq!(keys, ["mode", "frequency", "angle", "shape", "minDot", "maxDot", "inBlack", "inWhite", "gamma"]);
        assert!(matches!(&ht[1].kind, Kind::Range { default, .. } if *default == 25.0));
        let up = parse_spec(photocraft_engine::commands::find("dtf.upscale").unwrap().params);
        assert!(up.iter().any(|p| p.key == "keepPrintSize" && p.kind == Kind::Bool(true)), "{up:?}");
    }

    #[test]
    fn apply_runs_the_tool_and_remembers_its_settings() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 40, "height": 20, "resolution": 300, "background": "transparent"})).unwrap();
        let mut f = initial_fields(&app, "dtf.controlBackground");
        f.insert("color".into(), json!("#102030"));
        app.ui.dtf_panel.fields.insert("dtf.controlBackground".into(), f);
        apply(&mut app, "dtf.controlBackground");
        let names: Vec<String> = app.session.active().unwrap().doc.walk().into_iter().map(|(_, _, l)| l.name.clone()).collect();
        assert!(names.iter().any(|n| n == "backfill"), "{names:?}");
        let saved = app.session.prefs().dialogs.get("dtf.controlBackground").cloned();
        assert_eq!(saved.and_then(|v| v.get("color").cloned()), Some(json!("#102030")));
        assert_eq!(initial_fields(&app, "dtf.controlBackground").get("color"), Some(&json!("#102030")));
    }

    #[test]
    fn a_changed_setting_previews_on_the_canvas_until_cancel() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.background_jobs = false;
        app.run("file.new", json!({"width": 32, "height": 8, "resolution": 300, "background": "transparent"})).unwrap();
        app.session
            .edit("Ramp", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 0..8 {
                    for x in 0..32 {
                        surf.write_pixel(x, y, &[1.0, 0.0, 0.0, x as f32 / 31.0]);
                    }
                }
                Ok(())
            })
            .unwrap();
        let ctx = egui::Context::default();
        let cmd = "filter.dtf.removeSemiTransparent";
        // The tool's tab is the one on screen.
        app.ui.dtf_panel.window = Some(cmd.into());
        let mut f = initial_fields(&app, cmd);
        f.insert("threshold".into(), json!(50));
        app.ui.dtf_panel.fields.insert(cmd.into(), f);
        app.ui.dtf_panel.previewing = Some(cmd.into());
        assert!(crate::canvas::ensure_filter_preview(&mut app, 0, &ctx).is_some());
        let preview = app.filter_preview.as_ref().unwrap();
        assert_eq!(preview.key.dialog, PREVIEW_DIALOG);
        let doc = preview.result.as_ref().unwrap();
        let layer = doc.layer(doc.layers[0].id).unwrap().surface().unwrap();
        // The ramp starts transparent at x = 0: its fringe (2 px) becomes binary, the rest stays.
        assert!((1..3).all(|x| [0.0, 1.0].contains(&layer.pixel(x, 4)[3])), "the fringe is binary");
        assert!((layer.pixel(10, 4)[3] - 10.0 / 31.0).abs() < 0.01, "inside, the pixels stay");
        // The document itself is untouched until Apply.
        let st = app.session.active().unwrap();
        assert!(st.doc.layer(st.doc.layers[0].id).unwrap().surface().unwrap().pixel(10, 4)[3] > 0.0);
        stop_preview(&mut app);
        assert!(app.filter_preview.is_none());
        assert!(crate::canvas::ensure_filter_preview(&mut app, 0, &ctx).is_none());
    }

    #[test]
    fn the_cutout_previews_on_the_control_background() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.background_jobs = false;
        app.run("file.new", json!({"width": 30, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        app.session
            .edit("Logo", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 5..15 {
                    for x in 10..20 {
                        surf.write_pixel(x, y, &[1.0, 0.0, 0.0, 1.0]);
                    }
                }
                Ok(())
            })
            .unwrap();
        let ctx = egui::Context::default();
        let cmd = "filter.dtf.removeWhiteBackground";
        app.ui.dtf_panel.fields.insert(cmd.into(), initial_fields(&app, cmd));
        // The tool's tab is the one on screen.
        app.ui.dtf_panel.window = Some(cmd.into());
        assert_eq!(app.ui.dtf_panel.fields[cmd].get("color"), Some(&json!("#ffffff")));
        app.ui.dtf_panel.previewing = Some(cmd.into());
        assert!(crate::canvas::ensure_filter_preview(&mut app, 0, &ctx).is_some());
        let doc = app.filter_preview.as_ref().unwrap().result.as_ref().unwrap();
        let names: Vec<String> = doc.walk().into_iter().map(|(_, _, l)| l.name.clone()).collect();
        assert!(names.iter().any(|n| n == "backfill") && names.iter().any(|n| n == "Sans fond"), "{names:?}");
        // On screen: the dark control colour where the white was, the visual where it is.
        let rgb = photocraft_compose::flatten_to_surface(doc, photocraft_color::PixelFormat::new(photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::F32, false), None);
        let hex = |x, y| rgb.pixel(x, y).iter().take(3).map(|v| format!("{:02x}", (v * 255.0).round() as u8)).collect::<String>();
        assert_eq!(hex(0, 0), "231c1c");
        assert_eq!(hex(15, 10), "ff0000");
        // The document is untouched until Apply.
        assert_eq!(app.session.active().unwrap().doc.layers.len(), 1);
    }

    /// The DTF tools spread over several tab groups (the DTF workspace): drawing one tool's tab
    /// keeps the preview another tool's setting started.
    #[test]
    fn a_preview_survives_other_dtf_tabs_on_screen() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 30, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        let cutout = TOOLS.iter().position(|c| *c == "filter.dtf.removeWhiteBackground").unwrap();
        let saturation = TOOLS.iter().position(|c| *c == "dtf.saturation").unwrap();
        app.ui.dtf_panel.fields.insert(TOOLS[cutout].into(), initial_fields(&app, TOOLS[cutout]));
        app.ui.dtf_panel.previewing = Some(TOOLS[cutout].into());
        let mut h = egui_kittest::Harness::builder().with_size(vec2(300.0, 900.0)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                // The app's fonts are set up before the first frame draws a tab.
                let ready = egui::Id::new("dtf-test-fonts");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ALL[0]);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    return;
                }
                tool_body(app, ui, cutout);
                tool_body(app, ui, saturation);
            },
            app,
        );
        h.run_steps(4);
        let app = h.state();
        assert_eq!(app.ui.dtf_panel.previewing.as_deref(), Some(TOOLS[cutout]));
    }

    /// A harness drawing DTF tabs with the app's fonts (set up on the first pass).
    fn tabs_harness(app: PhotocraftApp, draw: impl FnMut(&mut egui::Ui, &mut PhotocraftApp) + 'static) -> egui_kittest::Harness<'static, PhotocraftApp> {
        let mut draw = draw;
        egui_kittest::Harness::builder().with_size(vec2(320.0, 900.0)).build_ui_state(
            move |ui, app: &mut PhotocraftApp| {
                let ready = egui::Id::new("dtf-test-fonts");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ALL[0]);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    return;
                }
                draw(ui, app);
            },
            app,
        )
    }

    fn tab_of(cmd: &str) -> usize {
        TOOLS.iter().position(|c| *c == cmd).unwrap()
    }

    /// Print Conformity: the layer name reads "origin" (a remembered 0 is dropped), the sizes
    /// start from the trimmed visual at the document's ppi, and the other side follows.
    #[test]
    fn conformity_starts_from_the_visual_and_keeps_its_proportions() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.session.prefs.edit(|p| p.dialogs.insert("dtf.conformity".into(), json!({"layerName": 0, "width": 0, "height": 27, "resolution": 300})));
        app.run("file.new", json!({"width": 600, "height": 600, "resolution": 150, "background": "transparent"})).unwrap();
        app.run("select.rect", json!({"x": 100, "y": 200, "width": 300, "height": 150})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
        let cmd = "dtf.conformity";
        let mut f = initial_fields(&app, cmd);
        assert_eq!(f.get("layerName"), Some(&json!("origin")));
        sync_conformity(&mut app, &mut f);
        // 300 × 150 px at 150 ppi: 5.08 × 2.54 cm.
        assert_eq!((f.get("width"), f.get("height"), f.get("resolution")), (Some(&json!(5.1)), Some(&json!(2.5)), Some(&json!(150.0))));
        // Applying as read only crops (no resampling): 300 × 150 plus the 2 px margin.
        let mut params = crate::filter_dialog::params_of(&f);
        conformity_params(&f, &mut params);
        assert_eq!((params["width"].clone(), params["height"].clone()), (json!(0), json!(0)));
        // A new resolution keeps the pixels: the cm follow, nothing is resampled.
        let before = f.clone();
        f.insert("resolution".into(), json!(300.0));
        link_dimensions(&app, &before, &mut f);
        assert_eq!((f.get("width"), f.get("height")), (Some(&json!(2.5)), Some(&json!(1.3))));
        let mut params = crate::filter_dialog::params_of(&f);
        conformity_params(&f, &mut params);
        assert_eq!((params["width"].clone(), params["height"].clone(), params["resolution"].clone()), (json!(0), json!(0), json!(300.0)));
        let before = f.clone();
        f.insert("resolution".into(), json!(150.0));
        link_dimensions(&app, &before, &mut f);
        // A new width sets the height.
        let before = f.clone();
        f.insert("width".into(), json!(10.0));
        link_dimensions(&app, &before, &mut f);
        assert_eq!(f.get("height"), Some(&json!(5.0)));
        let mut params = crate::filter_dialog::params_of(&f);
        conformity_params(&f, &mut params);
        assert_eq!((params["width"].clone(), params["height"].clone()), (json!(10.0), json!(0)));
        app.ui.dtf_panel.fields.insert(cmd.into(), f);
        apply(&mut app, cmd);
        let d = &app.session.active().unwrap().doc;
        // 10 cm at 150 ppi = 591 px (+ 4), proportions kept.
        assert_eq!((d.size.width, d.size.height), (595, 299));
        assert_eq!(d.layers[0].name, "origin");
        // The new document state is read again: its size now.
        let mut f = app.ui.dtf_panel.fields.remove(cmd).unwrap();
        sync_conformity(&mut app, &mut f);
        assert_eq!(f.get("width"), Some(&json!(10.0)));
    }

    /// Add Holes: each click adds a hole to the cut-out's preview, on the preview background;
    /// Apply cuts them with the background, contour unmixed (no light line); "Remove Last Hole"
    /// takes one back; off, the previous tool comes back.
    #[test]
    fn add_holes_preview_with_the_cutout_and_apply_together() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.background_jobs = false;
        app.run("file.new", json!({"width": 40, "height": 40, "resolution": 300, "background": "white"})).unwrap();
        // A red square (8..32) with a white hole (16..24); a pink antialiased hole edge at x = 15.
        app.session
            .edit("Logo", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 8..32 {
                    for x in 8..32 {
                        let hole = (16..24).contains(&x) && (16..24).contains(&y);
                        let c = if hole { [1.0; 3] } else if x == 15 && (16..24).contains(&y) { [1.0, 0.5, 0.5] } else { [1.0, 0.0, 0.0] };
                        surf.write_pixel(x, y, &[c[0], c[1], c[2], 1.0]);
                    }
                }
                Ok(())
            })
            .unwrap();
        let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
        app.ui.dtf_panel.window = Some(cmd.into());
        app.ui.tool = crate::state::Tool::Brush;
        assert!(!hole_click(&mut app, 20.0, 20.0), "off: the click is the tool's");
        set_hole_eraser(&mut app, true);
        assert_eq!(app.ui.tool, crate::state::Tool::MagicEraser);
        assert!(hole_click(&mut app, 20.0, 20.0));
        assert!(hole_click(&mut app, 2.0, 2.0));
        assert_eq!(holes(&app.ui.dtf_panel.fields[cmd]), 2);
        remove_last_hole(app.ui.dtf_panel.fields.get_mut(cmd).unwrap());
        assert_eq!(holes(&app.ui.dtf_panel.fields[cmd]), 1);
        // The preview shows the hole on the preview background, the document is unchanged.
        assert_eq!(app.ui.dtf_panel.previewing.as_deref(), Some(cmd));
        let ctx = egui::Context::default();
        assert!(crate::canvas::ensure_filter_preview(&mut app, 0, &ctx).is_some());
        let doc = app.filter_preview.as_ref().unwrap().result.as_ref().unwrap();
        let rgb = photocraft_compose::flatten_to_surface(doc, photocraft_color::PixelFormat::new(photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::F32, false), None);
        let control = crate::color_picker_ui::parse_hex(PREVIEW_COLOR).unwrap();
        for (x, y) in [(20, 20), (2, 2)] {
            assert!(rgb.pixel(x, y)[..3].iter().zip(control).all(|(a, b)| (a - b).abs() < 0.01), "({x}, {y}) {:?}", rgb.pixel(x, y));
        }
        let st = app.session.active().unwrap();
        assert_eq!(st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap().pixel(20, 20)[3], 1.0);
        // Apply: one cut, the hole and the background; the pink edge unmixed; holes not kept.
        apply(&mut app, cmd);
        let st = app.session.active().unwrap();
        let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap();
        assert_eq!((layer.pixel(20, 20)[3], layer.pixel(2, 2)[3], layer.pixel(10, 10)[3]), (0.0, 0.0, 1.0));
        let edge = layer.pixel(15, 20);
        assert!((edge[3] - 0.5).abs() < 0.02 && edge[1] < 0.02 && edge[0] > 0.98, "{edge:?}");
        assert_eq!(holes(&app.ui.dtf_panel.fields[cmd]), 0);
        assert!(app.session.prefs().dialogs.get(cmd).is_some_and(|v| v.get(HOLES).is_none()), "holes are not remembered");
        set_hole_eraser(&mut app, false);
        assert_eq!(app.ui.tool, crate::state::Tool::Brush);
    }

    /// Toggle Last State (Ctrl+Alt+Z) while the cut-out is previewed: a hole added by mistake
    /// goes, then comes back; one level; after Apply it is the document's again.
    #[test]
    fn toggle_last_state_steps_the_cutout_before_apply() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.background_jobs = false;
        app.run("file.new", json!({"width": 20, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
        assert!(crate::dtf_panel::is_enabled(&app, "edit.toggleLastState").is_none(), "nothing to toggle yet");
        set_hole_eraser(&mut app, true);
        assert!(hole_click(&mut app, 5.0, 5.0));
        assert!(hole_click(&mut app, 8.0, 8.0));
        assert_eq!(crate::dtf_panel::is_enabled(&app, "edit.toggleLastState"), Some(true));
        app.run("edit.toggleLastState", json!({})).unwrap();
        assert_eq!(holes(&app.ui.dtf_panel.fields[cmd]), 1, "the last hole goes");
        app.run("edit.toggleLastState", json!({})).unwrap();
        assert_eq!(holes(&app.ui.dtf_panel.fields[cmd]), 2, "and comes back");
        // A setting changed in the tab is a state too (here as the tab records it).
        let before = cutout_state(&app.ui.dtf_panel.fields[cmd]);
        app.ui.dtf_panel.fields.get_mut(cmd).unwrap().insert("tolerance".into(), json!(30));
        record_cutout(&mut app, before, false);
        app.run("edit.toggleLastState", json!({})).unwrap();
        assert_eq!(app.ui.dtf_panel.fields[cmd].get("tolerance"), Some(&json!(10.0)));
        // Cancel ends it: the document's own history answers again.
        stop_preview(&mut app);
        clear_cutout_history(&mut app);
        assert!(intercept(&mut app, "edit.toggleLastState").is_none());
    }

    /// Add Holes and the eyedropper are toggle buttons, never on together; with neither on, a
    /// click on the visual does nothing while the cut-out is previewed.
    #[test]
    fn the_cutout_modes_are_exclusive_toggles_and_idle_clicks_do_nothing() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 20, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
        app.ui.tool = crate::state::Tool::Brush;
        app.ui.dtf_panel.previewing = Some(cmd.into());
        assert!(blocks_canvas(&app), "no mode on: the click does nothing");
        let revision = app.session.active().unwrap().revision;
        crate::canvas::tool_event(&mut app, crate::canvas::ToolEvent::Down { x: 5.0, y: 5.0, pressure: 1.0 }, egui::Modifiers::NONE);
        crate::canvas::tool_event(&mut app, crate::canvas::ToolEvent::Up { x: 5.0, y: 5.0 }, egui::Modifiers::NONE);
        assert_eq!(app.session.active().unwrap().revision, revision, "the brush did not paint");
        set_hole_eraser(&mut app, true);
        assert!(!blocks_canvas(&app));
        set_background_picker(&mut app, true);
        assert!(app.ui.dtf_panel.hole_eraser.is_none() && app.ui.dtf_panel.background_picker.is_some());
        set_background_picker(&mut app, false);
        assert_eq!(app.ui.tool, crate::state::Tool::Brush);
        app.ui.tool = crate::state::Tool::Hand;
        assert!(!blocks_canvas(&app), "the Hand still moves the view");
    }

    /// The background eyedropper takes the clicked colour, turns detection off and goes off;
    /// it and Add Holes are never on together.
    #[test]
    fn the_background_eyedropper_takes_the_colour_to_remove() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 20, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        app.run("select.rect", json!({"x": 0, "y": 0, "width": 20, "height": 20})).unwrap();
        app.run("edit.fill", json!({"color": "#336699"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
        let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
        app.ui.tool = crate::state::Tool::Brush;
        assert!(!pick_background(&mut app, 3.0, 3.0));
        set_hole_eraser(&mut app, true);
        set_background_picker(&mut app, true);
        assert!(app.ui.dtf_panel.hole_eraser.is_none(), "Add Holes goes off");
        assert_eq!(app.ui.tool, crate::state::Tool::Eyedropper);
        assert!(pick_background(&mut app, 3.0, 3.0));
        let f = &app.ui.dtf_panel.fields[cmd];
        assert_eq!((f.get("color"), f.get("detectColor")), (Some(&json!("#336699")), Some(&json!(false))));
        assert_eq!(app.ui.dtf_panel.previewing.as_deref(), Some(cmd));
        assert!(app.ui.dtf_panel.background_picker.is_none());
        assert_eq!(app.ui.tool, crate::state::Tool::Brush, "the previous tool comes back");
    }

    /// An already transparent file: the tab says no plain background is detected.
    #[test]
    fn the_cutout_tab_warns_when_no_plain_background_is_detected() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 30, "height": 30, "resolution": 300, "background": "transparent"})).unwrap();
        // A brown disc on transparency.
        app.session
            .edit("Disc", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 0..30 {
                    for x in 0..30 {
                        if (x as f32 - 14.5).hypot(y as f32 - 14.5) < 10.0 {
                            surf.write_pixel(x, y, &[0.4, 0.2, 0.0, 1.0]);
                        }
                    }
                }
                Ok(())
            })
            .unwrap();
        assert!(!background_detected(&mut app, 10.0));
        let mut white = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        white.run("file.new", json!({"width": 30, "height": 30, "resolution": 300, "background": "white"})).unwrap();
        assert!(background_detected(&mut white, 10.0));
    }

    /// The cut-out previews on the colour picked in its tab.
    #[test]
    fn the_cutout_previews_on_the_colour_picked() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.background_jobs = false;
        app.run("file.new", json!({"width": 30, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
        app.ui.dtf_panel.window = Some(cmd.into());
        let mut f = initial_fields(&app, cmd);
        f.insert("previewColor".into(), json!("#0000ff"));
        app.ui.dtf_panel.fields.insert(cmd.into(), f);
        app.ui.dtf_panel.previewing = Some(cmd.into());
        let ctx = egui::Context::default();
        assert!(crate::canvas::ensure_filter_preview(&mut app, 0, &ctx).is_some());
        let doc = app.filter_preview.as_ref().unwrap().result.as_ref().unwrap();
        let rgb = photocraft_compose::flatten_to_surface(doc, photocraft_color::PixelFormat::new(photocraft_color::ColorMode::Rgb, photocraft_color::SampleType::F32, false), None);
        assert_eq!(rgb.pixel(0, 0)[..3], [0.0, 0.0, 1.0]);
        // Apply doesn't hand the preview colour to the command, and remembers it.
        apply(&mut app, cmd);
        assert_eq!(app.session.prefs().dialogs.get(cmd).and_then(|v| v.get("previewColor").cloned()), Some(json!("#0000ff")));
        assert_eq!(initial_fields(&app, cmd).get("previewColor"), Some(&json!("#0000ff")));
    }

    /// A retouching tab previews as soon as it is opened; "Show Before" shows the image as it is.
    #[test]
    fn a_retouching_tab_previews_when_opened_and_shows_before_on_demand() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 30, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        let sat = tab_of("dtf.saturation");
        let shown = std::rc::Rc::new(std::cell::Cell::new(false));
        let on = shown.clone();
        let mut h = tabs_harness(app, move |ui, app| {
            tool_body(app, ui, tab_of("dtf.controlBackground"));
            if on.get() {
                tool_body(app, ui, sat);
            }
        });
        h.run_steps(4);
        assert_eq!(h.state().ui.dtf_panel.previewing, None, "nothing previews on its own");
        shown.set(true);
        h.state_mut().ui.dtf_panel.window = Some("dtf.saturation".into());
        h.run_steps(2);
        assert_eq!(h.state().ui.dtf_panel.previewing.as_deref(), Some("dtf.saturation"), "opening the tab previews");
        assert!(preview_source(h.state()).is_some());
        h.state_mut().ui.dtf_panel.fields.get_mut("dtf.saturation").unwrap().insert("__before".into(), json!(true));
        assert!(preview_source(h.state()).is_none(), "Show Before: the image as it is");
    }

    /// The help: "?" opens the tool's sheet; a setting's tooltip shows over its row and the slider
    /// under it still drags.
    #[test]
    fn the_help_sheet_and_tooltips_leave_the_settings_working() {
        use egui_kittest::kittest::Queryable;
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 30, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        let sat = tab_of("dtf.saturation");
        app.ui.dtf_panel.window = Some("dtf.saturation".into());
        let mut h = tabs_harness(app, move |ui, app| {
            tool_body(app, ui, sat);
        });
        h.run_steps(4);
        // The slider runs under the "Saturation" label, across the tab.
        let label = h.get_by_label("Saturation").rect();
        let y = label.bottom() + 14.0;
        let (from, to) = (egui::pos2(170.0, y), egui::pos2(4.0, y));
        h.event(egui::Event::PointerMoved(from));
        for _ in 0..90 {
            h.run_steps(1);
        }
        assert!(h.query_by_label_contains("De −100").is_some(), "the setting's tooltip shows");
        let before = h.state().ui.dtf_panel.fields["dtf.saturation"]["saturation"].as_f64().unwrap();
        h.event(egui::Event::PointerButton { pos: from, button: egui::PointerButton::Primary, pressed: true, modifiers: egui::Modifiers::NONE });
        h.run_steps(1);
        for k in 1..=8 {
            h.event(egui::Event::PointerMoved(from + (to - from) * (k as f32 / 8.0)));
            h.run_steps(1);
        }
        h.event(egui::Event::PointerButton { pos: to, button: egui::PointerButton::Primary, pressed: false, modifiers: egui::Modifiers::NONE });
        h.run_steps(2);
        let after = h.state().ui.dtf_panel.fields["dtf.saturation"]["saturation"].as_f64().unwrap();
        assert!(after < before - 50.0, "the slider drags under its tooltip: {before} → {after}");
        assert!(h.query_by_label_contains("Renforcer ou atténuer").is_none(), "the sheet starts closed");
        h.get_by_label("?").click();
        h.run_steps(2);
        assert!(h.query_by_label_contains("Renforcer ou atténuer").is_some(), "the sheet opens");
    }

    /// The cut-out previews on the last colour picked, here or in Control Background.
    #[test]
    fn the_cutout_previews_on_the_last_colour_picked() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 30, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        set_last_color(&mut app, "#112233");
        let cut = tab_of(photocraft_engine::dtf_cmds::REMOVE_WHITE);
        app.ui.dtf_panel.window = Some(photocraft_engine::dtf_cmds::REMOVE_WHITE.into());
        let mut h = tabs_harness(app, move |ui, app| {
            tool_body(app, ui, cut);
        });
        h.run_steps(3);
        assert_eq!(h.state().ui.dtf_panel.fields[photocraft_engine::dtf_cmds::REMOVE_WHITE]["previewColor"], json!("#112233"));
        // A colour applied as Control Background becomes the cut-out's.
        let app = h.state_mut();
        let mut f = initial_fields(app, "dtf.controlBackground");
        f.insert("color".into(), json!("#445566"));
        app.ui.dtf_panel.fields.insert("dtf.controlBackground".into(), f);
        apply(app, "dtf.controlBackground");
        h.run_steps(2);
        assert_eq!(h.state().ui.dtf_panel.fields[photocraft_engine::dtf_cmds::REMOVE_WHITE]["previewColor"], json!("#445566"));
    }

    /// Screenshots for review (`DTF_VISUALS=<folder>`): the Home screen, the panel with a
    /// preview, and the layers DTF Halftone builds, in French.
    #[test]
    fn capture_dtf_visuals() {
        let Some(dir) = std::env::var_os("DTF_VISUALS").map(std::path::PathBuf::from) else { return };
        std::fs::create_dir_all(&dir).unwrap();
        let _fr = crate::i18n::language_scope(crate::i18n::Lang::from_pref("fr"));
        let shot = |session: photocraft_engine::Session, name: &str, setup: &dyn Fn(&mut PhotocraftApp)| {
            // The cut-out's tab is long: its shots are taller.
            let height = if name.starts_with("detourage") || name.starts_with("analyseur_plage") { 1300.0 } else { 900.0 };
            let mut h = egui_kittest::Harness::builder().with_size(vec2(1440.0, height)).with_max_steps(64).wgpu().build_eframe(move |cc| {
                PhotocraftApp::setup_context(&cc.egui_ctx, Default::default());
                let mut app = PhotocraftApp::new(session, crate::Services::default());
                // The canvas draws previews on the GPU, as in the app.
                if let Some(rs) = cc.wgpu_render_state.as_ref() {
                    app.set_wgpu(rs.clone());
                }
                dtf_craft_startup(&mut app);
                app
            });
            h.state_mut().session.prefs.edit(|p| p.interface.language = "fr".into());
            h.run_steps(6);
            setup(h.state_mut());
            // Previews compute off the UI thread: give them time to land.
            for _ in 0..40 {
                h.run_steps(1);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            h.render().unwrap().save(dir.join(format!("{name}.png"))).unwrap();
        };
        shot(photocraft_engine::Session::new(), "accueil", &|_| {});
        // With an illustration behind it (a drawn stand-in for a Pixabay one).
        shot(photocraft_engine::Session::new(), "accueil_illustration", &|app| {
            let (w, h) = (1280usize, 720usize);
            let blobs = [(0.2, 0.3, 0.35, [124.0, 58.0, 237.0]), (0.75, 0.25, 0.3, [6.0, 182.0, 212.0]), (0.55, 0.8, 0.4, [245.0, 158.0, 11.0]), (0.9, 0.85, 0.25, [219.0, 39.0, 119.0])];
            let mut rgba = Vec::with_capacity(w * h * 4);
            for y in 0..h {
                for x in 0..w {
                    let (fx, fy) = (x as f32 / w as f32, y as f32 / h as f32 * 0.5625);
                    let mut c = [30.0f32, 41.0, 59.0];
                    for (bx, by, r, col) in blobs {
                        let d = ((fx - bx).powi(2) + (fy - by * 0.5625).powi(2)).sqrt();
                        let k = (1.0 - d / r).clamp(0.0, 1.0).powf(1.5);
                        for i in 0..3 {
                            c[i] += (col[i] - c[i]) * k;
                        }
                    }
                    rgba.extend([c[0] as u8, c[1] as u8, c[2] as u8, 255]);
                }
            }
            let hit = crate::home_image::Hit { url: String::new(), user: "Exemple".into(), page: "https://pixabay.com/".into() };
            crate::home_image::show_image(app, [w, h], rgba, hit);
        });
        let visual = || {
            let mut s = photocraft_engine::Session::new();
            s.execute("file.new", json!({"width": 600, "height": 400, "resolution": 300, "background": "transparent"})).unwrap();
            s.edit("Visual", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 0..400 {
                    for x in 0..600 {
                        let d = (((x as f32 - 300.0).powi(2) + (y as f32 - 200.0).powi(2)).sqrt() / 180.0).min(1.0);
                        surf.write_pixel(x, y, &[0.9, 0.35 + 0.3 * d, 0.1, (1.2 - d).clamp(0.0, 1.0)]);
                    }
                }
                Ok(())
            })
            .unwrap();
            s.execute("layer.setProps", json!({"name": "origin"})).unwrap();
            s
        };
        shot(visual(), "panneau", &|_| {});
        shot(visual(), "apercu", &|app| {
            open_window(app, "filter.dtf.removeSemiTransparent");
            let mut f = initial_fields(app, "filter.dtf.removeSemiTransparent");
            f.insert("threshold".into(), json!(80));
            app.ui.dtf_panel.fields.insert("filter.dtf.removeSemiTransparent".into(), f);
            app.ui.dtf_panel.previewing = Some("filter.dtf.removeSemiTransparent".into());
        });
        let on_white = || {
            let mut s = photocraft_engine::Session::new();
            s.execute("file.new", json!({"width": 600, "height": 400, "resolution": 300, "background": "white"})).unwrap();
            s.edit("Logo", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 0..400 {
                    for x in 0..600 {
                        let d = ((x as f32 - 300.0).powi(2) + (y as f32 - 200.0).powi(2)).sqrt();
                        // A red disc with a soft (antialiased) edge and a white centre.
                        let cover = (150.0 - d).clamp(0.0, 1.5) / 1.5;
                        let centre = d < 50.0;
                        let c = if centre { [1.0, 1.0, 1.0] } else { [1.0 - 0.15 * cover, 1.0 - 0.85 * cover, 1.0 - 0.85 * cover] };
                        surf.write_pixel(x, y, &[c[0], c[1], c[2], 1.0]);
                    }
                }
                Ok(())
            })
            .unwrap();
            s
        };
        shot(on_white(), "detourage", &|app| {
            open_window(app, "filter.dtf.removeWhiteBackground");
            let cmd = "filter.dtf.removeWhiteBackground";
            let mut f = initial_fields(app, cmd);
            f.insert("tolerance".into(), json!(12));
            app.ui.dtf_panel.fields.insert(cmd.into(), f);
            app.ui.dtf_panel.previewing = Some(cmd.into());
        });
        // Add Holes pressed, two holes clicked: the toggle buttons, the counter.
        shot(on_white(), "detourage_trous", &|app| {
            open_window(app, "filter.dtf.removeWhiteBackground");
            let cmd = "filter.dtf.removeWhiteBackground";
            app.ui.dtf_panel.fields.insert(cmd.into(), initial_fields(app, cmd));
            set_hole_eraser(app, true);
            hole_click(app, 300.0, 200.0);
            hole_click(app, 20.0, 20.0);
        });
        shot(visual(), "conformite", &|app| {
            // As the menu bar runs it.
            crate::shortcut_dispatch::dispatch(app, &egui::Context::default(), "dtf.conformity");
        });
        // Saturation opened: its preview shows without moving the slider.
        shot(visual(), "saturation", &|app| {
            let mut f = initial_fields(app, "dtf.saturation");
            f.insert("saturation".into(), json!(-100));
            app.ui.dtf_panel.fields.insert("dtf.saturation".into(), f);
            app.ui.dtf_panel.first_pass = Some(0);
            open_window(app, "dtf.saturation");
        });
        // The help sheet open in the cut-out's tab.
        shot(on_white(), "aide", &|app| {
            open_window(app, "filter.dtf.removeWhiteBackground");
            let cmd = "filter.dtf.removeWhiteBackground";
            let mut f = initial_fields(app, cmd);
            f.insert("__help".into(), json!(true));
            app.ui.dtf_panel.fields.insert(cmd.into(), f);
        });
        let mut halftoned = visual();
        halftoned.execute("filter.dtf.halftone", json!({"mode": "semiTransparent", "frequency": 25})).unwrap();
        shot(halftoned, "halftone", &|_| {});
        // The Color Analyzer on a flat-colour logo, with a fictitious swatch book.
        let logo = || {
            let mut s = photocraft_engine::Session::new();
            s.execute("file.new", json!({"width": 400, "height": 400, "resolution": 300, "background": "transparent", "name": "logo-client.png"})).unwrap();
            s.edit("Logo", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                let rgb = |h: u32| [((h >> 16) & 255) as f32 / 255.0, ((h >> 8) & 255) as f32 / 255.0, (h & 255) as f32 / 255.0];
                for y in 0..400 {
                    for x in 0..400 {
                        let (fx, fy) = (x as f32 + 0.5, y as f32 + 0.5);
                        let d = ((fx - 200.0).powi(2) + (fy - 200.0).powi(2)).sqrt();
                        let cover = |edge: f32| (edge + 0.5).clamp(0.0, 1.0);
                        let alpha = cover(190.0 - d);
                        if alpha <= 0.0 {
                            continue;
                        }
                        let sun = ((fx - 200.0).powi(2) + (fy - 150.0).powi(2)).sqrt();
                        let wave = 235.0 + 12.0 * (fx / 40.0).sin();
                        let mut c = rgb(0x1F3A93);
                        let mix = |a: [f32; 3], b: [f32; 3], k: f32| [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * k);
                        c = mix(c, rgb(0xF2C14E), cover(55.0 - sun));
                        c = mix(c, rgb(0xE4572E), cover(fy - wave) * cover(150.0 - (fx - 200.0).abs()));
                        c = mix(c, rgb(0x2E8B57), cover(fy - wave - 34.0));
                        c = mix(c, rgb(0xFFFFFF), cover(3.0 - (d - 172.0).abs()));
                        surf.write_pixel(x, y, &[c[0], c[1], c[2], alpha]);
                    }
                }
                Ok(())
            })
            .unwrap();
            s
        };
        let analyzer = |app: &mut PhotocraftApp, view: crate::color_analyzer_ui::View| {
            let csv = "Ref;R;V;B\nEX-201 Bleu roi;30;59;150\nEX-000 Blanc;255;255;255\nEX-114 Orange vif;226;94;43\nEX-087 Jaune soleil;243;191;82\nEX-152 Vert prairie;47;148;80\nEX-900 Noir;0;0;0\n";
            let swatches = photocraft_algo::dtf_color::parse_swatch_book("Nuancier-exemple.csv", csv.as_bytes()).unwrap();
            crate::color_analyzer_ui::open(app).unwrap();
            let a = app.color_analyzer.as_mut().unwrap();
            a.set_book(Some(crate::color_analyzer_ui::Book { name: "Nuancier-exemple.csv".into(), swatches }));
            a.settings.client = "Atelier (exemple)".into();
            a.view = view;
        };
        shot(logo(), "analyseur", &|app| analyzer(app, crate::color_analyzer_ui::View::Replaced));
        shot(logo(), "analyseur_remplace", &|app| analyzer(app, crate::color_analyzer_ui::View::Replaced));
        // Color Range: the sun picked (the rest greyed), then turned orange.
        let sun = |app: &mut PhotocraftApp, view: crate::color_analyzer_ui::View, apply: bool| {
            analyzer(app, view);
            let a = app.color_analyzer.as_mut().unwrap();
            a.range_tool = true;
            a.range_click(0.5, 0.375, false, false);
            if apply {
                a.range_swatch = Some(2);
                a.apply_range();
            }
        };
        shot(logo(), "analyseur_plage", &|app| sun(app, crate::color_analyzer_ui::View::Original, false));
        shot(logo(), "analyseur_plage_appliquee", &|app| sun(app, crate::color_analyzer_ui::View::Replaced, true));
        // The colour sheet, as a PDF beside the shots, with the range.
        let mut app = PhotocraftApp::new(logo(), crate::Services::default());
        sun(&mut app, crate::color_analyzer_ui::View::Original, true);
        std::fs::write(dir.join("fiche.pdf"), crate::color_sheet::pdf(&app.color_analyzer.as_ref().unwrap().sheet())).unwrap();
    }
}
