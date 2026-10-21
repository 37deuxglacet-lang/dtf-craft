// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF Craft's DTF tools as dock panels: the DTF group (`dock::Group::Dtf`) has one tab per DTF
//! preparation tool, which drags, groups, floats and docks like every other panel (Window ›
//! Print Conformity…, one entry per tool). Each tab shows the tool's settings (the same schema as
//! its DTF menu dialog) and an Apply button; moving a setting previews the result on the canvas,
//! through the filter-dialog preview (`canvas::ensure_filter_preview`), until Apply or Cancel.
//!
//! The menu entries and their dialogs stay in the DTF menu: the tabs run the same commands.

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
    /// The beta notice is on screen (shown at every start of DTF Craft until OK).
    #[serde(skip)]
    pub beta_notice: bool,
}

/// The section title: the tool's DTF menu label without its "…".
fn title(command: &str) -> String {
    let label = photocraft_engine::commands::find(command).map_or(command, |c| c.label);
    tl!(label).trim_end_matches('…').to_string()
}

/// The layout version DTF Craft last set up (remembered with the preferences): raising it
/// brings the DTF default layout back once, over a layout saved by an older version.
const LAYOUT_VERSION: u64 = 4;
const LAYOUT_PREF: &str = "dtfCraft.layoutVersion";

/// The DTF workspace (Window › Workspace › DTF): the DTF tools and Layers in the right column,
/// three tabs at most per group; every other panel (Channels and Paths included) in the Window menu.
pub(crate) fn dtf_layout() -> craft_ui::docking::Layout<String> {
    use craft_ui::docking::{Layout, Node};
    use craft_ui::layout::{SplitAxis, SplitSize};
    let tabs = |panels: &[&str]| Node::Tabs { panels: panels.iter().map(|p| p.to_string()).collect(), active: 0 };
    let split = |height: f32, first: Node<String>, second: Node<String>| Node::Split {
        axis: SplitAxis::Vertical,
        size: SplitSize::FixedFirst(height),
        first: Box::new(first),
        second: Box::new(second),
    };
    Layout {
        root: Some(split(
            300.0,
            tabs(&["dtfBackground", "dtfSharpness", "dtfSaturation"]),
            split(210.0, tabs(&["dtfTransparency", "dtfFringe"]), tabs(&["layers", "dtfConformity", "dtfCutout"])),
        )),
        floating: Vec::new(),
    }
}

/// DTF Craft's start: the DTF tools, and on the first launch of this layout the DTF workspace.
pub fn dtf_craft_startup(app: &mut PhotocraftApp) {
    app.dtf_craft = true;
    app.ui.dtf_panel.beta_notice = true;
    let seen = app.session.prefs().dialogs.get(LAYOUT_PREF).and_then(Value::as_u64).unwrap_or(0);
    if seen < LAYOUT_VERSION {
        app.ui.workspace = "DTF".into();
        crate::menus::apply_workspace(app);
        app.session.prefs.edit(|prefs| prefs.dialogs.insert(LAYOUT_PREF.into(), json!(LAYOUT_VERSION)));
    }
}

