//! Preferences: the GitHub access token.

use crate::token;
use adw::prelude::*;

/// Show the dialog over `window`; `saved` runs after the token was kept.
pub fn present(window: &adw::ApplicationWindow, saved: impl Fn() + 'static) {
    let ui = gtk::Builder::from_resource("/dev/aperte/Gextant/preferences.ui");
    let dialog: adw::PreferencesDialog = ui.object("dialog").expect("dialog in preferences.blp");
    let row: adw::PasswordEntryRow = ui.object("token_row").expect("token_row");
    row.set_text(&token::load().unwrap_or_default());
    row.connect_apply({
        let dialog = dialog.clone();
        move |row| match token::store(row.text().trim()) {
            Ok(()) => saved(),
            Err(e) => dialog.add_toast(adw::Toast::new(&format!("Could not keep the token: {e}"))),
        }
    });
    dialog.present(Some(window));
}
