// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF Craft's start screen: the Pixabay illustration behind it (`home_image`), the app icon and
//! name, its version and base, New and Open, then Stéphane Glacet's sites (`links::home_links`).
//! PhotoCraft's `canvas::start_screen` hands over to it when `PhotocraftApp::dtf_craft` is on,
//! so canvas.rs keeps PhotoCraft's own start screen untouched.

use egui::{Rect, Sense};

use crate::PhotocraftApp;

pub(crate) fn start_screen(app: &mut PhotocraftApp, ui: &mut egui::Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let area = ui.available_rect_before_wrap();
    // DTF Craft: an illustration from Pixabay behind the start screen, else the dots.
    crate::home_image::start(app);
    crate::home_image::poll(app, ui.ctx());
    if !crate::home_image::paint(app, ui, area) {
        crate::canvas::paint_dots(ui, area, app.session.prefs().interface.show_canvas_dots);
    }
    // DTF Craft: a sober start screen, the card and the links under it, centred.
    let card = Rect::from_center_size(area.center(), egui::vec2(460.0, 360.0));
    let new_label = crate::shortcuts::command_label(app, "New document…", "file.new");
    let open_label = crate::shortcuts::command_label(app, "Open…", "file.open");
    ui.scope_builder(egui::UiBuilder::new().max_rect(card), |ui| {
        ui.vertical_centered(|ui| {
            // DTF Craft: the app icon and name, then "based on PhotoCraft · open source".
            ui.horizontal(|ui| {
                let title = ui.painter().layout_no_wrap(crate::brand::APP_NAME.into(), crate::theme::semibold(38.0), t.text);
                let mark = 46.0;
                let total = mark + 12.0 + title.size().x;
                ui.add_space(((card.width() - total) / 2.0).max(0.0));
                let (m, _) = ui.allocate_exact_size(egui::vec2(mark, mark), Sense::hover());
                crate::brand::paint_mark(ui, m);
                ui.add_space(12.0);
                let (r, _) = ui.allocate_exact_size(egui::vec2(title.size().x, mark), Sense::hover());
                ui.painter().galley(egui::pos2(r.left(), r.center().y - title.size().y / 2.0), title, t.text);
            });
            ui.add_space(2.0);
            // DTF Craft: its version, then its base.
            let version = crate::i18n::fmt(tl!("Version {version}"), &[("version", crate::brand::VERSION)]);
            ui.label(egui::RichText::new(format!("{version} · {}", tl!("based on PhotoCraft · open source"))).color(t.text_faint).size(13.0));
            ui.add_space(8.0);
            ui.label(egui::RichText::new(tl!("Create a new document or open an existing file.")).color(t.text_dim).size(14.0));
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                ui.add_space(((card.width() - 2.0 * 190.0 - 12.0) / 2.0).max(0.0));
                ui.spacing_mut().item_spacing.x = 12.0;
                if crate::widgets::primary_button(ui, &new_label, 190.0).clicked() {
                    let fields = app.new_document_fields();
                    app.ui.open_dialog(crate::state::DialogKind::NewDocument, fields);
                }
                if crate::widgets::secondary_button(ui, &open_label, 190.0).clicked() {
                    let _ = app.open_dialog_file();
                }
            });
            ui.add_space(22.0);
            ui.horizontal(|ui| {
                let msg = crate::canvas::start_screen_drop_hint(app);
                let g = ui.painter().layout_no_wrap(msg.to_string(), egui::FontId::proportional(12.5), t.text_faint);
                ui.add_space(((card.width() - g.size().x - 24.0) / 2.0).max(0.0));
                let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), Sense::hover());
                crate::icons::paint(ui, r, "image", 15.0, t.text_faint);
                ui.label(egui::RichText::new(msg).color(t.text_faint));
            });
            // DTF Craft: Stéphane Glacet's sites, then GitHub and PhotoCraft's site.
            ui.add_space(26.0);
            crate::links::home_links(app, ui);
        });
    });
    crate::home_image::credit(app, ui, area);
}
