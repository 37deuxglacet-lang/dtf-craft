// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF Craft › Window › DTF Settings ("Réglages DTF"): a floating panel with one folding section
//! per DTF preparation tool. Each section shows the tool's settings (the same schema as its DTF
//! menu dialog) and an Apply button; moving a setting previews the result on the canvas, through
//! the filter-dialog preview (`canvas::ensure_filter_preview`), until Apply or Cancel.
//!
//! The menu entries and their dialogs stay in the DTF menu: the panel runs the same commands.

use std::collections::BTreeMap;

use egui::{RichText, Sense, vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;

/// Window › DTF Settings.
pub const MENU_ID: &str = "window.panel.dtfSettings";

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

/// Tools applied on Apply only: Print Conformity resizes the image, Remove Control Background
/// has no setting.
const WITHOUT_PREVIEW: [&str; 2] = ["dtf.conformity", "dtf.removeControlBackground"];

const WIDTH: f32 = 340.0;

/// Panel view state. The settings live in `fields` while the app runs and are saved with the
/// dialogs' remembered values when applied.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct DtfPanelUi {
    #[serde(default)]
    pub open: bool,
    /// Commands whose section is unfolded.
    #[serde(default)]
    pub expanded: Vec<String>,
    #[serde(skip)]
    pub fields: BTreeMap<String, Map<String, Value>>,
    /// The section whose settings are previewed on the canvas.
    #[serde(skip)]
    pub previewing: Option<String>,
}

/// Menu checkmark for Window › DTF Settings.
pub fn checked(app: &PhotocraftApp, id: &str) -> Option<bool> {
    (id == MENU_ID).then_some(app.ui.dtf_panel.open)
}

pub fn handles(id: &str) -> bool {
    id == MENU_ID
}

/// Toggle the panel.
pub fn menu(app: &mut PhotocraftApp, id: &str, _params: &Value) -> Option<Result<Value, String>> {
    if id != MENU_ID {
        return None;
    }
    app.ui.dtf_panel.open = !app.ui.dtf_panel.open;
    if !app.ui.dtf_panel.open {
        stop_preview(app);
    }
    Some(Ok(json!({ "open": app.ui.dtf_panel.open })))
}

/// The section title: the tool's DTF menu label without its "…".
fn title(command: &str) -> String {
    let label = photocraft_engine::commands::find(command).map_or(command, |c| c.label);
    tl!(label).trim_end_matches('…').to_string()
}

/// The panel title.
pub fn panel_title() -> &'static str {
    tl!("DTF Settings")
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
    if !app.ui.dtf_panel.open {
        return None;
    }
    let cmd = app.ui.dtf_panel.previewing.as_ref()?;
    let f = app.ui.dtf_panel.fields.get(cmd)?;
    Some((PREVIEW_DIALOG, f.clone()))
}

/// Stop the panel's canvas preview (Cancel, Apply, folding the section, closing the panel).
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

enum Act {
    Toggle(String),
    Preview(String),
    Apply(String),
    Cancel,
    Close,
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

/// One section header: chevron, folder icon and the tool's name; a click folds or unfolds it.
fn header(ui: &mut egui::Ui, name: &str, open: bool) -> bool {
    let t = Tokens::get(ui.ctx());
    let (rect, response) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::click());
    if response.hovered() {
        ui.painter().rect_filled(rect, 3.0, t.hover);
    }
    let mut x = rect.left() + 2.0;
    let icon = |x: f32| egui::Rect::from_center_size(egui::pos2(x + 8.0, rect.center().y), vec2(14.0, 14.0));
    crate::icons::paint(ui, icon(x), if open { "chevron-down" } else { "chevron-right" }, 12.0, t.text_dim);
    x += 18.0;
    crate::icons::paint(ui, icon(x), "folder", 13.0, t.text_dim);
    x += 22.0;
    ui.painter().text(egui::pos2(x, rect.center().y), egui::Align2::LEFT_CENTER, name, egui::FontId::proportional(13.0), t.text);
    response.clicked()
}

