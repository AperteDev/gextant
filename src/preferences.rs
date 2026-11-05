//! Preferences: the access tokens, GitHub's and GitLab's.

use crate::token;
use adw::prelude::*;

/// Show the dialog over `window`; `saved` runs after a token was kept.
pub fn present(window: &adw::ApplicationWindow, saved: impl Fn() + Clone + 'static) {
    let ui = gtk::Builder::from_resource("/dev/aperte/Gextant/preferences.ui");
    let dialog: adw::PreferencesDialog = ui.object("dialog").expect("dialog in preferences.blp");
    for (service, id) in [("github", "token_row"), ("gitlab", "gitlab_token_row")] {
        let row: adw::PasswordEntryRow = ui.object(id).expect("token row in preferences.blp");
        row.set_text(&token::load(service).unwrap_or_default());
        row.connect_apply({
            let (dialog, saved) = (dialog.clone(), saved.clone());
            move |row| match token::store(service, row.text().trim()) {
                Ok(()) => saved(),
                Err(e) => {
                    dialog.add_toast(adw::Toast::new(&format!("Could not keep the token: {e}")))
                }
            }
        });
    }
    dialog.present(Some(window));
}
