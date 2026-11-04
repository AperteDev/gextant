//! The main window: the installed extensions, the user's and the system's, with what the
//! user's have upstream.

use crate::cache::{self, Cache};
use crate::extensions::{self, Extension};
use crate::upstream::{self, Found, Outcome, Source, Version};
use crate::{preferences, token};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A user extension's row, to check and show the result in.
#[derive(Clone)]
struct Checked {
    uuid: String,
    version_name: Option<String>,
    counter: Option<u64>,
    url: Option<String>,
    /// Beside the version: where its code was found, and what it found, unless that is "up to
    /// date", which turns the version itself green.
    icon: gtk::Image,
    version: gtk::Label,
    status: gtk::Label,
}

impl Checked {
    fn show(&self, text: &str, class: &str, tooltip: Option<&str>, source: Option<Source>) {
        let up_to_date = class == "success";
        let version = self.version.text();
        // An extension without a version has nothing to turn green: say it in words.
        let (text, class) = if up_to_date && !version.is_empty() {
            ("", "dim-label")
        } else {
            (text, class)
        };
        self.status.set_visible(!text.is_empty());
        show(&self.status, text, class, tooltip);
        self.version
            .set_css_classes(&[if up_to_date { "success" } else { "dim-label" }]);
        self.version.set_tooltip_text(Some(&if up_to_date {
            format!("{version}\nUp to date")
        } else {
            version.to_string()
        }));
        self.icon.set_visible(source.is_some());
        if let Some(source) = source {
            self.icon.set_icon_name(Some(source.icon()));
            self.icon.set_tooltip_text(Some(source.name()));
        }
    }
}

/// What checking needs: the rows, the button that waits for the checks and the banner for the
/// rate limit.
struct Checker {
    rows: Vec<Checked>,
    button: gtk::Button,
    banner: adw::Banner,
    /// For a token GitHub refuses.
    token_banner: adw::Banner,
    /// Counts the runs: a result from an older one (the token changed meanwhile) is dropped.
    run: Cell<u32>,
    cache: RefCell<Cache>,
    /// The running Shell's major version, for extensions.gnome.org.
    shell: Option<String>,
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
            let version = version_label(e);
            if !system {
                let icon = gtk::Image::builder()
                    .pixel_size(16)
                    .css_classes(["dim-label"])
                    .valign(gtk::Align::Center)
                    .visible(false)
                    .build();
                let status = gtk::Label::builder()
                    .valign(gtk::Align::Center)
                    .ellipsize(gtk::pango::EllipsizeMode::End)
                    .max_width_chars(24)
                    .build();
                row.add_suffix(&icon);
                row.add_suffix(&version);
                row.add_suffix(&status);
                checked.push(Checked {
                    uuid: e.uuid.clone(),
                    version_name: e.version_name.clone(),
                    counter: e.counter,
                    url: e.url.clone(),
                    icon,
                    version,
                    status,
                });
            } else {
                row.add_suffix(&version);
            }
            group.add(&row);
        }
    }
    let stack: gtk::Stack = object("stack").downcast().unwrap();
    stack.set_visible_child_name(if all.is_empty() { "empty" } else { "list" });

    let button: gtk::Button = object("refresh_button").downcast().unwrap();
    button.set_sensitive(!checked.is_empty());
    let checker = Rc::new(Checker {
        rows: checked,
        button: button.clone(),
        banner: object("banner").downcast().unwrap(),
        token_banner: object("token_banner").downcast().unwrap(),
        run: Cell::new(0),
        cache: RefCell::new(Cache::load()),
        shell: extensions::shell_major(),
    });
    button.connect_clicked({
        let checker = checker.clone();
        move |_| check_all(&checker, true)
    });
    let preferences = gio::ActionEntry::builder("preferences")
        .activate({
            let checker = checker.clone();
            move |window: &adw::ApplicationWindow, _, _| {
                let checker = checker.clone();
                preferences::present(window, move || check_all(&checker, false));
            }
        })
        .build();
    window.add_action_entries([preferences]);
    check_all(&checker, false);
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