/// Draw the panel.
pub fn windows(app: &mut PhotocraftApp, ctx: &egui::Context) {
    if !app.ui.dtf_panel.open {
        return;
    }
    for cmd in TOOLS {
        if !app.ui.dtf_panel.fields.contains_key(cmd) {
            let f = initial_fields(app, cmd);
            app.ui.dtf_panel.fields.insert(cmd.to_string(), f);
        }
    }
    let have_doc = app.session.active().is_some();
    let enabled: Vec<bool> = TOOLS.iter().map(|c| have_doc && app.session.is_enabled(c)).collect();
    let mut fields = std::mem::take(&mut app.ui.dtf_panel.fields);
    let expanded = app.ui.dtf_panel.expanded.clone();
    let previewing = app.ui.dtf_panel.previewing.clone();
    let mut act: Option<Act> = None;
    let panel_name = panel_title();
    crate::analysis_ui::panel_window(app, ctx, "dtf-settings", panel_name, vec2(0.0, 12.0), WIDTH, |ui| {
        if crate::analysis_ui::title_row(ui, panel_name) {
            act = Some(Act::Close);
        }
        if !have_doc {
            ui.weak(tl!("Open a document to use the DTF settings."));
            ui.add_space(4.0);
        }
        egui::ScrollArea::vertical().id_salt("dtf-settings-scroll").max_height((ctx.content_rect().height() - 160.0).max(240.0)).min_scrolled_height(420.0).auto_shrink([false, true]).show(ui, |ui| {
            for (i, cmd) in TOOLS.iter().enumerate() {
                let open = expanded.iter().any(|e| e == cmd);
                if header(ui, &title(cmd), open) {
                    act = Some(Act::Toggle(cmd.to_string()));
                }
                if !open {
                    continue;
                }
                let Some(f) = fields.get_mut(*cmd) else { continue };
                ui.indent(("dtf-section", *cmd), |ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    let before = f.clone();
                    if *cmd == "dtf.controlBackground" {
                        color_row(ui, f);
                    } else if *cmd != "dtf.removeControlBackground" {
                        crate::filter_dialog::body(ui, f);
                    }
                    if *f != before && !WITHOUT_PREVIEW.contains(cmd) && enabled[i] {
                        act = Some(Act::Preview(cmd.to_string()));
                    }
                    ui.add_space(4.0);
                    ui.horizontal(|ui| {
                        ui.add_enabled_ui(enabled[i], |ui| {
                            if crate::widgets::primary_button(ui, tl!("Apply"), 110.0).clicked() {
                                act = Some(Act::Apply(cmd.to_string()));
                            }
                        });
                        if previewing.as_deref() == Some(*cmd) && crate::widgets::secondary_button(ui, tl!("Cancel"), 110.0).clicked() {
                            act = Some(Act::Cancel);
                        }
                    });
                    ui.add_space(6.0);
                });
            }
        });
    });
    app.ui.dtf_panel.fields = fields;
    match act {
        Some(Act::Close) => {
            app.ui.dtf_panel.open = false;
            stop_preview(app);
        }
        Some(Act::Toggle(cmd)) => {
            if let Some(i) = app.ui.dtf_panel.expanded.iter().position(|e| *e == cmd) {
                app.ui.dtf_panel.expanded.remove(i);
                if app.ui.dtf_panel.previewing.as_deref() == Some(cmd.as_str()) {
                    stop_preview(app);
                }
            } else {
                app.ui.dtf_panel.expanded.push(cmd);
            }
        }
        Some(Act::Preview(cmd)) => app.ui.dtf_panel.previewing = Some(cmd),
        Some(Act::Apply(cmd)) => apply(app, &cmd),
        Some(Act::Cancel) => stop_preview(app),
        None => {}
    }
    // A preview runs while its section shows settings for the active document only.
    if app.ui.dtf_panel.previewing.is_some() {
        ctx.request_repaint();
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
    fn the_menu_toggles_the_panel_and_a_preview_stops_with_it() {
        let mut app = PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        crate::menus::invoke(&mut app, &ctx, MENU_ID, json!({})).unwrap();
        assert!(app.ui.dtf_panel.open);
        assert_eq!(checked(&app, MENU_ID), Some(true));
        app.ui.dtf_panel.fields.insert("dtf.saturation".into(), initial_fields(&app, "dtf.saturation"));
        app.ui.dtf_panel.previewing = Some("dtf.saturation".into());
        assert_eq!(preview_source(&app).map(|p| p.0), Some(PREVIEW_DIALOG));
        crate::menus::invoke(&mut app, &ctx, MENU_ID, json!({})).unwrap();
        assert!(!app.ui.dtf_panel.open);
        assert!(preview_source(&app).is_none());
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
        app.ui.dtf_panel.open = true;
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
                PhotocraftApp::new(session, crate::Services::default())
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
            app.ui.dtf_panel.open = true;
            app.ui.dtf_panel.expanded = vec!["dtf.controlBackground".into(), "filter.dtf.removeSemiTransparent".into(), "dtf.saturation".into()];
        });
        shot(visual(), "apercu", &|app| {
            app.ui.dtf_panel.open = true;
            app.ui.dtf_panel.expanded = vec!["filter.dtf.removeSemiTransparent".into()];
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
