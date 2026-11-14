// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Screen Printing Separations…: the **public placeholder** of the subscription module.
//!
//! The module (CMYK, indexed and white underbase separations for screen printers) is sold by
//! monthly or yearly subscription and its code is not public. This crate is the contract the app
//! builds against: the official build replaces this folder with the module's private crate, which
//! exposes the same items ([`OFFICIAL`], [`Host`], [`Separations`]). A build from the public code
//! keeps this placeholder: the menu item opens a window saying the module comes with the official
//! version.
//!
//! Keep this API in step with the private crate: any change here goes there too.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use serde_json::{Value, json};

/// `true` in the official build (the private module), `false` in this placeholder.
pub const OFFICIAL: bool = false;

/// The menu command that opens the module.
pub const COMMAND: &str = "dtf.separations";

/// Where the module is presented (the official site; its subscription page comes later).
pub const SITE_URL: &str = "https://www.dtfcraft.fr";

/// What the module asks of the app.
pub trait Host {
    /// Is a document open?
    fn has_document(&self) -> bool;
    /// Opens `url` in the user's browser.
    fn open_url(&mut self, url: &str);
}

/// The module's window state.
#[derive(Debug, Default)]
pub struct Separations {
    /// Set once the user closed the window.
    closed: bool,
}

impl Separations {
    pub fn new() -> Self {
        Self::default()
    }

    /// Draws the window. Returns `false` once the user closed it (the app then drops it).
    pub fn show(&mut self, ctx: &egui::Context, host: &mut dyn Host) -> bool {
        let mut open = !self.closed;
        let mut close = false;
        egui::Window::new("Séparations sérigraphie")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, egui::Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.label(egui::RichText::new("Module disponible dans la version officielle de DTF Craft").strong());
                ui.add_space(6.0);
                ui.label("Séparations quadri (CMJN), indexées et blanc de soutien pour la sérigraphie textile : films séparés, aperçu sur textile et fiche technique PDF.");
                ui.add_space(6.0);
                ui.label("Ce module est proposé sur abonnement, avec une période d'essai.");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.button("En savoir plus").clicked() {
                        host.open_url(SITE_URL);
                    }
                    if ui.button("Fermer").clicked() {
                        close = true;
                    }
                });
            });
        if close || !open {
            self.closed = true;
        }
        !self.closed
    }

    /// The window's state for the control channel.
    pub fn describe(&self) -> Value {
        json!({ "official": OFFICIAL, "open": !self.closed })
    }

    /// Control channel actions (`{"close": true}`); the placeholder has nothing else to drive.
    pub fn control(&mut self, params: &Value) -> Result<Value, String> {
        if params.get("close").and_then(Value::as_bool) == Some(true) {
            self.closed = true;
        }
        Ok(self.describe())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn placeholder_is_not_official() {
        assert_eq!(Separations::new().describe()["official"], json!(false));
        assert_eq!(COMMAND, "dtf.separations");
    }

    #[test]
    fn control_closes_and_ignores_other_params() {
        let mut s = Separations::new();
        assert_eq!(s.describe()["open"], json!(true));
        assert!(s.control(&json!({"unknown": 3})).is_ok());
        assert_eq!(s.describe()["open"], json!(true));
        assert!(s.control(&json!(null)).is_ok());
        assert!(s.control(&json!({"close": true})).is_ok());
        assert_eq!(s.describe()["open"], json!(false));
    }
}
