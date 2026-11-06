//! The details of one extension, from its `metadata.json`, the running Shell and the checks.

use crate::check::View;
use crate::extensions::{self, Extension, ShellInfo};
use crate::install;
use crate::upstream::Version;
use adw::prelude::*;
use gtk::{gio, glib};
use serde_json::Value;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

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

/// Say what an action did or why it failed.
fn toast(overlay: &adw::ToastOverlay, ok: bool, done: &str, failed: &str) {
    overlay.add_toast(adw::Toast::new(if ok { done } else { failed }));
}

/// A flat icon button for a row's end.
fn icon_button(icon: &str, tip: &str, clicked: impl Fn(&gtk::Button) + 'static) -> gtk::Button {
    let button = gtk::Button::builder()
        .icon_name(icon)
        .tooltip_text(tip)
        .valign(gtk::Align::Center)
        .css_classes(["flat"])
        .build();
    button.connect_clicked(clicked);
    button
}

/// The button that installs `plan`'s version over `e`, with the mark of its source; it asks no
/// more, but says what happened.
fn update_button(
    toasts: &adw::ToastOverlay,
    e: &Extension,
    shell: Option<&str>,
    plan: install::Plan,
    all: &Rc<RefCell<Vec<gtk::Button>>>,
    on_updated: &Rc<dyn Fn()>,
) -> gtk::Button {
    let content = adw::ButtonContent::builder()
        .icon_name(plan.source.icon())
        .label("Update")
        .build();
    let button = gtk::Button::builder()
        .child(&content)
        .tooltip_text(format!("Update from {}", plan.source.name()))
        .valign(gtk::Align::Center)
        .css_classes(["suggested-action"])
        .build();
    let (toasts, e, shell) = (toasts.clone(), e.clone(), shell.map(str::to_owned));
    let (all, on_updated, plan) = (all.clone(), on_updated.clone(), plan);
    button.connect_clicked(move |button| {
        let (button, content, toasts, all) =
            (button.clone(), content.clone(), toasts.clone(), all.clone());
        let (e, shell, plan, on_updated) =
            (e.clone(), shell.clone(), plan.clone(), on_updated.clone());
        glib::spawn_future_local(async move {
            // A link (a package linked into the user's folder) is replaced by a copy of its
            // own: after that, the package's updates no longer reach it.
            if let Ok(target) = std::fs::read_link(&e.path) {
                let dialog = adw::AlertDialog::new(
                    Some("Replace the link?"),
                    Some(&format!(
                        "{} is a link to {}, probably a package. Updating replaces the link with a copy of its own: the package's updates will no longer reach it until that copy is removed. The link is kept in the backups.",
                        e.name,
                        target.display()
                    )),
                );
                dialog.add_responses(&[("cancel", "Cancel"), ("replace", "Replace")]);
                dialog.set_response_appearance("replace", adw::ResponseAppearance::Destructive);
                dialog.set_default_response(Some("cancel"));
                dialog.set_close_response("cancel");
                if dialog.choose_future(Some(&button)).await != "replace" {
                    return;
                }
            }
            for b in all.borrow().iter() {
                b.set_sensitive(false);
            }
            content.set_label("Updating…");
            let (e2, shell2, plan2) = (e.clone(), shell.clone(), plan.clone());
            let result =
                gio::spawn_blocking(move || install::update(&e2, &plan2, shell2.as_deref()))
                    .await
                    .unwrap_or_else(|_| Err("The update crashed".to_owned()));
            match result {
                Ok(version) => {
                    content.set_label("Updated");
                    content.set_icon_name("object-select-symbolic");
                    let text = format!("Updated to {version}. Log out to apply it.");
                    toasts.add_toast(adw::Toast::new(&text));
                    on_updated();
                }
                Err(why) => {
                    content.set_label("Update");
                    for b in all.borrow().iter() {
                        b.set_sensitive(true);
                    }
                    toasts.add_toast(adw::Toast::new(&format!("Could not update: {why}")));
                }
            }
        });
    });
    button
}

/// `text` as Pango markup, for the part of Markdown descriptions use: `*`/`-` bullets, `#`
/// headings, `**bold**`, `*italic*`, `_italic_`, `` `code` `` and links, written out or bare.
fn markup(text: &str) -> String {
    let mut lines: Vec<String> = Vec::new();
    for line in text.lines() {
        let line = line.trim();
        let line = if let Some(item) = line.strip_prefix("* ").or_else(|| line.strip_prefix("- ")) {
            format!("• {}", inline(item))
        } else if line.starts_with('#') {
            format!(
                "<b>{}</b>",
                inline(line.trim_start_matches('#').trim_start())
            )
        } else {
            inline(line)
        };
        lines.push(line);
    }
    lines.join("\n")
}

