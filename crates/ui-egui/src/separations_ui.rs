// DTF Craft — Copyright (c) 2026 Stéphane Glacet. Licensed under MIT OR Apache-2.0.
//! DTF › Screen Printing Separations…: opens the module of `photocraft-separations` and gives it
//! what it needs from the app ([`Host`]).
//!
//! The public build ships the module's placeholder (a window saying the module comes with the
//! official version); the official build replaces that crate with the subscription module, behind
//! the same API.
//!
//! Control channel: `ui.menu.invoke {"id":"dtf.separations"}` opens it and returns its state;
//! `{"params":{"ui":{…}}}` forwards the object to the module (`{"close": true}` closes it).

use photocraft_separations::{COMMAND, Host, Separations};
use serde_json::Value;

use crate::PhotocraftApp;

/// The app as the module sees it.
struct AppHost<'a> {
    app: &'a mut PhotocraftApp,
    ctx: &'a egui::Context,
}

impl Host for AppHost<'_> {
    fn has_document(&self) -> bool {
        self.app.session.active().is_some()
    }

    fn open_url(&mut self, url: &str) {
        crate::links::open(self.app, self.ctx, url);
    }
}

/// `dtf.separations` from a menu or the control channel.
pub fn menu(app: &mut PhotocraftApp, id: &str, params: &Value) -> Option<Result<Value, String>> {
    if id != COMMAND {
        return None;
    }
    let module = app.separations.get_or_insert_with(Separations::new);
    Some(match params.get("ui") {
        Some(ui) => module.control(ui),
        None => Ok(module.describe()),
    })
}

/// Draws the module while it is open; drops it once closed.
pub fn show(app: &mut PhotocraftApp, ctx: &egui::Context) {
    let Some(mut module) = app.separations.take() else {
        return;
    };
    let open = module.show(ctx, &mut AppHost { app, ctx });
    if open {
        app.separations = Some(module);
    }
}
