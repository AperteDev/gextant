//! The main window: the installed extensions, the user's and the system's.

use crate::extensions::{self, Extension};
use adw::prelude::*;
use gtk::glib;

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let ui = gtk::Builder::from_resource("/dev/aperte/Gextant/window.ui");
    let object = |id: &str| ui.object::<glib::Object>(id).expect("object in window.blp");
    let window: adw::ApplicationWindow = object("window").downcast().unwrap();
    window.set_application(Some(app));

    let groups = [
        (
            false,
            object("user_group")
                .downcast::<adw::PreferencesGroup>()
                .unwrap(),
        ),
        (
            true,
            object("system_group")
                .downcast::<adw::PreferencesGroup>()
                .unwrap(),
        ),
    ];
    let all = extensions::installed();
    for (system, group) in &groups {
        let mut rows = all.iter().filter(|e| e.system == *system).peekable();
        group.set_visible(rows.peek().is_some());
        rows.for_each(|e| group.add(&row(e)));
    }
    let stack: gtk::Stack = object("stack").downcast().unwrap();
    stack.set_visible_child_name(if all.is_empty() { "empty" } else { "list" });
    window
}

fn row(e: &Extension) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(&e.name)
        .subtitle(&e.uuid)
        .use_markup(false)
        .subtitle_selectable(true)
        .build();
    if let Some(version) = &e.version {
        row.add_suffix(
            &gtk::Label::builder()
                .label(version)
                .css_classes(["dim-label"])
                .ellipsize(gtk::pango::EllipsizeMode::End)
                .max_width_chars(16)
                .tooltip_text(version)
                .valign(gtk::Align::Center)
                .build(),
        );
    }
    row
}
