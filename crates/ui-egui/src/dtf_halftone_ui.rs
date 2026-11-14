// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF Halftone's dialog: an Automatic tab (one preset per Photoshop action, and the optional
//! "RasterFx" retouching layers) and a Manual tab (the screen settings, locked when the dialog
//! opens, with Reset). The tab follows the `preset` param: "none" is Manual.

use egui::RichText;
use serde_json::{Map, Value, json};

use crate::filter_dialog::{Kind, parse_spec};
use crate::theme::Tokens;

pub const COMMAND: &str = photocraft_engine::dtf_cmds::HALFTONE;

/// The preset the Automatic tab starts on.
pub const DEFAULT_PRESET: &str = "teeBlack";

/// The presets' cards, in order: (id, name, the screen in short).
fn presets() -> [(&'static str, String, String); 5] {
    let levels = tl!("Levels");
    let round = tl!("Round");
    [
        ("teeBlack", tl!("Black T-Shirt").to_string(), format!("{levels} ×2 · 25 lpi · 22° · {round}")),
        ("teeBlackSoft", tl!("Soft Black T-Shirt").to_string(), format!("{levels} ×1 · 25 lpi · 22° · {round}")),
        ("teeWhite", tl!("White T-Shirt").to_string(), format!("{levels} ×2 · 25 lpi · 22° · {round}")),
        ("blackWhite", tl!("Black and White").to_string(), format!("55 lpi · 22° · {round}")),
        ("uniform", tl!("Uniform").to_string(), format!("25 lpi · 22° · {round}")),
    ]
}

/// The params only the Automatic tab shows.
const AUTO_KEYS: [&str; 2] = ["preset", "rasterFx"];

/// Whether `key` is drawn by the dialog's own rows (Manual) rather than by this module.
pub fn manual_row(key: &str) -> bool {
    !AUTO_KEYS.contains(&key)
}

/// The tabs and, in Automatic, the presets; in Manual, the lock row. Draws the "?" and the
/// sheet (the dialog's own "?" is left out, `dtf_help::BAR`). Returns (automatic, locked): in
/// Automatic the dialog draws none of its rows; in Manual they are greyed while locked.
pub fn header(ui: &mut egui::Ui, f: &mut Map<String, Value>) -> (bool, bool) {
    f.insert(crate::dtf_help::BAR.into(), json!(true));
    let preset = f.get("preset").and_then(Value::as_str).unwrap_or("none").to_string();
    let mut auto = preset != "none";
    let t = Tokens::get(ui.ctx());
    // The tabs: Automatic | Manual, the dialog's width.
    ui.horizontal(|ui| {
        let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(60.0);
        for (on, label) in [(true, tl!("Automatic")), (false, tl!("Manual"))] {
            let b = egui::Button::new(RichText::new(label)).selected(auto == on).min_size(egui::vec2(w, 26.0));
            if ui.add(b).clicked() && auto != on {
                if on {
                    let last = f.get("__lastPreset").and_then(Value::as_str).unwrap_or(DEFAULT_PRESET).to_string();
                    f.insert("preset".into(), json!(last));
                } else {
                    f.insert("__lastPreset".into(), json!(preset));
                    f.insert("preset".into(), json!("none"));
                }
                auto = on;
            }
        }
    });
    ui.add_space(4.0);
    let mut open = f.get("__help").and_then(Value::as_bool).unwrap_or(false);
    let mut locked = false;
    if auto {
        ui.allocate_ui_with_layout(egui::vec2(ui.available_width(), 22.0), egui::Layout::right_to_left(egui::Align::Center), |ui| {
            crate::dtf_help::sheet_button(ui, &mut open, COMMAND);
        });
    } else {
        let mut unlocked = f.get("__unlocked").and_then(Value::as_bool).unwrap_or(false);
        if crate::dtf_help::lock_row(ui, COMMAND, &mut unlocked, &mut open) {
            factory_manual(f);
        }
        f.insert("__unlocked".into(), json!(unlocked));
        locked = !unlocked;
    }
    f.insert("__help".into(), json!(open));
    if open {
        crate::dtf_help::sheet_content(ui, COMMAND);
    }
    if !auto {
        return (false, locked);
    }
    // The presets: two cards a row, the chosen one pressed.
    let current = f.get("preset").and_then(Value::as_str).unwrap_or(DEFAULT_PRESET).to_string();
    let w = ((ui.available_width() - ui.spacing().item_spacing.x) / 2.0).max(80.0);
    for row in presets().chunks(2) {
        ui.horizontal(|ui| {
            for (id, name, screen) in row {
                let mut job = egui::text::LayoutJob::default();
                job.append(name, 0.0, egui::TextFormat { font_id: egui::FontId::proportional(13.5), color: t.text, ..Default::default() });
                job.append(
                    &format!("\n{screen}"),
                    0.0,
                    egui::TextFormat { font_id: egui::FontId::proportional(11.0), color: t.text_dim, ..Default::default() },
                );
                job.wrap.max_width = w - 16.0;
                let b = egui::Button::new(job).selected(current == *id).min_size(egui::vec2(w, 46.0));
                let r = ui.add_sized(egui::vec2(w, 46.0), b);
                let r = match crate::dtf_help::tip(COMMAND, id) {
                    Some(text) => r.on_hover_text(text),
                    None => r,
                };
                if r.clicked() {
                    f.insert("preset".into(), json!(id));
                }
            }
        });
    }
    ui.add_space(4.0);
    let mut fx = f.get("rasterFx").and_then(Value::as_bool).unwrap_or(false);
    let r = crate::widgets::checkbox(ui, &mut fx, tl!("Retouch Layers (Brightness +32, Saturation +16), Hidden"));
    if let Some(text) = crate::dtf_help::tip(COMMAND, "rasterFx") {
        r.on_hover_text(text);
    }
    f.insert("rasterFx".into(), json!(fx));
    (true, false)
}

/// Reset in the Manual tab: the screen settings' factory values (the preset stays "none").
fn factory_manual(f: &mut Map<String, Value>) {
    let Some(spec) = photocraft_engine::commands::find(COMMAND) else { return };
    for p in parse_spec(spec.params) {
        if !manual_row(&p.key) {
            continue;
        }
        let v = match p.kind {
            Kind::Range { default, .. } => json!(default),
            Kind::Choice { options, default } => json!(default.or_else(|| options.first().cloned()).unwrap_or_default()),
            Kind::Bool(default) => json!(default),
            _ => continue,
        };
        f.insert(p.key, v);
    }
}

/// The dialog's first values: the Automatic tab on its first preset, unless a preset (or
/// Manual) was remembered from the last time.
pub fn defaults(remembered: Option<&Value>, fields: &mut Map<String, Value>) {
    if remembered.and_then(|r| r.get("preset")).is_none() {
        fields.insert("preset".into(), json!(DEFAULT_PRESET));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::vec2;

    fn fields() -> Map<String, Value> {
        let mut f = Map::new();
        f.insert("__command".into(), json!(COMMAND));
        for p in parse_spec(photocraft_engine::commands::find(COMMAND).unwrap().params) {
            let v = match p.kind {
                Kind::Range { default, .. } => json!(default),
                Kind::Choice { options, default } => json!(default.or_else(|| options.first().cloned()).unwrap_or_default()),
                Kind::Bool(default) => json!(default),
                _ => continue,
            };
            f.insert(p.key, v);
        }
        defaults(None, &mut f);
        f
    }

    fn harness(f: Map<String, Value>) -> egui_kittest::Harness<'static, Map<String, Value>> {
        egui_kittest::Harness::builder().with_size(vec2(380.0, 900.0)).build_ui_state(
            |ui, f: &mut Map<String, Value>| {
                let ready = egui::Id::new("halftone-test-fonts");
                if !ui.ctx().data(|d| d.get_temp::<bool>(ready)).unwrap_or(false) {
                    crate::PhotocraftApp::setup_context(ui.ctx(), crate::theme::ThemeKind::ALL[0]);
                    ui.ctx().data_mut(|d| d.insert_temp(ready, true));
                    return;
                }
                crate::filter_dialog::body(ui, f);
            },
            f,
        )
    }

    /// The dialog opens on Automatic (the first preset); a card picks its preset, the manual
    /// rows stay hidden; Manual shows them locked, Reset brings their factory values once
    /// unlocked, and Automatic comes back on the preset left.
    #[test]
    fn automatic_presets_and_a_locked_manual_tab() {
        use egui_kittest::kittest::Queryable;
        let mut h = harness(fields());
        h.run_steps(3);
        assert_eq!(h.state()["preset"], json!("teeBlack"));
        assert!(h.query_by_label("Frequency").is_none(), "no manual rows in Automatic");
        h.get_by_label_contains("White T-Shirt").click();
        h.run_steps(2);
        assert_eq!(h.state()["preset"], json!("teeWhite"));
        h.get_by_label_contains("Retouch Layers").click();
        h.run_steps(2);
        assert_eq!(h.state()["rasterFx"], json!(true));
        // Manual: the screen settings, locked.
        h.get_by_label("Manual").click();
        h.run_steps(2);
        assert_eq!(h.state()["preset"], json!("none"));
        assert!(h.query_by_label("Frequency").is_some());
        assert!(h.query_by_label("Locked").is_some());
        assert!(h.query_by_label_contains("Retouch Layers").is_none());
        h.state_mut().insert("frequency".into(), json!(60));
        h.get_by_label("Reset").click();
        h.run_steps(2);
        assert_eq!(h.state()["frequency"].as_f64(), Some(60.0), "locked: Reset waits");
        h.get_by_label("Locked").click();
        h.run_steps(2);
        h.get_by_label("Reset").click();
        h.run_steps(2);
        assert_eq!(h.state()["frequency"].as_f64(), Some(25.0), "the factory value");
        assert_eq!(h.state()["preset"], json!("none"), "Reset stays in Manual");
        // Back to Automatic: the preset left.
        h.get_by_label("Automatic").click();
        h.run_steps(2);
        assert_eq!(h.state()["preset"], json!("teeWhite"));
        // The params sent: the preset, the option; no private keys.
        let p = crate::filter_dialog::params_of(h.state());
        assert_eq!(p["preset"], json!("teeWhite"));
        assert!(p.as_object().unwrap().keys().all(|k| !k.starts_with("__")));
    }

    #[test]
    fn a_remembered_choice_is_kept() {
        let mut f = Map::new();
        f.insert("preset".into(), json!("none"));
        defaults(Some(&json!({"preset": "none"})), &mut f);
        assert_eq!(f["preset"], json!("none"));
        defaults(Some(&json!({"frequency": 30})), &mut f);
        assert_eq!(f["preset"], json!(DEFAULT_PRESET));
    }
}
