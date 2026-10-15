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

/// The tools, top to bottom.
pub const TOOLS: [&str; 7] = [
    "dtf.conformity",
    "dtf.controlBackground",
    "dtf.removeControlBackground",
    "filter.dtf.removeSemiTransparent",
    "dtf.defringe",
    "dtf.sharpen",
    "dtf.saturation",
];

/// The tabs' labels (and dock panel names), in [`TOOLS`] order.
pub const TABS: [&str; 7] = ["Conformity", "Control Background", "No Background", "Transparency", "Fringe", "Sharpness", "Saturation"];

/// Tools applied on Apply only: Print Conformity resizes the image, Remove Control Background
/// has no setting.
const WITHOUT_PREVIEW: [&str; 2] = ["dtf.conformity", "dtf.removeControlBackground"];

/// The DTF tabs' state. The settings live in `fields` while the app runs and are saved with the
/// dialogs' remembered values when applied.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DtfPanelUi {
    #[serde(skip)]
    pub fields: BTreeMap<String, Map<String, Value>>,
    /// The section whose settings are previewed on the canvas.
    #[serde(skip)]
    pub previewing: Option<String>,
}

/// The section title: the tool's DTF menu label without its "…".
fn title(command: &str) -> String {
    let label = photocraft_engine::commands::find(command).map_or(command, |c| c.label);
    tl!(label).trim_end_matches('…').to_string()
}

/// The layout version DTF Craft last set up (remembered with the preferences): raising it
/// brings the DTF default layout back once, over a layout saved by an older version.
const LAYOUT_VERSION: u64 = 2;
const LAYOUT_PREF: &str = "dtfCraft.layoutVersion";

/// DTF Craft's start: the DTF tools group, and on the first launch of this layout its Essentials
/// workspace (Layers and the DTF tools above it, every other panel in the Window menu).
pub fn dtf_craft_startup(app: &mut PhotocraftApp) {
    app.dtf_craft = true;
    let seen = app.session.prefs().dialogs.get(LAYOUT_PREF).and_then(Value::as_u64).unwrap_or(0);
    if seen < LAYOUT_VERSION {
        app.ui.workspace = "Essentials".into();
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
        for p in &params {
            if let Some(v) = saved.get(&p.key) {
                f.insert(p.key.clone(), v.clone());
            }
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
    let f = app.ui.dtf_panel.fields.get(cmd)?;
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
    let params = app.with_mask_target(command, crate::filter_dialog::params_of(&f));
    let previewed = app.ui.dtf_panel.previewing.as_deref() == Some(command);
    app.ui.dtf_panel.previewing = None;
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
                let saved: Map<String, Value> = saved.into_iter().filter(|(k, _)| k != "layer" && k != "channel").collect();
                app.session.prefs.edit(|prefs| prefs.dialogs.insert(command.into(), Value::Object(saved)));
            }
        }
        Err(error) => crate::notices::error(app, error),
    }
}

