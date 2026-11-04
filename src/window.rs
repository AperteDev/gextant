//! The main window: the installed extensions, the user's and the system's, with what the
//! user's have upstream.

use crate::extensions::{self, Extension};
use crate::upstream::{self, Outcome, Version};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::Cell;
use std::rc::Rc;

/// A user extension's row, to check and show the result in.
struct Checked {
    uuid: String,
    version_name: Option<String>,
    counter: Option<u64>,
    url: Option<String>,
    status: gtk::Label,
}

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
    let mut checked = Vec::new();
    for (system, group) in &groups {
        let mut rows = all.iter().filter(|e| e.system == *system).peekable();
        group.set_visible(rows.peek().is_some());
        for e in rows {
            let row = row(e);
            row.add_suffix(&version_label(e));
            if !system {
                let status = gtk::Label::builder()
                    .valign(gtk::Align::Center)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(24)
                    .build();
                row.add_suffix(&status);
                checked.push(Checked {
                    uuid: e.uuid.clone(),
                    version_name: e.version_name.clone(),
                    counter: e.counter,
                    url: e.url.clone(),
                    status,
                });
            }
            group.add(&row);
        }
    }
    let stack: gtk::Stack = object("stack").downcast().unwrap();
    stack.set_visible_child_name(if all.is_empty() { "empty" } else { "list" });

    let checked = Rc::new(checked);
    let button: gtk::Button = object("refresh_button").downcast().unwrap();
    button.set_sensitive(!checked.is_empty());
    button.connect_clicked({
        let checked = checked.clone();
        move |button| check_all(&checked, button)
    });
    check_all(&checked, &button);
    window
}

fn row(e: &Extension) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(&e.name)
        .subtitle(&e.uuid)
        .use_markup(false)
        .subtitle_selectable(true)
        .build()
}

fn version_label(e: &Extension) -> gtk::Label {
    let version = e.version().unwrap_or_default();
    gtk::Label::builder()
        .label(&version)
        .css_classes(["dim-label"])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(16)
        .tooltip_text(version)
        .valign(gtk::Align::Center)
        .build()
}

/// Check every user extension on its own thread; the button waits for the last.
fn check_all(rows: &Rc<Vec<Checked>>, button: &gtk::Button) {
    button.set_sensitive(false);
    let pending = Rc::new(Cell::new(rows.len()));
    for r in rows.iter() {
        show(&r.status, "Checking…", "dim-label", None);
        let (uuid, version_name, url) = (r.uuid.clone(), r.version_name.clone(), r.url.clone());
        let counter = r.counter;
        let (status, button, pending) = (r.status.clone(), button.clone(), pending.clone());
        glib::spawn_future_local(async move {
            let outcome = gio::spawn_blocking(move || {
                upstream::check(&uuid, version_name.as_deref(), counter, url.as_deref())
            })
            .await
            .unwrap_or_else(|_| Outcome::Failed("The check crashed".into()));
            match outcome {
                Outcome::Checked {
                    latest,
                    version: Version::Newer,
                } => show(
                    &status,
                    &format!("Update available: {latest}"),
                    "accent",
                    None,
                ),
                Outcome::Checked {
                    version: Version::Same,
                    ..
                } => show(&status, "Up to date", "success", None),
                Outcome::Checked {
                    latest,
                    version: Version::Unknown,
                } => show(
                    &status,
                    &format!("Latest: {latest}"),
                    "dim-label",
                    Some(&format!(
                        "Latest release: {latest}\nThe installed version cannot be compared with it"
                    )),
                ),
                Outcome::NoRepo => show(&status, "No repository found", "dim-label", None),
                Outcome::NoRelease => show(&status, "No releases", "dim-label", None),
                Outcome::Failed(e) => show(&status, "Check failed", "error", Some(&e)),
            }
            pending.set(pending.get() - 1);
            if pending.get() == 0 {
                button.set_sensitive(true);
            }
        });
    }
}

fn show(label: &gtk::Label, text: &str, class: &str, tooltip: Option<&str>) {
    label.set_label(text);
    label.set_css_classes(&[class]);
    label.set_tooltip_text(tooltip);
}
