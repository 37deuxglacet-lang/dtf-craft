// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF Craft's beta modules and bug reports.
//!
//! * A module in beta has "(beta)" after its menu entry and a BETA badge in its window. The first
//!   time it opens in a version, a window says it is still being developed and may have bugs;
//!   "Don't show again for this version" keeps it closed until the next version.
//! * Report a Bug (that window, the beta module's header, Help › Report a Bug…): a report ready to
//!   complete (what was done, what happened, what was expected, then DTF Craft's version, the
//!   system, the module, the document and the last message), editable in the window, to copy into
//!   an e-mail to [`crate::offers::CONTACT`] or to open in the user's mail program. Nothing is sent
//!   without the user; a mail program may be missing (webmail), hence Copy the Report.

use egui::{RichText, vec2};
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::theme::Tokens;
use crate::widgets;

/// The modules in beta: their command and French name (the report is read in French).
pub const MODULES: &[(&str, &str)] = &[(photocraft_engine::gang_sheet_cmds::GANG_SHEET, "Gang sheet")];

/// The badge's colour (DTF Craft's magenta).
const BADGE: egui::Color32 = egui::Color32::from_rgb(192, 58, 168);

/// Help › Report a Bug….
pub const REPORT: &str = "help.reportBug";
pub const REPORT_LABEL: &str = "Report a Bug…";
/// The modules whose beta window is not shown again, with the version it was hidden in.
const HIDDEN_KEY: &str = "dtfCraft.betaHidden";

/// Whether `command` opens a module in beta.
pub fn is_beta(command: &str) -> bool {
    MODULES.iter().any(|(c, _)| *c == command)
}

fn name_of(command: &str) -> Option<&'static str> {
    MODULES.iter().find(|(c, _)| *c == command).map(|(_, n)| *n)
}

