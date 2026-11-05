//! The details of one extension, from its `metadata.json`, the running Shell and the checks.

use crate::check::View;
use crate::extensions::{self, Extension, ShellInfo};
use adw::prelude::*;
use gtk::{gio, glib};
use serde_json::Value;

/// The keys shown by name; every other one goes under "Other Metadata".
const KNOWN: [&str; 7] = [
    "uuid",
    "name",
    "description",
    "version",
    "version-name",
    "url",
    "shell-version",
];

/// A key and its value, selectable.
fn property(group: &adw::PreferencesGroup, title: &str, value: &str) -> adw::ActionRow {
    let row = adw::ActionRow::builder()
        .title(title)
        .subtitle(value)
        .use_markup(false)
        .subtitle_selectable(true)
        .css_classes(["property"])
        .build();
    group.add(&row);
    row
}

fn value_text(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Array(a) if a.iter().all(Value::is_string) => a
            .iter()
            .filter_map(Value::as_str)
            .collect::<Vec<_>>()
            .join(", "),
        v => v.to_string(),
    }
}

/// Show the details of `e` over `window`; `shell` is the running Shell's major version and `view`
/// what the checks found (none for a system extension).
pub fn present(
    window: &adw::ApplicationWindow,
    e: &Extension,
    shell: Option<&str>,
    view: Option<&View>,
) {
    let ui = gtk::Builder::from_resource("/dev/aperte/Gextant/details.ui");
    let object = |id: &str| {
        ui.object::<adw::PreferencesGroup>(id)
            .expect("group in details.blp")
    };
    let dialog: adw::Dialog = ui.object("dialog").expect("dialog in details.blp");
    dialog.set_title(&e.name);

    let info = object("info_group");
    if let Some(d) = e.metadata.get("description") {
        property(&info, "Description", &value_text(d));
    }
    property(&info, "UUID", &e.uuid);
    let version = match (&e.version_name, e.counter) {
        (Some(name), Some(c)) => format!("{name} (version {c})"),
        (Some(v), None) => v.clone(),
        (None, Some(c)) => c.to_string(),
        (None, None) => "None".to_owned(),
    };
    property(&info, "Version", &version);
    property(
        &info,
        "Installed",
        if e.system {
            "For all users (system)"
        } else {
            "By you"
        },
    );
    property(&info, "Location", &e.path.display().to_string());

    let shell_group = object("shell_group");
    let supported = if e.shell_versions.is_empty() {
        "Not stated".to_owned()
    } else {
        e.shell_versions.join(", ")
    };
    let row = property(&shell_group, "Supported versions", &supported);
    if let Some(shell) = shell {
        let ok = e.supports(shell);
        let (icon, class, text) = if ok {
            (
                "object-select-symbolic",
                "success",
                format!("Supports {shell}"),
            )
        } else {
            (
                "dialog-warning-symbolic",
                "warning",
                format!("Does not support GNOME Shell {shell}"),
            )
        };
        row.add_suffix(
            &gtk::Image::builder()
                .icon_name(icon)
                .css_classes([class])
                .tooltip_text(text)
                .build(),
        );
    }

    let source = object("source_group");
    let mut has_source = false;
    if let Some(url) = &e.url {
        has_source = true;
        let row = property(&source, "Website", url);
        if url.starts_with("https://") || url.starts_with("http://") {
            let url = url.clone();
            row.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
            row.set_activatable(true);
            let window = window.clone();
            row.connect_activated(move |_| {
                gtk::UriLauncher::new(&url).launch(
                    Some(&window),
                    None::<&gio::Cancellable>,
                    |_| {},
                );
            });
        }
    }
    for chip in view.iter().flat_map(|v| &v.chips) {
        has_source = true;
        let title = if chip.missing {
            "No version"
        } else {
            &chip.version
        };
        let row = property(&source, title, &chip.tip);
        let icons = gtk::Box::builder().spacing(4).build();
        for (s, _) in &chip.sources {
            icons.append(&gtk::Image::from_icon_name(s.icon()));
        }
        row.add_prefix(&icons);
    }
    source.set_visible(has_source);

    let other = object("metadata_group");
    let mut any = false;
    for (key, value) in &e.metadata {
        if !KNOWN.contains(&key.as_str()) {
            any = true;
            property(&other, key, &value_text(value));
        }
    }
    other.set_visible(any);

    // The Shell is asked off the main thread and answers after the dialog is up.
    let ext = e.clone();
    glib::spawn_future_local(async move {
        if let Ok(Some(info)) = gio::spawn_blocking(move || extensions::shell_info(&ext)).await {
            shell_rows(&shell_group, &info);
        }
    });
    dialog.present(Some(window));
}

fn shell_rows(group: &adw::PreferencesGroup, info: &ShellInfo) {
    let state = match info.state {
        1 => "Enabled",
        2 => "Disabled",
        3 => "Error",
        4 => "Out of date",
        _ => "Not loaded",
    };
    property(group, "Status", state);
    if !info.error.is_empty() {
        property(group, "Error", &info.error);
    }
    property(
        group,
        "Settings",
        if info.has_prefs { "Yes" } else { "None" },
    );
}