/// The beta notice: this build is a beta, the release is planned for 2 November 2026. Shown at
/// every start of DTF Craft; OK, Enter or Esc closes it.
pub fn beta_notice(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.ui.dtf_panel.beta_notice {
        return;
    }
    let mut ok = false;
    let modal = egui::Modal::new(egui::Id::new("dtf-craft-beta")).show(ctx, |ui| {
        ui.set_max_width(420.0);
        ui.label(RichText::new(tl!("DTF Craft — Beta Version")).font(crate::theme::semibold(15.0)));
        ui.add_space(4.0);
        crate::widgets::hairline(ui);
        ui.add_space(8.0);
        ui.label(tl!("This is a beta version of DTF Craft, for testing: some tools may still change or contain bugs."));
        ui.add_space(4.0);
        ui.label(RichText::new(tl!("Release planned for November 2, 2026.")).strong());
        ui.add_space(12.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ok = crate::widgets::primary_button(ui, tl!("OK"), 84.0).clicked();
        });
    });
    if ok || modal.should_close() || ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter)) {
        app.ui.dtf_panel.beta_notice = false;
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
    if !app.ui.panels.dtf {
        return None;
    }
    let cmd = app.ui.dtf_panel.previewing.as_ref()?;
    // Only while the tool's tab is on screen (several DTF tabs can be, in separate groups).
    let tab = TOOLS.iter().position(|t| t == cmd)?;
    if !crate::panel_docking::dtf_tab_shown(app, tab) {
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
                let mut saved: Map<String, Value> = saved.into_iter().filter(|(k, _)| k != "layer" && k != "channel").collect();
                if let Some(c) = preview_color {
                    saved.insert("previewColor".into(), c);
                }
                app.session.prefs.edit(|prefs| prefs.dialogs.insert(command.into(), Value::Object(saved)));
            }
        }
        Err(error) => crate::notices::error(app, error),
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
fn upscale_buttons(app: &mut PhotocraftApp, ui: &mut egui::Ui, f: &Map<String, Value>) {
    let Some(st) = app.session.active() else { return };
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
}

