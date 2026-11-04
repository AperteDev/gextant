//! The main window: the installed extensions, the user's and the system's, with what the
//! user's have upstream.

use crate::cache::{self, Cache};
use crate::check::{self, Checks, Chip, Found, View, abbreviate};
use crate::extensions::{self, Extension};
use crate::upstream::{self, Version};
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
    /// Holds what the checks found, rebuilt with each result.
    holder: gtk::Box,
}

impl Checked {
    fn loading(&self) {
        self.clear();
        self.holder.append(&label("Checking…", "dim-label", None));
    }

    fn clear(&self) {
        while let Some(child) = self.holder.first_child() {
            self.holder.remove(&child);
        }
    }

    /// The installed version if no chip has it, a chip per version found (with the icons of the
    /// sources that have it), and what is to be said where a check has none.
    fn render(&self, view: View) {
        self.clear();
        if let Some(installed) = view.installed {
            self.holder.append(&installed_widget(&installed));
        }
        for chip in &view.chips {
            self.holder.append(&chip_widget(chip));
        }
        if let Some(note) = view.note {
            self.holder
                .append(&label(note.text, note.class, note.tip.as_deref()));
        }
    }
}

fn label(text: &str, class: &str, tip: Option<&str>) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .css_classes([class])
        .ellipsize(gtk::pango::EllipsizeMode::End)
        .max_width_chars(24)
        .tooltip_text(tip.unwrap_or_default())
        .valign(gtk::Align::Center)
        .build()
}

/// The installed version, with the mark of this computer.
fn installed_widget(version: &str) -> gtk::Box {
    let widget = gtk::Box::builder()
        .spacing(4)
        .valign(gtk::Align::Center)
        .tooltip_text(format!("Installed: {version}"))
        .build();
    widget.append(
        &gtk::Image::builder()
            .icon_name("computer-symbolic")
            .pixel_size(16)
            .css_classes(["dim-label"])
            .build(),
    );
    widget.append(&label(
        &abbreviate(version),
        "dim-label",
        Some(&format!("Installed: {version}")),
    ));
    widget
}

/// A version with the icons of its sources, in the colour of how it stands against what is
/// installed: green if that is up to date, yellow if the version is newer, grey if the two cannot
/// be compared.
fn chip_widget(chip: &Chip) -> gtk::Box {
    let class = match chip.state {
        Version::Same => "success",
        Version::Newer => "warning",
        Version::Unknown => "dim-label",
    };
    let widget = gtk::Box::builder()
        .spacing(4)
        .valign(gtk::Align::Center)
        .tooltip_text(&chip.tip)
        .build();
    for (source, _) in &chip.sources {
        widget.append(
            &gtk::Image::builder()
                .icon_name(source.icon())
                .pixel_size(16)
                .css_classes([class])
                .build(),
        );
    }
    widget.append(&label(&abbreviate(&chip.version), class, Some(&chip.tip)));
    widget
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
            if *system {
                row.add_suffix(&version_label(e));
            } else {
                let holder = gtk::Box::builder()
                    .spacing(8)
                    .valign(gtk::Align::Center)
                    .build();
                row.add_suffix(&holder);
                checked.push(Checked {
                    uuid: e.uuid.clone(),
                    version_name: e.version_name.clone(),
                    counter: e.counter,
                    url: e.url.clone(),
                    holder,
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
        .label(abbreviate(&version))
        .css_classes(["dim-label"])
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
            r.loading();
        }
        let (uuid, url) = (r.uuid.clone(), r.url.clone());
        let token = token.clone();
        let (row, checker, pending) = (r.clone(), checker.clone(), pending.clone());
        glib::spawn_future_local(async move {
            let have_token = token.is_some();
            let (u, url2, shell) = (uuid.clone(), url.clone(), checker.shell.clone());
            let (checks, fresh) = gio::spawn_blocking(move || match cached {
                Some(found) => (Checks::from_found(found), false),
                None => (
                    check::run(&u, url2.as_deref(), token.as_deref(), shell.as_deref()),
                    true,
                ),
            })
            .await
            .unwrap_or_else(|_| {
                let crashed = || upstream::Error::Other("The check crashed".into());
                (
                    Checks {
                        upstream: Err(crashed()),
                        ego: Err(crashed()),
                    },
                    false,
                )
            });
            if checker.run.get() != run {
                return;
            }
            if let (Some(found), true) = (checks.found(), fresh) {
                checker
                    .cache
                    .borrow_mut()
                    .put(&uuid, url.as_deref(), found, now);
            }
            if checks.rate_limited() {
                checker.banner.set_title(if have_token {
                    "GitHub's rate limit is used up. Try again later."
                } else {
                    "GitHub's rate limit is used up. An access token raises it."
                });
                checker.banner.set_revealed(true);
            }
            row.render(check::view(
                &checks,
                row.version_name.as_deref(),
                row.counter,
            ));
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
