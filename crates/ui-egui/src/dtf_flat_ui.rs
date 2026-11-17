// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Flat Color…'s own rows in its filter window: the protected colours (picked on the image
//! with the eyedropper, a click on a swatch removes it), "Show Before" and "Highlight Changes"
//! (preview views: never applied).

use egui::{Color32, vec2};
use serde_json::{Map, Value, json};

use crate::theme::Tokens;

/// The command (`filter.dtf.flatColor`).
pub const COMMAND: &str = photocraft_engine::dtf_cmds::FLAT_COLOR;
/// The protected colours' param.
pub const PROTECTED: &str = "protectedColors";
/// The preview shows the image as it is (no preview), while on.
const BEFORE: &str = "__before";
/// The preview paints the pixels changed magenta, while on.
const HIGHLIGHT: &str = "__highlight";
/// Most protected colours.
const MAX_PROTECTED: usize = 16;

/// Whether `f` is Flat Color's window.
pub fn owns(f: &Map<String, Value>) -> bool {
    f.get("__command").and_then(Value::as_str) == Some(COMMAND)
}

fn protected(f: &Map<String, Value>) -> Vec<String> {
    f.get(PROTECTED).and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_str).map(str::to_string).collect()).unwrap_or_default()
}

/// The colour picked on the image joins the protected ones (once).
pub fn add_protected(f: &mut Map<String, Value>, rgb: [f32; 3]) {
    let hex = crate::color_picker_ui::hex(rgb);
    let mut list = protected(f);
    if !list.iter().any(|c| c.eq_ignore_ascii_case(&hex)) && list.len() < MAX_PROTECTED {
        list.push(hex);
    }
    f.insert(PROTECTED.into(), json!(list));
}

/// Whether the canvas shows the image as it is ("Show Before").
pub fn hides_preview(f: &Map<String, Value>) -> bool {
    owns(f) && f.get(BEFORE).and_then(Value::as_bool) == Some(true)
}

/// The preview's own params: "Highlight Changes" paints the pixels changed (OK never does).
pub fn preview_params(f: &Map<String, Value>, params: &mut Value) {
    if owns(f)
        && f.get(HIGHLIGHT).and_then(Value::as_bool) == Some(true)
        && let Some(o) = params.as_object_mut()
    {
        o.insert("highlight".into(), json!(true));
    }
}

