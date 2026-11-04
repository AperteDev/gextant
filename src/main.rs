//! Gextant: updates GNOME Shell extensions straight from their upstream repositories.

mod extensions;
mod window;

use adw::prelude::*;
use gtk::{gdk, gio, glib};

const APP_ID: &str = "dev.aperte.Gextant";

fn main() -> glib::ExitCode {
    gio::resources_register_include!("gextant.gresource").expect("resources");
    // The sandbox copy (GEXTANT_SANDBOX, see `make sandbox`) runs beside the normal app.
    let id = if std::env::var_os("GEXTANT_SANDBOX").is_some() {
        format!("{APP_ID}.Sandbox")
    } else {
        APP_ID.to_owned()
    };
    let app = adw::Application::builder().application_id(id).build();
    app.connect_startup(|app| {
        gtk::IconTheme::for_display(&gdk::Display::default().expect("display"))
            .add_resource_path("/dev/aperte/Gextant/icons");
        let quit = gio::ActionEntry::builder("quit")
            .activate(|app: &adw::Application, _, _| app.quit())
            .build();
        app.add_action_entries([quit]);
        app.set_accels_for_action("app.quit", &["<Control>q"]);
    });
    app.connect_activate(|app| window::build(app).present());
    app.run()
}