/// The beta window and the bug report open now.
#[derive(Default)]
pub struct BetaState {
    /// The beta module whose window is shown, and its "don't show again".
    notice: Option<(&'static str, bool)>,
    /// The report being completed: its module (`None`: DTF Craft as a whole) and its text.
    report: Option<(Option<&'static str>, String)>,
}

fn hidden(app: &PhotocraftApp, command: &str) -> bool {
    app.session.prefs().dialogs.get(HIDDEN_KEY).and_then(|v| v.get(command)).and_then(Value::as_str) == Some(crate::brand::VERSION)
}

/// A beta module opened: its window, unless hidden for this version.
pub fn opened(app: &mut PhotocraftApp, command: &str) {
    let Some((c, _)) = MODULES.iter().find(|(c, _)| *c == command) else { return };
    if !hidden(app, command) {
        app.beta.notice = Some((c, false));
    }
}

/// Opens the bug report about `command`'s module (or DTF Craft, `None`).
pub fn report(app: &mut PhotocraftApp, command: Option<&str>) {
    let module = command.and_then(|c| MODULES.iter().find(|(m, _)| *m == c)).map(|(_, n)| *n);
    let text = report_text(app, module);
    app.beta.report = Some((module, text));
}

/// The report's subject.
pub fn subject(module: Option<&str>) -> String {
    match module {
        Some(m) => format!("DTF Craft {} - {m} (bêta) - Bug", crate::brand::VERSION),
        None => format!("DTF Craft {} - Bug", crate::brand::VERSION),
    }
}

/// The report to complete: the three questions, then what DTF Craft knows.
pub fn report_text(app: &PhotocraftApp, module: Option<&str>) -> String {
    let build = photocraft_engine::build_info::short_sha().map_or_else(String::new, |s| format!(" ({s})"));
    let doc = app.session.active().map_or_else(
        || "aucun".to_string(),
        |d| format!("{} × {} px, {} ppp", d.doc.size.width, d.doc.size.height, d.doc.resolution_dpi.round()),
    );
    let last = if app.ui.status.is_empty() { "aucun".to_string() } else { app.ui.status.to_string() };
    let about = match module {
        Some(m) => format!("dans le module {m} (bêta)"),
        None => "dans DTF Craft".to_string(),
    };
    format!(
        "Bonjour,\n\nJe signale un bug {about}.\n\nCe que j'ai fait :\n\n\nCe qui s'est passé :\n\n\nCe que j'attendais :\n\n\n(Joignez une capture d'écran si possible.)\n\n--\nDTF Craft {}{build} · {}\nModule : {}\nDocument : {doc}\nDernier message : {last}",
        crate::brand::VERSION,
        crate::offers::system(),
        module.unwrap_or("-"),
    )
}

/// The beta window, then the bug report: above every window.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let _dtf = crate::dtf_i18n::scope();
    notice(app, ctx);
    report_window(app, ctx);
}

fn notice(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some((command, mut hide)) = app.beta.notice else { return };
    let t = Tokens::get(ctx);
    let name = name_of(command).unwrap_or("");
    let mut answer: Option<bool> = None;
    egui::Modal::new(egui::Id::unique("dtf-beta-notice")).show(ctx, |ui| {
        ui.set_max_width(440.0);
        ui.horizontal(|ui| {
            badge(ui);
            ui.label(RichText::new(tl!("Module in Beta")).strong().size(15.0));
        });
        ui.add_space(8.0);
        ui.label(crate::i18n::fmt(tl!("{module} is still being developed. It may have bugs and change from one version to the next. Check your files before printing."), &[("module", name)]));
        ui.add_space(6.0);
        ui.label(RichText::new(tl!("Found a bug? Report it: every report helps finish the module.")).color(t.text_dim));
        ui.add_space(8.0);
        widgets::checkbox(ui, &mut hide, tl!("Don't show again for this version"));
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::primary_button(ui, tl!("Continue"), 100.0).clicked() {
                answer = Some(false);
            }
            if widgets::secondary_button(ui, tl!("Report a Bug"), 130.0).clicked() {
                answer = Some(true);
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.key_pressed(egui::Key::Enter)) && answer.is_none() {
        answer = Some(false);
    }
    app.beta.notice = Some((command, hide));
    if let Some(report_it) = answer {
        close_notice(app, command, hide);
        if report_it {
            report(app, Some(command));
        }
    }
}

/// Closes the beta window; `hide`: not again in this version.
fn close_notice(app: &mut PhotocraftApp, command: &str, hide: bool) {
    app.beta.notice = None;
    if hide {
        let mut all = app.session.prefs().dialogs.get(HIDDEN_KEY).cloned().filter(Value::is_object).unwrap_or_else(|| json!({}));
        all[command] = json!(crate::brand::VERSION);
        app.session.prefs.edit(|p| p.dialogs.insert(HIDDEN_KEY.into(), all));
    }
}

/// The BETA badge (the beta window, a beta module's header).
pub fn badge(ui: &mut egui::Ui) -> egui::Response {
    let text = tl!("BETA");
    let font = crate::theme::semibold(10.5);
    let g = ui.painter().layout_no_wrap(text.to_string(), font, egui::Color32::WHITE);
    let (r, resp) = ui.allocate_exact_size(g.size() + vec2(12.0, 6.0), egui::Sense::hover());
    ui.painter().rect_filled(r, 4.0, BADGE);
    ui.painter().galley(r.center() - g.size() / 2.0, g, egui::Color32::WHITE);
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Label, true, text));
    resp
}

fn report_window(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some((module, mut text)) = app.beta.report.take() else { return };
    let t = Tokens::get(ctx);
    let subject = subject(module);
    let mut close = false;
    let mut copy = false;
    let mut mail = false;
    egui::Modal::new(egui::Id::unique("dtf-bug-report")).show(ctx, |ui| {
        ui.set_width(520.0);
        ui.label(RichText::new(tl!("Report a Bug")).strong().size(15.0));
        ui.add_space(6.0);
        ui.label(crate::i18n::fmt(tl!("Complete the report, then copy it into an e-mail to {address} (with a screenshot if you can)."), &[("address", crate::offers::CONTACT)]));
        ui.add_space(6.0);
        ui.label(RichText::new(format!("{} : {subject}", tl!("Subject"))).color(t.text_dim));
        egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
            ui.add(egui::TextEdit::multiline(&mut text).desired_width(f32::INFINITY).desired_rows(14).font(egui::TextStyle::Monospace)).accessible_name(tl!("Report"));
        });
        ui.add_space(10.0);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::primary_button(ui, tl!("Copy the Report"), 150.0).clicked() {
                copy = true;
            }
            if widgets::secondary_button(ui, tl!("Open in Mail"), 130.0).on_hover_text(tl!("Opens the report in your mail program, if one is installed")).clicked() {
                mail = true;
            }
            if widgets::secondary_button(ui, tl!("Close"), 90.0).clicked() {
                close = true;
            }
        });
    });
    if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
        close = true;
    }
    if copy {
        ctx.copy_text(format!("{} {subject}\n\n{text}", tl!("Subject:")));
        app.ui.status = crate::i18n::fmt(tl!("Report copied: paste it into an e-mail to {address}."), &[("address", crate::offers::CONTACT)]).into();
    }
    if mail {
        let url = mailto(&subject, &text);
        let opened = app.services.open_url.as_ref().is_some_and(|f| f(&url).is_ok());
        if !opened {
            ctx.open_url(egui::OpenUrl::new_tab(&url));
        }
        app.ui.status = tl!("Report opened in your mail program: complete it, then send it.").into();
    }
    if !close {
        app.beta.report = Some((module, text));
    }
}

