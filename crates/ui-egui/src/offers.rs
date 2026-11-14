// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! Stéphane Glacet's two offers to DTF Craft's users, in French only (the interface in French):
//!
//! * **Training**: 1 h in a video call on the DTF tools, 75 € net of VAT, paid by PayPal once the
//!   appointment is confirmed;
//! * **Quote**: a specific development or a change to an existing tool, in régie at 375 € HT a day.
//!
//! Both are asked for by e-mail: a message ready to complete opens in the user's mail program
//! (`mailto:`), to [`CONTACT`], with DTF Craft's version, the system and the tool concerned.
//! Nothing is sent without the user. They are offered in the DTF and Help menus and in each DTF
//! tool's help sheet.

use egui::RichText;
use serde_json::{Value, json};

use crate::PhotocraftApp;
use crate::menus::MenuItem;
use crate::theme::Tokens;

/// Where the requests go.
pub const CONTACT: &str = "contact@stephane-glacet.com";
/// The training's price, net of VAT.
pub const TRAINING_PRICE: &str = "75 €";
/// The régie's rate, excluding VAT.
pub const DAY_RATE: &str = "375 € HT / jour";

/// The menu commands: DTF menu, then the same in the Help menu.
pub const TRAINING: &str = "dtf.training";
pub const QUOTE: &str = "dtf.quote";
pub const HELP_TRAINING: &str = "help.training";
pub const HELP_QUOTE: &str = "help.quote";
/// Their labels (English keys, translated in the French catalogue).
pub const TRAINING_LABEL: &str = "DTF Tools Training (1 h · €75)…";
pub const QUOTE_LABEL: &str = "Request a Development Quote…";


/// What the user asks for.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Request {
    #[default]
    Training,
    Quote,
}

/// The offers are shown with the interface in French: the training is given in French.
pub fn shown() -> bool {
    crate::i18n::current().code() == "fr"
}

/// The tool's name in French for a DTF command (its help sheet, the request).
pub fn tool_name(command: &str) -> Option<&'static str> {
    Some(match command {
        "dtf.conformity" => "Conformité",
        "filter.dtf.removeWhiteBackground" => "Détourage",
        "dtf.controlBackground" => "Fond de contrôle",
        "filter.dtf.removeSemiTransparent" => "Pixels transparents",
        "dtf.defringe" => "Supprimer franges",
        "dtf.sharpen" => "Netteté",
        "dtf.saturation" => "Saturation",
        "filter.dtf.halftone" => "Halftone DTF",
        "dtf.colorAnalyzer" => "Analyseur couleur",
        "dtf.gangSheet" => "Gang sheet",
        "dtf.vectorize" => "Vectorisation",
        _ => return None,
    })
}

fn system() -> &'static str {
    match std::env::consts::OS {
        "windows" => "Windows",
        "macos" => "macOS",
        "linux" => "Linux",
        other => other,
    }
}

/// `text` for a `mailto:` field: every byte outside the unreserved characters as `%XX`.
fn encode(text: &str) -> String {
    let mut out = String::with_capacity(text.len() * 2);
    for b in text.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{b:02X}"));
        }
    }
    out
}

/// The message's subject and body for `request` about `tool` (a DTF tool's French name).
pub fn message(request: Request, tool: Option<&str>) -> (String, String) {
    let footer = format!("--\nDTF Craft {} · {}", crate::brand::VERSION, system());
    match request {
        Request::Training => {
            let subject = match tool {
                Some(t) => format!("DTF Craft - Demande de formation (1 h) : {t}"),
                None => "DTF Craft - Demande de formation (1 h)".to_string(),
            };
            let body = format!(
                "Bonjour,\n\nJe souhaite réserver 1 h de formation en visio sur DTF Craft.\n\nOutil(s) à voir : {}\n\nCréneaux qui me conviennent (2 ou 3 propositions) :\n- \n- \n\nMa société : \nMon nom : \nMon téléphone : \n\n{footer}\nTarif : {TRAINING_PRICE} net de TVA, réglé par PayPal à la confirmation du rendez-vous.",
                tool.unwrap_or("")
            );
            (subject, body)
        }
        Request::Quote => {
            let (subject, need) = match tool {
                Some(t) => (format!("DTF Craft - Demande de devis : évolution de l'outil {t}"), format!("une évolution de l'outil {t}")),
                None => ("DTF Craft - Demande de devis : développement spécifique".to_string(), "un développement spécifique (nouvel outil)".to_string()),
            };
            let body = format!(
                "Bonjour,\n\nJe souhaite un devis pour {need}.\n\nMa société : \nMon nom : \nMon téléphone : \n\nDescription du besoin (ce que l'outil doit faire, sur quel visuel, pour quel résultat) :\n\n\nDélai souhaité : \n\nSi possible, joignez un visuel d'exemple.\n\n{footer}\nRégie : {DAY_RATE}. Le devis précise le nombre de jours estimé."
            );
            (subject, body)
        }
    }
}