/// Turn the interior-hole Magic Eraser on or off (Remove White Background tab). On, the Magic
/// Eraser tool is active, contiguous: a click on the canvas erases the enclosed area it reaches,
/// with the tolerance set in the tab (the tool's own). Off, the previous tool comes back.
pub(crate) fn set_hole_eraser(app: &mut PhotocraftApp, on: bool) {
    match (on, app.ui.dtf_panel.hole_eraser) {
        (true, None) => {
            app.ui.dtf_panel.hole_eraser = Some(app.ui.tool);
            app.ui.tool = crate::state::Tool::MagicEraser;
            app.ui.tool_options.contiguous = true;
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

/// One DTF tool's tab (`tab` indexes [`TOOLS`]): its settings, Apply, and Cancel while previewed.
pub(crate) fn tool_body(app: &mut PhotocraftApp, ui: &mut egui::Ui, tab: usize) {
    let Some(&cmd) = TOOLS.get(tab) else { return };
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
    let Some(mut f) = app.ui.dtf_panel.fields.remove(cmd) else { return };
    if cmd == "dtf.conformity" {
        sync_conformity(app, &mut f);
    }
    let previewing = app.ui.dtf_panel.previewing.as_deref() == Some(cmd);
    match note {
        Some(n) => f.insert("__note".into(), json!(n)),
        None => f.remove("__note"),
    };
    let (mut apply_it, mut cancel, mut remove_bg) = (false, false, false);
    let has_backfill =
        cmd == "dtf.controlBackground" && app.session.active().is_some_and(|st| st.doc.walk().into_iter().any(|(_, _, l)| l.name == "backfill"));
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
    if cmd == "dtf.conformity" {
        link_dimensions(app, &before, &mut f);
        if have_doc {
            upscale_buttons(app, ui, &f);
        }
    }
    let mut eraser = app.ui.dtf_panel.hole_eraser.is_some();
    if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE {
        // The control colour the cut-out previews on (the garment's).
        let top = ui.cursor().top();
        if color_row(ui, &mut f, "previewColor", tl!("Preview Background"))
            && let Some(hex) = f.get("previewColor").and_then(Value::as_str).map(str::to_string)
        {
            set_last_color(app, &hex);
        }
        crate::dtf_help::tip_over(ui, cmd, "previewColor", top);
        // Interior holes by hand: each click with the Magic Eraser erases the enclosed area.
        ui.add_enabled_ui(have_doc, |ui| {
            help(crate::widgets::checkbox(ui, &mut eraser, tl!("Add Holes (Magic Eraser)")), cmd, "holeEraser");
        });
        if eraser {
            let top = ui.cursor().top();
            let mut tolerance = app.ui.tool_options.tolerance;
            crate::widgets::slider_row(ui, tl!("Eraser Tolerance"), &mut tolerance, 0.0..=255.0, "", None);
            app.ui.tool_options.tolerance = tolerance.round();
            crate::dtf_help::tip_over(ui, cmd, "eraserTolerance", top);
        }
    }
    if retouch {
        let mut show_before = f.get("__before").and_then(Value::as_bool).unwrap_or(false);
        help(crate::widgets::checkbox(ui, &mut show_before, tl!("Show Before")), cmd, "showBefore");
        f.insert("__before".into(), json!(show_before));
    }
    let changed = f.iter().any(|(k, v)| !k.starts_with("__") && before.get(k) != Some(v));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        if cmd == "dtf.controlBackground" {
            // Show (or recolour) the control background, and remove it, from the same tab.
            ui.add_enabled_ui(enabled, |ui| {
                apply_it = help(crate::widgets::primary_button(ui, tl!("Show Background"), 130.0), cmd, "showBackground").clicked();
            });
            ui.add_enabled_ui(have_doc && has_backfill, |ui| {
                remove_bg = help(crate::widgets::secondary_button(ui, tl!("Delete Background"), 130.0), cmd, "deleteBackground").clicked();
            });
        } else {
            ui.add_enabled_ui(enabled, |ui| {
                apply_it = help(crate::widgets::primary_button(ui, tl!("Apply"), 110.0), cmd, "apply").clicked();
            });
        }
        if previewing {
            cancel = help(crate::widgets::secondary_button(ui, tl!("Cancel"), 110.0), cmd, "cancel").clicked();
        }
    });
    app.ui.dtf_panel.fields.insert(cmd.to_string(), f);
    if cmd == photocraft_engine::dtf_cmds::REMOVE_WHITE && eraser != app.ui.dtf_panel.hole_eraser.is_some() {
        set_hole_eraser(app, eraser);
    }
    if changed && enabled && !WITHOUT_PREVIEW.contains(&cmd) {
        app.ui.dtf_panel.previewing = Some(cmd.to_string());
    }
    if apply_it {
        apply(app, cmd);
    } else if remove_bg {
        if app.ui.dtf_panel.previewing.as_deref() == Some(cmd) {
            stop_preview(app);
        }
        if let Err(error) = app.run("dtf.removeControlBackground", json!({})) {
            crate::notices::error(app, error);
        }
    } else if cancel {
        stop_preview(app);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    #[test]
    fn every_tool_is_a_command_with_a_title() {
        for cmd in TOOLS {
            assert!(photocraft_engine::commands::find(cmd).is_some(), "{cmd}");
            assert!(!title(cmd).ends_with('…'), "{cmd}");
        }
    }

    #[test]
    fn dtf_craft_opens_on_the_dtf_workspace_and_a_preview_stops_with_its_tab() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        dtf_craft_startup(&mut app);
        assert_eq!(app.ui.workspace, "DTF");
        let layout = app.ui.docking.clone().expect("the DTF workspace's layout");
        assert_eq!(layout, dtf_layout());
        assert!(crate::panel_docking::valid(&layout));
        // Groups of three tabs at most; Channels and Paths stay out of the right column.
        let mut pending: Vec<_> = layout.root.iter().collect();
        let mut groups = Vec::new();
        while let Some(node) = pending.pop() {
            match node {
                craft_ui::docking::Node::Split { first, second, .. } => pending.extend([first.as_ref(), second.as_ref()]),
                craft_ui::docking::Node::Tabs { panels, .. } => groups.push(panels.clone()),
                _ => {}
            }
        }
        assert_eq!(groups.len(), 3);
        assert!(groups.iter().all(|g| g.len() <= 3), "{groups:?}");
        let all: Vec<&String> = groups.iter().flatten().collect();
        assert_eq!(all.len(), TOOLS.len() + 1, "every DTF tool and Layers: {all:?}");
        assert!(!all.iter().any(|p| *p == "channels" || *p == "paths"));
        let p = &app.ui.panels;
        assert!(p.dtf && p.layers && !p.color && !p.properties && !p.history && !p.navigator);
        // Essentials keeps Channels and Paths out too.
        crate::menus::invoke(&mut app, &ctx, "window.workspace.essentials", json!({})).unwrap();
        let shown: Vec<&str> = app.ui.dock.visible_tabs(crate::dock::Group::Layers, false).into_iter().map(|(_, n)| n).collect();
        assert_eq!(shown, ["Layers"]);
        crate::menus::invoke(&mut app, &ctx, "window.workspace.dtf", json!({})).unwrap();
        assert_eq!(app.ui.docking, Some(dtf_layout()));
        // A preview shows while its tool's tab is the active one of its group.
        app.run("file.new", json!({"width": 20, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        app.ui.dtf_panel.fields.insert("dtf.saturation".into(), initial_fields(&app, "dtf.saturation"));
        app.ui.dtf_panel.previewing = Some("dtf.saturation".into());
        assert!(preview_source(&app).is_none(), "Saturation is behind Control Background");
        app.run("window.panel.activate", json!({"panel": "Saturation"})).unwrap();
        assert_eq!(preview_source(&app).map(|p| p.0), Some(PREVIEW_DIALOG));
        // Window › Saturation closes its tab, and the preview with it.
        crate::menus::invoke(&mut app, &ctx, "window.panel.dtfSaturation", json!({})).unwrap();
        assert!(preview_source(&app).is_none());
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
        app.ui.panels.dtf = true;
        let cmd = "filter.dtf.removeSemiTransparent";
        // The tool's tab is the one on screen.
        app.ui.dock_tabs.dtf = TOOLS.iter().position(|c| *c == cmd).unwrap();
        let mut f = initial_fields(&app, cmd);
        f.insert("threshold".into(), json!(50));
        app.ui.dtf_panel.fields.insert(cmd.into(), f);
        app.ui.dtf_panel.previewing = Some(cmd.into());
        assert!(crate::canvas::ensure_filter_preview(&mut app, 0, &ctx).is_some());
        let preview = app.filter_preview.as_ref().unwrap();
        assert_eq!(preview.key.dialog, PREVIEW_DIALOG);
        let doc = preview.result.as_ref().unwrap();
        let layer = doc.layer(doc.layers[0].id).unwrap().surface().unwrap();
        assert!((0..32).all(|x| [0.0, 1.0].contains(&layer.pixel(x, 4)[3])), "the preview is binary");
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
        app.ui.panels.dtf = true;
        let cmd = "filter.dtf.removeWhiteBackground";
        app.ui.dtf_panel.fields.insert(cmd.into(), initial_fields(&app, cmd));
        // The tool's tab is the one on screen.
        app.ui.dock_tabs.dtf = TOOLS.iter().position(|c| *c == cmd).unwrap();
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
        app.ui.panels.dtf = true;
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

    /// The interior holes: on, the Magic Eraser (contiguous) erases the enclosed area clicked;
    /// off, the previous tool comes back.
    #[test]
    fn the_hole_eraser_erases_interior_holes() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.run("file.new", json!({"width": 30, "height": 30, "resolution": 300, "background": "transparent"})).unwrap();
        app.run("select.rect", json!({"x": 0, "y": 0, "width": 30, "height": 30})).unwrap();
        app.run("edit.fill", json!({"color": "#ff0000"})).unwrap();
        app.run("select.rect", json!({"x": 10, "y": 10, "width": 10, "height": 10})).unwrap();
        app.run("edit.fill", json!({"color": "#ffffff"})).unwrap();
        app.run("select.deselect", json!({})).unwrap();
        app.ui.tool = crate::state::Tool::Brush;
        app.ui.tool_options.contiguous = false;
        set_hole_eraser(&mut app, true);
        assert_eq!(app.ui.tool, crate::state::Tool::MagicEraser);
        assert!(app.ui.tool_options.contiguous, "only the enclosed area clicked");
        assert!(crate::eraser_ui::click(&mut app, crate::state::Tool::MagicEraser, 15.0, 15.0));
        let st = app.session.active().unwrap();
        let layer = st.doc.layer(st.active_layer.unwrap()).unwrap().surface().unwrap();
        assert_eq!((layer.pixel(15, 15)[3], layer.pixel(2, 2)[3]), (0.0, 1.0));
        set_hole_eraser(&mut app, false);
        assert_eq!(app.ui.tool, crate::state::Tool::Brush);
    }

    /// The cut-out previews on the colour picked in its tab.
    #[test]
    fn the_cutout_previews_on_the_colour_picked() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        app.background_jobs = false;
        app.run("file.new", json!({"width": 30, "height": 20, "resolution": 300, "background": "white"})).unwrap();
        app.ui.panels.dtf = true;
        let cmd = photocraft_engine::dtf_cmds::REMOVE_WHITE;
        app.ui.dock_tabs.dtf = tab_of(cmd);
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
        app.ui.panels.dtf = true;
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
        h.state_mut().ui.dock_tabs.dtf = sat;
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
        app.ui.panels.dtf = true;
        let sat = tab_of("dtf.saturation");
        app.ui.dock_tabs.dtf = sat;
        let mut h = tabs_harness(app, move |ui, app| tool_body(app, ui, sat));
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
        app.ui.panels.dtf = true;
        set_last_color(&mut app, "#112233");
        let cut = tab_of(photocraft_engine::dtf_cmds::REMOVE_WHITE);
        app.ui.dock_tabs.dtf = cut;
        let mut h = tabs_harness(app, move |ui, app| tool_body(app, ui, cut));
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
            let mut h = egui_kittest::Harness::builder().with_size(vec2(1440.0, 900.0)).with_max_steps(64).wgpu().build_eframe(move |cc| {
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
            // The beta notice shows on its own shot only.
            if name != "beta" {
                h.state_mut().ui.dtf_panel.beta_notice = false;
            }
            setup(h.state_mut());
            // Previews compute off the UI thread: give them time to land.
            for _ in 0..40 {
                h.run_steps(1);
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
            h.render().unwrap().save(dir.join(format!("{name}.png"))).unwrap();
        };
        shot(photocraft_engine::Session::new(), "beta", &|_| {});
        shot(photocraft_engine::Session::new(), "accueil", &|_| {});
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
            app.run("window.panel.activate", json!({"panel": "Transparent Pixels"})).unwrap();
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
            app.run("window.panel.activate", json!({"panel": "Cutout"})).unwrap();
            let cmd = "filter.dtf.removeWhiteBackground";
            let mut f = initial_fields(app, cmd);
            f.insert("tolerance".into(), json!(12));
            app.ui.dtf_panel.fields.insert(cmd.into(), f);
            app.ui.dtf_panel.previewing = Some(cmd.into());
        });
        shot(visual(), "conformite", &|app| {
            app.run("window.panel.activate", json!({"panel": "Conformity"})).unwrap();
        });
        // Saturation opened: its preview shows without moving the slider.
        shot(visual(), "saturation", &|app| {
            let mut f = initial_fields(app, "dtf.saturation");
            f.insert("saturation".into(), json!(-100));
            app.ui.dtf_panel.fields.insert("dtf.saturation".into(), f);
            app.ui.dtf_panel.first_pass = Some(0);
            app.run("window.panel.activate", json!({"panel": "Saturation"})).unwrap();
        });
        // The help sheet open in the cut-out's tab.
        shot(on_white(), "aide", &|app| {
            app.run("window.panel.activate", json!({"panel": "Cutout"})).unwrap();
            let cmd = "filter.dtf.removeWhiteBackground";
            let mut f = initial_fields(app, cmd);
            f.insert("__help".into(), json!(true));
            app.ui.dtf_panel.fields.insert(cmd.into(), f);
        });
        let mut halftoned = visual();
        halftoned.execute("filter.dtf.halftone", json!({"mode": "semiTransparent", "frequency": 25})).unwrap();
        shot(halftoned, "halftone", &|_| {});
    }
}