/// The report as a `mailto:` link.
pub fn mailto(subject: &str, body: &str) -> String {
    format!("mailto:{}?subject={}&body={}", crate::offers::CONTACT, crate::offers::encode(subject), crate::offers::encode(&body.replace('\n', "\r\n")))
}

/// Help › Report a Bug….
pub fn menu(app: &mut PhotocraftApp, id: &str) -> Option<Result<Value, String>> {
    (id == REPORT).then(|| {
        report(app, None);
        Ok(json!({"report": true}))
    })
}

/// Help › Report a Bug…, before System Info.
pub fn insert_menu_items(items: &mut Vec<crate::menus::MenuItem>) {
    if let Some(at) = items.iter().position(|i| i.id == "help.systemInfo") {
        let item = crate::menus::MenuItem { id: REPORT.into(), label: REPORT_LABEL.into(), path: vec!["Help".into()], shortcut: None, enabled: true, checked: None, color: None };
        items.insert(at, item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> PhotocraftApp {
        PhotocraftApp::new(photocraft_engine::Session::new(), Default::default())
    }

    /// The Gang Sheet opens with the beta window; hidden for this version, it doesn't come back.
    #[test]
    fn the_beta_window_shows_until_hidden_for_the_version() {
        let mut app = app();
        assert!(is_beta(photocraft_engine::gang_sheet_cmds::GANG_SHEET));
        crate::gang_sheet_ui::open(&mut app);
        assert_eq!(app.beta.notice.map(|n| n.0), Some(photocraft_engine::gang_sheet_cmds::GANG_SHEET));
        close_notice(&mut app, photocraft_engine::gang_sheet_cmds::GANG_SHEET, false);
        app.gang_sheet = None;
        crate::gang_sheet_ui::open(&mut app);
        assert!(app.beta.notice.is_some(), "not hidden: shown again");
        close_notice(&mut app, photocraft_engine::gang_sheet_cmds::GANG_SHEET, true);
        app.gang_sheet = None;
        crate::gang_sheet_ui::open(&mut app);
        assert!(app.beta.notice.is_none(), "hidden for this version");
        assert_eq!(app.session.prefs().dialogs[HIDDEN_KEY][photocraft_engine::gang_sheet_cmds::GANG_SHEET], crate::brand::VERSION);
    }

    /// The report names the module, the version and the document; its mailto link is encoded.
    #[test]
    fn the_report_names_the_module_version_and_document() {
        let mut app = app();
        app.run("file.new", json!({"width": 120, "height": 80, "resolution": 300})).unwrap();
        report(&mut app, Some(photocraft_engine::gang_sheet_cmds::GANG_SHEET));
        let (module, text) = app.beta.report.clone().unwrap();
        assert_eq!(module, Some("Gang sheet"));
        assert!(text.contains("module Gang sheet (bêta)") && text.contains("Ce que j'attendais"), "{text}");
        assert!(text.contains(&format!("DTF Craft {}", crate::brand::VERSION)) && text.contains("120 × 80 px, 300 ppp"), "{text}");
        assert_eq!(subject(module), format!("DTF Craft {} - Gang sheet (bêta) - Bug", crate::brand::VERSION));
        let url = mailto(&subject(module), &text);
        assert!(url.starts_with("mailto:contact@stephane-glacet.com?subject=DTF%20Craft") && !url.contains(' ') && url.contains("%0D%0A"), "{url}");
        // Help › Report a Bug…: about DTF Craft as a whole.
        assert!(menu(&mut app, REPORT).is_some());
        assert_eq!(app.beta.report.as_ref().unwrap().0, None);
        let mut items = vec![crate::menus::MenuItem { id: "help.systemInfo".into(), label: "System Info…".into(), path: vec!["Help".into()], shortcut: None, enabled: true, checked: None, color: None }];
        insert_menu_items(&mut items);
        assert_eq!(items[0].id, REPORT);
    }
}