/// The protected colours' row (eyedropper, swatches, Clear), then Show Before and Highlight Changes.
pub fn rows(ui: &mut egui::Ui, f: &mut Map<String, Value>) {
    let t = Tokens::get(ui.ctx());
    let list = protected(f);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Protected Colors")).color(t.text_dim));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let armed = f.get(crate::filter_dialog::COLOR_EYEDROPPER).and_then(Value::as_bool).unwrap_or(false);
            if crate::color_range_ui::eyedropper(ui, "+", armed, tl!("Protect a Color Picked on the Image")).clicked() {
                f.insert(crate::filter_dialog::COLOR_EYEDROPPER.into(), json!(!armed));
                f.insert("__tool".into(), json!("sample"));
            }
            let clear = ui.add_enabled_ui(!list.is_empty(), |ui| crate::widgets::secondary_button(ui, tl!("Clear"), 60.0)).inner;
            if clear.on_hover_text(tl!("No protected colour")).clicked() {
                f.insert(PROTECTED.into(), json!([]));
            }
        });
    });
    if list.is_empty() {
        ui.label(egui::RichText::new(tl!("None: click the eyedropper, then a colour to keep (an outline's black).")).size(11.5).color(t.text_faint));
    } else {
        let mut remove = None;
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
            for (i, hex) in list.iter().enumerate() {
                let c = Color32::from_hex(hex).unwrap_or(Color32::BLACK);
                let name = crate::i18n::fmt(tl!("Remove {color}"), &[("color", hex)]);
                if crate::widgets::color_swatch_button(ui, c, &name).on_hover_text(&name).clicked() {
                    remove = Some(i);
                }
            }
        });
        if let Some(i) = remove {
            let mut list = list;
            list.remove(i);
            f.insert(PROTECTED.into(), json!(list));
        }
    }
    let mut before = f.get(BEFORE).and_then(Value::as_bool).unwrap_or(false);
    let r = crate::widgets::checkbox(ui, &mut before, tl!("Show Before"));
    crate::dtf_help::tip_over(ui, COMMAND, "showBefore", r.rect.top());
    if r.changed() {
        f.insert(BEFORE.into(), json!(before));
    }
    let mut highlight = f.get(HIGHLIGHT).and_then(Value::as_bool).unwrap_or(false);
    let r = crate::widgets::checkbox(ui, &mut highlight, tl!("Highlight Changes"));
    crate::dtf_help::tip_over(ui, COMMAND, "highlight", r.rect.top());
    if r.changed() {
        f.insert(HIGHLIGHT.into(), json!(highlight));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The colours picked join once; Highlight Changes reaches the preview only; Show Before
    /// hides it.
    #[test]
    fn protected_colours_and_preview_views() {
        let mut f = Map::new();
        f.insert("__command".into(), json!(COMMAND));
        add_protected(&mut f, [0.0, 0.0, 0.0]);
        add_protected(&mut f, [0.0, 0.0, 0.0]);
        add_protected(&mut f, [1.0, 1.0, 1.0]);
        assert_eq!(f[PROTECTED], json!(["#000000", "#ffffff"]));
        let mut params = crate::filter_dialog::params_of(&f);
        assert!(params.get("highlight").is_none());
        f.insert(HIGHLIGHT.into(), json!(true));
        preview_params(&f, &mut params);
        assert_eq!(params["highlight"], true);
        assert!(crate::filter_dialog::params_of(&f).get("highlight").is_none(), "OK never highlights");
        assert!(!hides_preview(&f));
        f.insert(BEFORE.into(), json!(true));
        assert!(hides_preview(&f));
    }

    /// DTF › Flat Color… opens its filter window; the eyedropper protects the colour clicked on the
    /// image; OK cleans the fringe on the layer.
    #[test]
    fn the_window_protects_a_picked_colour_and_cleans_the_fringe() {
        let mut app = crate::PhotocraftApp::new(photocraft_engine::Session::new(), Default::default());
        let ctx = egui::Context::default();
        app.run("file.new", json!({"width": 40, "height": 40, "resolution": 300, "background": "transparent"})).unwrap();
        app.session
            .edit("Paint", |doc, active| {
                let surf = doc.layer_mut(active.unwrap()).unwrap().surface_mut().unwrap();
                for y in 0..40 {
                    for x in 0..40 {
                        let inside = (10..30).contains(&x) && (10..30).contains(&y);
                        let edge = inside && (x < 12 || x >= 28 || y < 12 || y >= 28);
                        let px = if edge {
                            [0.55, 0.05, 0.05, 1.0]
                        } else if inside {
                            [0.85, 0.1, 0.1, 1.0]
                        } else {
                            [0.1, 0.2, 0.85, 1.0]
                        };
                        surf.write_pixel(x, y, &px);
                    }
                }
                Ok(())
            })
            .unwrap();
        let ids: Vec<&str> = crate::menu_catalog::CATALOG.iter().filter(|(path, _, _, _)| path == &["DTF"]).map(|(_, _, _, id)| *id).collect();
        assert!(ids.contains(&COMMAND), "in the DTF menu");
        crate::menus::invoke(&mut app, &ctx, COMMAND, json!({})).unwrap();
        let id = app.ui.dialogs.last().unwrap().id;
        assert!(owns(&app.ui.dialogs.last().unwrap().fields));
        if let Some(d) = app.ui.dialog_mut(id) {
            d.fields.insert(crate::filter_dialog::COLOR_EYEDROPPER.into(), json!(true));
        }
        assert!(crate::filter_dialog::color_eyedropper_sample(&mut app, 10.5, 20.5, egui::Modifiers::NONE));
        let f = app.ui.dialogs.last().unwrap().fields.clone();
        assert_eq!(protected(&f).len(), 1, "{f:?}");
        if let Some(d) = app.ui.dialog_mut(id) {
            d.fields.insert(PROTECTED.into(), json!([]));
            d.fields.insert("autoColors".into(), json!(false));
            d.fields.insert("colorCount".into(), json!(2));
            d.fields.insert(HIGHLIGHT.into(), json!(true));
        }
        crate::dialogs::confirm(&mut app, id).unwrap();
        let st = app.session.active().unwrap();
        let layer = st.active_layer.unwrap();
        let p = st.doc.layer(layer).unwrap().surface().unwrap().read_region(photocraft_geom::Rect::new(10, 20, 11, 21));
        assert!((p[0] - 0.85).abs() < 0.02 && p[1] < 0.2, "the red, not magenta: {p:?}");
    }
}
