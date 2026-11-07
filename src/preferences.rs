//! Preferences: General (how often to check, prereleases, notifications, the dock counter) and Connections (the access tokens,
//! GitHub's and GitLab's).

use crate::settings::{self, Interval, Settings};
use crate::token;
use adw::prelude::*;

/// Show the dialog over `window`; `saved` runs after a token or a setting was kept.
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
    let interval: adw::ComboRow = ui.object("interval_row").expect("row in preferences.blp");
    let prereleases: adw::SwitchRow = ui.object("prerelease_row").expect("row in preferences.blp");
    let notifications: adw::SwitchRow = ui
        .object("notification_row")
        .expect("row in preferences.blp");
    let dock: adw::SwitchRow = ui.object("dock_row").expect("row in preferences.blp");
    let labels: Vec<&str> = Interval::ALL.iter().map(|i| i.label()).collect();
    interval.set_model(Some(&gtk::StringList::new(&labels)));
    let now = settings::load();
    interval.set_selected(
        Interval::ALL
            .iter()
            .position(|i| *i == now.interval)
            .unwrap_or(1) as u32,
    );
    prereleases.set_active(now.prereleases);
    notifications.set_active(now.notifications);
    notifications.set_visible(now.interval.seconds().is_some());
    dock.set_active(now.dock_counter);
    // Every row writes the whole of the settings, from what they show.
    let keep = {
        let (interval, prereleases, notifications, dock, dialog, saved) = (
            interval.clone(),
            prereleases.clone(),
            notifications.clone(),
            dock.clone(),
            dialog.clone(),
            saved.clone(),
        );
        move || {
            let interval = Interval::ALL
                .get(interval.selected() as usize)
                .copied()
                .unwrap_or_default();
            let new = Settings {
                interval,
                prereleases: prereleases.is_active(),
                notifications: notifications.is_active(),
                dock_counter: dock.is_active(),
            };
            notifications.set_visible(interval.seconds().is_some());
            match settings::save(&new) {
                Ok(()) => saved(),
                Err(e) => dialog.add_toast(adw::Toast::new(&format!(
                    "Could not keep the settings: {e}"
                ))),
            }
        }
    };
    interval.connect_selected_notify({
        let keep = keep.clone();
        move |_| keep()
    });
    for row in [&prereleases, &notifications, &dock] {
        row.connect_active_notify({
            let keep = keep.clone();
            move |_| keep()
        });
    }
    dialog.present(Some(window));
}
