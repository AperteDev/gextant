//! Gextant: updates GNOME Shell extensions straight from their upstream repositories.

mod about;
mod cache;
mod check;
mod details;
mod ego;
mod extensions;
mod gitlab;
mod preferences;
mod token;
mod upstream;
mod window;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

const APP_ID: &str = "dev.aperte.Gextant";

/// The sandbox copy (`GEXTANT_SANDBOX`, see `make sandbox`) runs beside the normal app, with
/// an id and keyring entries of its own.
pub fn app_id() -> String {
    if std::env::var_os("GEXTANT_SANDBOX").is_some() {
        format!("{APP_ID}.Sandbox")
    } else {
        APP_ID.to_owned()
    }
}

fn main() -> glib::ExitCode {
    gio::resources_register_include!("gextant.gresource").expect("resources");
    let app = adw::Application::builder().application_id(app_id()).build();
    app.connect_startup(|app| {
        let display = gdk::Display::default().expect("display");
        gtk::IconTheme::for_display(&display).add_resource_path("/dev/aperte/Gextant/icons");
        // For a widget inside a coloured row (success, warning…) that should not take its colour.
        let css = gtk::CssProvider::new();
        css.load_from_string(".neutral { color: @window_fg_color; }");
        gtk::style_context_add_provider_for_display(
            &display,
            &css,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &adw::Application, _, _| app.quit())
            .build();
        let about = gio::ActionEntry::builder("about")
            .activate(|app: &adw::Application, _, _| {
                if let Some(window) = app.active_window() {
                    about::present(&window);
                }
            })
            .build();
        app.add_action_entries([quit, about]);
        app.set_accels_for_action("app.quit", &["<Control>q"]);
        app.set_accels_for_action("win.preferences", &["<Control>comma"]);
    });
    app.connect_activate(|app| window::build(app).present());
    app.run()
}