/// The `mailto:` link of `request` about `tool`.
pub fn mailto(request: Request, tool: Option<&str>) -> String {
    let (subject, body) = message(request, tool);
    // Mail programs expect CRLF line breaks in a mailto body.
    format!("mailto:{CONTACT}?subject={}&body={}", encode(&subject), encode(&body.replace('\n', "\r\n")))
}

/// Opens the message of `request` in the user's mail program.
pub fn open(app: &mut PhotocraftApp, ctx: &egui::Context, request: Request, tool: Option<&str>) -> Value {
    let url = mailto(request, tool);
    let opened = app.services.open_url.as_ref().map(|f| f(&url).is_ok()).unwrap_or(false);
    if !opened {
        ctx.open_url(egui::OpenUrl::new_tab(&url));
    }
    app.ui.status = match request {
        Request::Training => "Demande de formation ouverte dans votre messagerie : complétez-la puis envoyez-la.".into(),
        Request::Quote => "Demande de devis ouverte dans votre messagerie : complétez-la puis envoyez-la.".into(),
    };
    json!({"url": url})
}

/// The menu commands: `Some` for one of the offers' ids.
pub fn menu(app: &mut PhotocraftApp, ctx: &egui::Context, id: &str) -> Option<Result<Value, String>> {
    let request = match id {
        TRAINING | HELP_TRAINING => Request::Training,
        QUOTE | HELP_QUOTE => Request::Quote,
        _ => return None,
    };
    Some(Ok(open(app, ctx, request, None)))
}

/// Whether `id` is one of the offers' commands.
pub fn handles(id: &str) -> bool {
    matches!(id, TRAINING | QUOTE | HELP_TRAINING | HELP_QUOTE)
}

fn item(id: &str, label: &str, top: &str) -> MenuItem {
    MenuItem { id: id.into(), label: label.into(), path: vec![top.into()], shortcut: None, enabled: id != "---", checked: None, color: None }
}

/// The offers in the menus, in French only: at the end of the DTF menu after a separator, and in
/// the Help menu before System Info.
pub fn insert_menu_items(items: &mut Vec<MenuItem>) {
    // DTF Craft has no Help › Check for Updates (PhotoCraft's releases): its versions are
    // announced on dtfcraft.fr and by e-mail.
    items.retain(|i| i.id != "help.checkForUpdates");
    if !shown() {
        return;
    }
    if let Some(last) = items.iter().rposition(|i| i.path.first().map(String::as_str) == Some("DTF")) {
        let add = [item("---", "---", "DTF"), item(TRAINING, TRAINING_LABEL, "DTF"), item(QUOTE, QUOTE_LABEL, "DTF")];
        for (k, it) in add.into_iter().enumerate() {
            items.insert(last + 1 + k, it);
        }
    }
    if let Some(at) = items.iter().position(|i| i.id == "help.systemInfo") {
        let add = [item(HELP_TRAINING, TRAINING_LABEL, "Help"), item(HELP_QUOTE, QUOTE_LABEL, "Help"), item("---", "---", "Help")];
        for (k, it) in add.into_iter().enumerate() {
            items.insert(at + k, it);
        }
    }
}

/// A request made where there is no app at hand (a help sheet): kept until [`poll`].
const PENDING: &str = "dtf-offer-request";

fn ask(ctx: &egui::Context, request: Request, tool: Option<&'static str>) {
    ctx.data_mut(|d| d.insert_temp(egui::Id::unique(PENDING), (request, tool)));
}

