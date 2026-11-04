//! The main window: the installed extensions, the user's and the system's, with what the
//! user's have upstream.

use crate::cache::{self, Cache};
use crate::extensions::{self, Extension};
use crate::upstream::{self, Found, Outcome, Version};
use crate::{preferences, token};
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::rc::Rc;

/// A user extension's row, to check and show the result in.
struct Checked {
    uuid: String,
    version_name: Option<String>,
    counter: Option<u64>,
    url: Option<String>,
    status: gtk::Label,
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

    let button: gtk::Button = object("refresh_button").downcast().unwrap();
    button.set_sensitive(!checked.is_empty());
    let checker = Rc::new(Checker {
        rows: checked,
        button: button.clone(),
        banner: object("banner").downcast().unwrap(),
        token_banner: object("token_banner").downcast().unwrap(),
        run: Cell::new(0),
        cache: RefCell::new(Cache::load()),
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
            show(&r.status, "Checking…", "dim-label", None);
        }
        let (uuid, version_name, url) = (r.uuid.clone(), r.version_name.clone(), r.url.clone());
        let (counter, token) = (r.counter, token.clone());
        let (status, checker, pending) = (r.status.clone(), checker.clone(), pending.clone());
        glib::spawn_future_local(async move {
            let have_token = token.is_some();
            let (u, url2) = (uuid.clone(), url.clone());
            let (found, fresh) = gio::spawn_blocking(move || match cached {
                Some(found) => (Ok(found), false),
                None => (upstream::find(&u, url2.as_deref(), token.as_deref()), true),
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
            match upstream::outcome(found, version_name.as_deref(), counter) {
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
                Outcome::RateLimited => {
                    show(&status, "Rate limit reached", "warning", None);
                    checker.banner.set_title(if have_token {
                        "GitHub's rate limit is used up. Try again later."
                    } else {
                        "GitHub's rate limit is used up. An access token raises it."
                    });
                    checker.banner.set_revealed(true);
                }
                Outcome::Failed(e) => show(&status, "Check failed", "error", Some(&e)),
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