fn esc(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// `s` starts with `d` and has `d` again after some text that does not begin or end with a
/// space: that text, and what follows.
fn delimited<'a>(s: &'a str, d: &str) -> Option<(&'a str, &'a str)> {
    let body = s.strip_prefix(d)?;
    let end = body.find(d)?;
    let inner = &body[..end];
    let spaced = inner.starts_with(char::is_whitespace) || inner.ends_with(char::is_whitespace);
    (!inner.is_empty() && !spaced).then(|| (inner, &body[end + d.len()..]))
}

fn link(url: &str, text: &str) -> String {
    format!("<a href=\"{}\">{text}</a>", esc(url))
}

/// The markup for what starts `rest`, and what is left of it, if it starts a span or a link.
/// `_` and `*` open italics only at the start of a word (not in `snake_case`).
fn construct(rest: &str, word_start: bool) -> Option<(String, &str)> {
    if let Some((i, after)) = delimited(rest, "**") {
        return Some((format!("<b>{}</b>", inline(i)), after));
    }
    if let Some((i, after)) = delimited(rest, "`") {
        return Some((format!("<tt>{}</tt>", esc(i)), after));
    }
    if word_start {
        for d in ["*", "_"] {
            if let Some((i, after)) = delimited(rest, d) {
                return Some((format!("<i>{}</i>", inline(i)), after));
            }
        }
    }
    if let Some(r) = rest.strip_prefix('[') {
        let (text, r) = r.split_once("](")?;
        let (url, after) = r.split_once(')')?;
        return url
            .starts_with("http")
            .then(|| (link(url, &inline(text)), after));
    }
    if rest.starts_with("http://") || rest.starts_with("https://") {
        let end = rest.find(char::is_whitespace).unwrap_or(rest.len());
        let url = rest[..end].trim_end_matches(['.', ',', ';', ':', ')', '!', '?', '\'', '"']);
        return Some((link(url, &esc(url)), &rest[url.len()..]));
    }
    None
}