/// The Control Background colour row: a swatch with egui's colour picker and the hex value.
fn color_row(ui: &mut egui::Ui, f: &mut Map<String, Value>) -> bool {
    let t = Tokens::get(ui.ctx());
    let hex = f.get("color").and_then(Value::as_str).unwrap_or("#231c1c").to_string();
    let rgb = crate::color_picker_ui::parse_hex(&hex).unwrap_or([0.137, 0.11, 0.11]);
    let mut bytes = rgb.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    let mut changed = false;
    ui.horizontal(|ui| {
        ui.label(RichText::new(crate::filter_dialog::label("color")).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if egui::color_picker::color_edit_button_srgb(ui, &mut bytes).changed() {
                changed = true;
            }
            ui.label(RichText::new(&hex).color(t.text_dim).monospace());
        });
    });
    if changed {
        f.insert("color".into(), json!(format!("#{:02x}{:02x}{:02x}", bytes[0], bytes[1], bytes[2])));
    }
    changed
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
    // A preview belongs to the tab that shows it: another tab takes it away.
    if app.ui.dtf_panel.previewing.as_deref().is_some_and(|p| TOOLS.contains(&p) && p != cmd) {
        stop_preview(app);
    }
    let have_doc = app.session.active().is_some();
    let enabled = have_doc && app.session.is_enabled(cmd);
    let note = if cmd == "dtf.conformity" { current_size_note(app) } else { None };
    let previewing = app.ui.dtf_panel.previewing.as_deref() == Some(cmd);
    let Some(mut f) = app.ui.dtf_panel.fields.remove(cmd) else { return };
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
    let before = f.clone();
    if cmd == "dtf.controlBackground" {
        color_row(ui, &mut f);
    } else if cmd != "dtf.removeControlBackground" {
        crate::filter_dialog::body(ui, &mut f);
    }
    let changed = f.iter().any(|(k, v)| !k.starts_with("__") && before.get(k) != Some(v));
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_enabled_ui(enabled, |ui| {
            apply_it = crate::widgets::primary_button(ui, tl!("Apply"), 110.0).clicked();
        });
        if previewing {
            cancel = crate::widgets::secondary_button(ui, tl!("Cancel"), 110.0).clicked();
        }
    });
    app.ui.dtf_panel.fields.insert(cmd.to_string(), f);
    if changed && enabled && !WITHOUT_PREVIEW.contains(&cmd) {
        app.ui.dtf_panel.previewing = Some(cmd.to_string());
    }
    if apply_it {
        apply(app, cmd);
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
    fn the_dtf_tools_open_beside_layers_and_a_preview_stops_with_them() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        dtf_craft_startup(&mut app);
        // DTF Craft opens on Layers and the DTF group above it, nothing else.
        let p = &app.ui.panels;
        assert!(p.dtf && p.layers && !p.color && !p.properties && !p.history && !p.navigator);
        let order = app.ui.dock.order();
        let at = |g| order.iter().position(|x| *x == g).unwrap();
        assert!(at(crate::dock::Group::Dtf) < at(crate::dock::Group::Layers), "DTF above Layers: {order:?}");
        assert_eq!(crate::dock::Group::Dtf.tabs(false).len(), TOOLS.len());
        app.ui.dtf_panel.fields.insert("dtf.saturation".into(), initial_fields(&app, "dtf.saturation"));
        app.ui.dtf_panel.previewing = Some("dtf.saturation".into());
        assert_eq!(preview_source(&app).map(|p| p.0), Some(PREVIEW_DIALOG));
        // Window › Saturation (each DTF tool has its entry) hides and shows the group.
        crate::menus::invoke(&mut app, &ctx, "window.panel.dtfSaturation", json!({})).unwrap();
        assert!(!app.ui.panels.dtf);
        assert!(preview_source(&app).is_none());
        crate::menus::invoke(&mut app, &ctx, "window.panel.dtfSaturation", json!({})).unwrap();
        assert!(app.ui.panels.dtf);
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
                dtf_craft_startup(&mut app);
                app
            });
            h.state_mut().session.prefs.edit(|p| p.interface.language = "fr".into());
            h.run_steps(6);
            setup(h.state_mut());
            h.run_steps(12);
            h.render().unwrap().save(dir.join(format!("{name}.png"))).unwrap();
        };
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
        shot(visual(), "panneau", &|app| {
            app.ui.dock_tabs.dtf = 0;
        });
        shot(visual(), "apercu", &|app| {
            app.ui.dock_tabs.dtf = 3;
            let mut f = initial_fields(app, "filter.dtf.removeSemiTransparent");
            f.insert("threshold".into(), json!(80));
            app.ui.dtf_panel.fields.insert("filter.dtf.removeSemiTransparent".into(), f);
            app.ui.dtf_panel.previewing = Some("filter.dtf.removeSemiTransparent".into());
        });
        let mut halftoned = visual();
        halftoned.execute("filter.dtf.halftone", json!({"mode": "semiTransparent", "frequency": 25})).unwrap();
        shot(halftoned, "halftone", &|_| {});
    }
}