/// Opens the message a help sheet asked for.
pub fn poll(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let pending = ctx.data_mut(|d| d.remove_temp::<(Request, Option<&'static str>)>(egui::Id::unique(PENDING)));
    if let Some((request, tool)) = pending {
        open(app, ctx, request, tool);
    }
}

/// The two links at the bottom of a DTF tool's help sheet.
pub fn help_links(ui: &mut egui::Ui, command: &str) {
    if !shown() {
        return;
    }
    let Some(tool) = tool_name(command) else { return };
    let t = Tokens::get(ui.ctx());
    ui.add_space(2.0);
    crate::widgets::hairline(ui);
    ui.add_space(2.0);
    ui.label(RichText::new("Besoin d'aller plus loin ?").strong());
    let training = ui.link(format!("Demander 1 h de formation sur l'outil {tool} ({TRAINING_PRICE}, net de TVA)"));
    if training.clicked() {
        ask(ui.ctx(), Request::Training, Some(tool));
    }
    let quote = ui.link(format!("Demander une évolution de l'outil {tool} (devis, régie {DAY_RATE})"));
    if quote.clicked() {
        ask(ui.ctx(), Request::Quote, Some(tool));
    }
    ui.label(RichText::new("Un message à compléter s'ouvre dans votre messagerie.").small().color(t.text_faint));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fr() -> crate::i18n::Lang {
        crate::i18n::Lang::from_code("fr").expect("fr")
    }

    fn french() -> impl Drop {
        crate::i18n::language_scope(fr())
    }

    #[test]
    fn the_messages_name_the_tool_the_price_and_the_version() {
        let (subject, body) = message(Request::Training, Some("Gang sheet"));
        assert_eq!(subject, "DTF Craft - Demande de formation (1 h) : Gang sheet");
        assert!(body.contains("Outil(s) à voir : Gang sheet") && body.contains("75 € net de TVA") && body.contains("PayPal"), "{body}");
        assert!(body.contains(&format!("DTF Craft {}", crate::brand::VERSION)));
        let (subject, body) = message(Request::Quote, None);
        assert_eq!(subject, "DTF Craft - Demande de devis : développement spécifique");
        assert!(body.contains("375 € HT / jour"), "{body}");
        let (subject, _) = message(Request::Quote, Some("Vectorisation"));
        assert!(subject.ends_with("évolution de l'outil Vectorisation"));
    }

    #[test]
    fn the_mailto_link_is_encoded_and_short_enough_for_windows() {
        for (r, tool) in [(Request::Training, Some("Analyseur couleur")), (Request::Quote, Some("Analyseur couleur")), (Request::Quote, None)] {
            let url = mailto(r, tool);
            assert!(url.starts_with("mailto:contact@stephane-glacet.com?subject=DTF%20Craft"), "{url}");
            assert!(url.contains("%0D%0A") && !url.contains(' ') && !url.contains('\n'), "{url}");
            // Windows' ShellExecute takes about 2 000 characters.
            assert!(url.len() < 1900, "{} characters", url.len());
        }
        assert_eq!(encode("é & ?"), "%C3%A9%20%26%20%3F");
    }

    #[test]
    fn the_menus_offer_them_in_french_only() {
        let app = PhotocraftApp::new(photocraft_engine::Session::new(), crate::Services::default());
        let english = crate::menus::menu_items(&app);
        assert!(!english.iter().any(|i| handles(&i.id)));
        let _fr = french();
        let items = crate::menus::menu_items(&app);
        let dtf: Vec<&str> = items.iter().filter(|i| i.path.first().map(String::as_str) == Some("DTF")).map(|i| i.id.as_str()).collect();
        assert_eq!(dtf[dtf.len() - 3..], ["---", TRAINING, QUOTE], "{dtf:?}");
        let help: Vec<&str> = items.iter().filter(|i| i.path.first().map(String::as_str) == Some("Help")).map(|i| i.id.as_str()).collect();
        let at = help.iter().position(|i| *i == HELP_TRAINING).expect("in the Help menu");
        assert_eq!(help[at..at + 4], [HELP_TRAINING, HELP_QUOTE, "---", "help.systemInfo"], "{help:?}");
        // Translated.
        assert_eq!(crate::i18n::tr(fr(), TRAINING_LABEL), "Formation aux outils DTF (1 h · 75 €)…");
        assert_eq!(crate::i18n::tr(fr(), QUOTE_LABEL), "Demander un devis de développement…");
    }

    #[test]
    fn the_commands_open_the_message_in_the_mail_program() {
        let opened = std::rc::Rc::new(std::cell::RefCell::new(Vec::<String>::new()));
        let seen = opened.clone();
        let mut app = PhotocraftApp::new(
            photocraft_engine::Session::new(),
            crate::Services {
                open_url: Some(Box::new(move |u: &str| {
                    seen.borrow_mut().push(u.to_string());
                    Ok(())
                })),
                ..Default::default()
            },
        );
        let ctx = egui::Context::default();
        crate::menus::invoke(&mut app, &ctx, TRAINING, json!({})).unwrap();
        crate::menus::invoke(&mut app, &ctx, HELP_QUOTE, json!({})).unwrap();
        // A help sheet's link, opened on the next frame.
        ask(&ctx, Request::Training, Some("Netteté"));
        poll(&mut app, &ctx);
        let urls = opened.borrow();
        assert_eq!(urls.len(), 3);
        assert!(urls[0].contains("formation") && urls[1].contains("devis") && urls[2].contains("Nettet%C3%A9"), "{urls:?}");
        assert!(app.ui.status.contains("messagerie"));
    }
}