/// Check every user extension on its own thread; the button waits for the last. What was found
/// less than a few hours ago is not looked for again, unless `force` (the refresh button).
fn check_all(checker: &Rc<Checker>, force: bool) {
    if checker.rows.is_empty() {
        return;
    }
    let run = checker.run.get() + 1;
    checker.run.set(run);
    checker.button.set_sensitive(false);
    checker.banner.set_revealed(false);
    checker.token_banner.set_revealed(false);
    let now = cache::now();
    let cached: Vec<Option<Found>> = {
        let cache = checker.cache.borrow();
        let get = |r: &Checked| cache.get(&r.uuid, r.url.as_deref(), now);
        checker
            .rows
            .iter()
            .map(|r| if force { None } else { get(r) })
            .collect()
    };
    let token = token::load();
    // Only when something is about to be asked of GitHub: the token matters then.
    if let Some(token) = token.clone().filter(|_| cached.iter().any(Option::is_none)) {
        let checker = checker.clone();
        glib::spawn_future_local(async move {
            let refused = gio::spawn_blocking(move || upstream::token_refused(&token)).await;
            if matches!(refused, Ok(true)) && checker.run.get() == run {
                checker.token_banner.set_revealed(true);
            }
        });
    }
    let pending = Rc::new(Cell::new(checker.rows.len()));
    for (r, cached) in checker.rows.iter().zip(cached) {
        if cached.is_none() {
            r.show("Checking…", "dim-label", None, None);
        }
        let (uuid, version_name, url) = (r.uuid.clone(), r.version_name.clone(), r.url.clone());
        let (counter, token) = (r.counter, token.clone());
        let (row, checker, pending) = (r.clone(), checker.clone(), pending.clone());
        glib::spawn_future_local(async move {
            let have_token = token.is_some();
            let (u, url2, shell) = (uuid.clone(), url.clone(), checker.shell.clone());
            let (found, fresh) = gio::spawn_blocking(move || match cached {
                Some(found) => (Ok(found), false),
                None => (
                    upstream::find(&u, url2.as_deref(), token.as_deref(), shell.as_deref()),
                    true,
                ),
            })
            .await
            .unwrap_or_else(|_| {
                (
                    Err(upstream::Error::Other("The check crashed".into())),
                    false,
                )
            });
            if checker.run.get() != run {
                return;
            }
            if let (Ok(found), true) = (&found, fresh) {
                checker
                    .cache
                    .borrow_mut()
                    .put(&uuid, url.as_deref(), found.clone(), now);
            }
            let source = found.as_ref().ok().and_then(Found::source);
            match upstream::outcome(found, version_name.as_deref(), counter) {
                Outcome::Checked {
                    latest,
                    version: Version::Newer,
                } => row.show(
                    &format!("Update available: {latest}"),
                    "accent",
                    None,
                    source,
                ),
                Outcome::Checked {
                    version: Version::Same,
                    ..
                } => row.show("Up to date", "success", None, source),
                Outcome::Checked {
                    latest,
                    version: Version::Unknown,
                } => row.show(
                    &format!("Latest: {latest}"),
                    "dim-label",
                    Some(&format!(
                        "Latest release: {latest}\nThe installed version cannot be compared with it"
                    )),
                    source,
                ),
                Outcome::NoRepo => row.show("No repository found", "dim-label", None, source),
                Outcome::NoRelease => row.show("No releases", "dim-label", None, source),
                Outcome::RateLimited => {
                    row.show("Rate limit reached", "warning", None, None);
                    checker.banner.set_title(if have_token {
                        "GitHub's rate limit is used up. Try again later."
                    } else {
                        "GitHub's rate limit is used up. An access token raises it."
                    });
                    checker.banner.set_revealed(true);
                }
                Outcome::Failed(e) => row.show("Check failed", "error", Some(&e), None),
            }
            pending.set(pending.get() - 1);
            if pending.get() == 0 {
                checker.button.set_sensitive(true);
                let mut cache = checker.cache.borrow_mut();
                let uuids: Vec<&str> = checker.rows.iter().map(|r| r.uuid.as_str()).collect();
                cache.retain(&uuids);
                cache.save();
            }
        });
    }
}

fn show(label: &gtk::Label, text: &str, class: &str, tooltip: Option<&str>) {
    label.set_label(text);
    label.set_css_classes(&[class]);
    label.set_tooltip_text(tooltip);
}