fn inline(s: &str) -> String {
    let (mut out, mut rest, mut prev) = (String::new(), s, ' ');
    while let Some(c) = rest.chars().next() {
        if let Some((html, after)) = construct(rest, !prev.is_alphanumeric()) {
            out.push_str(&html);
            (rest, prev) = (after, ' ');
        } else {
            out.push_str(&esc(c.encode_utf8(&mut [0; 4])));
            (rest, prev) = (&rest[c.len_utf8()..], c);
        }
    }
    out
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
    on_updated: Rc<dyn Fn()>,
) {
    let ui = gtk::Builder::from_resource("/dev/aperte/Gextant/details.ui");
    let object = |id: &str| {
        ui.object::<adw::PreferencesGroup>(id)
            .expect("group in details.blp")
    };
    let dialog: adw::Dialog = ui.object("dialog").expect("dialog in details.blp");
    dialog.set_title(&e.name);
    let toasts: adw::ToastOverlay = ui.object("toasts").expect("toasts in details.blp");

    let info = object("info_group");
    if let Some(d) = e.metadata.get("description") {
        let text = value_text(d);
        let (full, short) = (markup(&text), markup(&text).replace('\n', " "));
        if text.contains('\n') || text.chars().count() > 60 {
            // One line, as the other rows (each line of a label is cut on its own, so it is
            // one paragraph), until expanded to the whole text.
            let row = adw::ExpanderRow::builder()
                .title("Description")
                .subtitle(&short)
                .subtitle_lines(1)
                .use_markup(true)
                .build();
            row.add_css_class("property");
            row.add_row(
                &gtk::Label::builder()
                    .label(&full)
                    .use_markup(true)
                    .wrap(true)
                    .selectable(true)
                    .xalign(0.0)
                    .margin_top(12)
                    .margin_bottom(12)
                    .margin_start(12)
                    .margin_end(12)
                    .build(),
            );
            // The whole text is shown below; the line above would only repeat it.
            row.connect_expanded_notify(move |row| {
                row.set_subtitle(if row.is_expanded() { "" } else { &short });
            });
            info.add(&row);
        } else {
            let row = property(&info, "Description", "");
            row.set_use_markup(true);
            row.set_subtitle(&full);
        }
    }
    let row = property(&info, "UUID", &e.uuid);
    row.add_suffix(&icon_button("edit-copy-symbolic", "Copy UUID", {
        let (uuid, toasts) = (e.uuid.clone(), toasts.clone());
        move |button| {
            button.clipboard().set_text(&uuid);
            toast(&toasts, true, "UUID copied", "");
        }
    }));
    if let Some(v) = &e.version_name {
        property(&info, "Version", v).add_prefix(&gtk::Image::from_icon_name("computer-symbolic"));
    }
    if let Some(c) = e.counter {
        property(&info, "Version Number", &c.to_string())
            .add_prefix(&gtk::Image::from_icon_name("gextant-gnome-symbolic"));
    }
    let (who, icon) = if e.system {
        ("For all users", "system-users-symbolic")
    } else {
        ("For you only", "avatar-default-symbolic")
    };
    property(&info, "Installed", who).add_prefix(&gtk::Image::from_icon_name(icon));
    let row = property(&info, "Location", &e.path.display().to_string());
    row.add_suffix(&icon_button("folder-open-symbolic", "Open Folder", {
        let (window, folder, toasts) =
            (window.clone(), gio::File::for_path(&e.path), toasts.clone());
        move |_| {
            let toasts = toasts.clone();
            gtk::FileLauncher::new(Some(&folder)).launch(
                Some(&window),
                None::<&gio::Cancellable>,
                move |r| {
                    toast(
                        &toasts,
                        r.is_ok(),
                        "Folder opened",
                        "Could not open the folder",
                    )
                },
            )
        }
    }));

    let shell_group = object("shell_group");
    if let Some(note) = e.shadow_note() {
        let row = property(&shell_group, "Shadowed", &note);
        row.add_prefix(
            &gtk::Image::builder()
                .icon_name("dialog-warning-symbolic")
                .css_classes(["warning"])
                .build(),
        );
        row.add_css_class("warning");
    }
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
    // All of them, to be switched off together while one is working.
    let update_buttons: Rc<RefCell<Vec<gtk::Button>>> = Rc::default();
    let mut has_source = false;
    if let Some(url) = &e.url {
        has_source = true;
        let row = property(&source, "Website", url);
        if url.starts_with("https://") || url.starts_with("http://") {
            let url = url.clone();
            row.add_suffix(&gtk::Image::from_icon_name("adw-external-link-symbolic"));
            row.set_activatable(true);
            let (window, toasts) = (window.clone(), toasts.clone());
            row.connect_activated(move |_| {
                let toasts = toasts.clone();
                gtk::UriLauncher::new(&url).launch(
                    Some(&window),
                    None::<&gio::Cancellable>,
                    move |r| toast(&toasts, r.is_ok(), "Link opened", "Could not open the link"),
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
        // The version first; what is behind it, dimmed (the subtitle) below.
        let mut lines: Vec<String> = Vec::new();
        if !chip.missing {
            lines.push(
                match chip.state {
                    Version::Same => "Up to date",
                    Version::Newer => "Update available",
                    Version::Unknown => "Cannot be compared with the installed version",
                }
                .to_owned(),
            );
        }
        lines.extend(
            chip.sources
                .iter()
                .map(|(s, text)| format!("{}: {text}", s.name())),
        );
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(lines.join("\n"))
            .use_markup(false)
            .subtitle_selectable(true)
            .build();
        let icons = gtk::Box::builder().spacing(4).build();
        for (s, _) in &chip.sources {
            icons.append(
                &gtk::Image::builder()
                    .icon_name(s.icon())
                    .css_classes([chip.class()])
                    .build(),
            );
        }
        row.add_prefix(&icons);
        row.add_css_class(chip.class());
        // Where the version is newer than what is installed, a button for each source that has
        // it (a user's extension only: a system one is not ours to change).
        if !e.system && !chip.missing && chip.state == Version::Newer {
            for (s, tag) in &chip.sources {
                let plan = install::Plan {
                    source: *s,
                    repo: view.and_then(|v| v.repo.clone()),
                    tag: tag.clone(),
                };
                let button = update_button(&toasts, e, shell, plan, &update_buttons, &on_updated);
                row.add_suffix(&button);
                update_buttons.borrow_mut().push(button);
            }
        }
        source.add(&row);
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
    let (ext, ext2) = (e.clone(), e.clone());
    let compatible = shell.is_some_and(|s| e.supports(s));
    let toasts = toasts.clone();
    glib::spawn_future_local(async move {
        if let Ok(Some(info)) = gio::spawn_blocking(move || extensions::shell_info(&ext)).await {
            shell_rows(&shell_group, &toasts, &ext2, compatible, &info);
        }
    });
    dialog.present(Some(window));
}

/// Show the Shell's state on the status row: its text, icon and colour.
fn set_status(row: &adw::ActionRow, icon: &gtk::Image, info: &ShellInfo) {
    let (state, name, class) = match info.state {
        1 => ("Enabled", "object-select-symbolic", "success"),
        2 => ("Disabled", "action-unavailable-symbolic", "dim-label"),
        3 => ("Error", "dialog-error-symbolic", "error"),
        4 => ("Out of date", "dialog-warning-symbolic", "warning"),
        _ => ("Not loaded", "dialog-question-symbolic", "dim-label"),
    };
    for old in ["success", "dim-label", "error", "warning"] {
        row.remove_css_class(old);
        icon.remove_css_class(old);
    }
    row.set_subtitle(state);
    icon.set_icon_name(Some(name));
    // `dim-label` fades what it is on, the button included: the icon only.
    if class != "dim-label" {
        row.add_css_class(class);
    }
    icon.add_css_class(class);
}

/// The button that switches the extension: Enable in the accent colour, Disable plain (and not
/// taking the colour of the row it is in).
fn set_toggle(button: &gtk::Button, enabled: bool) {
    button.set_label(if enabled { "Disable" } else { "Enable" });
    button.set_css_classes(&[if enabled {
        "neutral"
    } else {
        "suggested-action"
    }]);
}

/// The Shell's state of the extension, with a button to switch it on or off where it supports
/// the running Shell (and the Shell lets it be changed).
fn shell_rows(
    group: &adw::PreferencesGroup,
    toasts: &adw::ToastOverlay,
    ext: &Extension,
    compatible: bool,
    info: &ShellInfo,
) {
    let uuid = ext.uuid.as_str();
    let row = property(group, "Status", "");
    let icon = gtk::Image::new();
    row.add_prefix(&icon);
    set_status(&row, &icon, info);
    if compatible && info.can_change {
        let button = gtk::Button::builder().valign(gtk::Align::Center).build();
        set_toggle(&button, info.enabled);
        let enabled = Rc::new(Cell::new(info.enabled));
        let (ext, toasts, status, icon) = (ext.clone(), toasts.clone(), row.clone(), icon.clone());
        button.connect_clicked(move |button| {
            let enable = !enabled.get();
            button.set_sensitive(false);
            let (button, enabled, ext, toasts) =
                (button.clone(), enabled.clone(), ext.clone(), toasts.clone());
            let (status, icon) = (status.clone(), icon.clone());
            glib::spawn_future_local(async move {
                let e = ext.clone();
                let (ok, now) = gio::spawn_blocking(move || {
                    (
                        extensions::set_enabled(&e.uuid, enable),
                        extensions::shell_info(&e),
                    )
                })
                .await
                .unwrap_or((false, None));
                toast(
                    &toasts,
                    ok,
                    if enable {
                        "Extension enabled"
                    } else {
                        "Extension disabled"
                    },
                    if enable {
                        "Could not enable the extension"
                    } else {
                        "Could not disable the extension"
                    },
                );
                if let Some(now) = now {
                    set_status(&status, &icon, &now);
                    enabled.set(now.enabled);
                    set_toggle(&button, now.enabled);
                }
                button.set_sensitive(true);
            });
        });
        row.add_suffix(&button);
    }
    if !info.error.is_empty() {
        let row = property(group, "Error", &info.error);
        row.add_prefix(
            &gtk::Image::builder()
                .icon_name("dialog-error-symbolic")
                .css_classes(["error"])
                .build(),
        );
        row.add_css_class("error");
    }
    let row = property(
        group,
        "Settings",
        if info.has_prefs { "Yes" } else { "None" },
    );
    if info.has_prefs {
        let (uuid, toasts) = (uuid.to_owned(), toasts.clone());
        row.add_suffix(&icon_button(
            "emblem-system-symbolic",
            "Open Settings",
            move |_| {
                let (uuid, toasts) = (uuid.clone(), toasts.clone());
                glib::spawn_future_local(async move {
                    let ok = gio::spawn_blocking(move || extensions::open_prefs(&uuid))
                        .await
                        .unwrap_or(false);
                    toast(
                        &toasts,
                        ok,
                        "Settings opened",
                        "Could not open the settings",
                    );
                });
            },
        ));
    }
}

#[cfg(test)]
mod tests {
    use super::markup;

    #[test]
    fn markdown_to_pango() {
        assert_eq!(
            markup("# Hi\n* **a** & `b<c>`\n- _x_ snake_case_name"),
            "<b>Hi</b>\n• <b>a</b> &amp; <tt>b&lt;c&gt;</tt>\n• <i>x</i> snake_case_name"
        );
        assert_eq!(
            markup("See [docs](https://a.b/c) or (https://d.e/f), 2 * 3 * 4"),
            "See <a href=\"https://a.b/c\">docs</a> or (<a href=\"https://d.e/f\">https://d.e/f</a>), 2 * 3 * 4"
        );
    }
}
