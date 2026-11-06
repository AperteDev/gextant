//! The main window: the installed extensions, the user's and the system's, with what the
//! user's have upstream.

use crate::cache::{self, Cache};
use crate::check::{self, Checks, Chip, Found, View, abbreviate};
use crate::extensions::{self, Extension};
use crate::token::Tokens;
use crate::upstream::{self, Version};
use crate::{details, gitlab, preferences};
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
    /// What the last check showed, for the details.
    view: Rc<RefCell<Option<View>>>,
    row: adw::ActionRow,
    /// Marks the row when a source has a newer version.
    update_icon: gtk::Image,
    /// Updated here: the new version runs after the next login, and until then there is
    /// nothing to check.
    updated: Rc<Cell<bool>>,
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
        if self.updated.get() {
            return;
        }
        self.clear();
        if let Some(installed) = &view.installed {
            self.holder.append(&installed_widget(installed));
        }
        for chip in &view.chips {
            self.holder.append(&chip_widget(chip));
        }
        let update = view.update();
        self.show_update_icon(update.is_some());
        self.update_icon
            .set_tooltip_text(update.map(|v| format!("Update available: {v}")).as_deref());
        *self.view.borrow_mut() = Some(view);
    }

    /// Only in the row while there is an update: an empty prefix still takes room.
    fn show_update_icon(&self, show: bool) {
        match (show, self.update_icon.parent().is_some()) {
            (true, false) => self.row.add_prefix(&self.update_icon),
            (false, true) => self.row.remove(&self.update_icon),
            _ => {}
        }
    }

    fn mark_updated(&self) {
        self.updated.set(true);
        self.show_update_icon(false);
        self.clear();
        self.holder
            .append(&label("Updated, log out to apply", "accent", None));
    }

    fn has_update(&self) -> bool {
        !self.updated.get() && (self.view.borrow().as_ref()).is_some_and(|v| v.update().is_some())
    }
}

fn label(text: &str, class: &str, tip: Option<&str>) -> gtk::Label {
    gtk::Label::builder()
        .label(text)
        .css_classes(if class.is_empty() {
            vec![]
        } else {
            vec![class]
        })
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
/// installed: quiet where that is up to date (the icons dimmed, the version plain), blue in a
/// pill if the version is newer, grey if the two cannot be compared; yellow with a `?` where
/// the source has no version.
fn chip_widget(chip: &Chip) -> gtk::Box {
    let (icons, text) = match (chip.missing, chip.state) {
        (false, Version::Same) => ("dim-label", ""),
        _ => (chip.class(), chip.class()),
    };
    let widget = gtk::Box::builder()
        .spacing(4)
        .valign(gtk::Align::Center)
        .tooltip_text(&chip.tip)
        .build();
    if !chip.missing && chip.state == Version::Newer {
        widget.add_css_class("update-chip");
    }
    for (source, _) in &chip.sources {
        widget.append(
            &gtk::Image::builder()
                .icon_name(source.icon())
                .pixel_size(16)
                .css_classes([icons])
                .build(),
        );
    }
    widget.append(&label(&abbreviate(&chip.version), text, Some(&chip.tip)));
    widget
}

/// What checking needs: the rows, the button that waits for the checks and the banner for the
/// rate limit.
struct Checker {
    rows: Vec<Checked>,
    button: gtk::Button,
    banner: adw::Banner,
    /// For a token its forge refuses, and which forges (this run's).
    token_banner: adw::Banner,
    refused: RefCell<Vec<&'static str>>,
    /// Counts the runs: a result from an older one (the token changed meanwhile) is dropped.
    run: Cell<u32>,
    cache: RefCell<Cache>,
    /// The running Shell's major version, for extensions.gnome.org.
    shell: Option<String>,
    /// The user's extensions, where the ones with an update go first, and the count in the title.
    group: adw::PreferencesGroup,
    title: adw::WindowTitle,
}

pub fn build(app: &adw::Application) -> adw::ApplicationWindow {
    let ui = gtk::Builder::from_resource("/dev/aperte/Gextant/window.ui");
    let object = |id: &str| ui.object::<glib::Object>(id).expect("object in window.blp");
    let window: adw::ApplicationWindow = object("window").downcast().unwrap();
    window.set_application(Some(app));
    // A development build (debug: `cargo run`, `make sandbox`; packages are release builds) has
    // the stripes of GNOME's own.
    if cfg!(debug_assertions) {
        window.add_css_class("devel");
    }

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
    let restart_banner: adw::Banner = object("restart_banner").downcast().unwrap();
    let window_title: adw::WindowTitle = object("window_title").downcast().unwrap();
    let all = extensions::installed();
    let shell = extensions::shell_major();
    let mut checked = Vec::new();
    for (system, group) in &groups {
        let mut rows = all.iter().filter(|e| e.system == *system).peekable();
        group.set_visible(rows.peek().is_some());
        for e in rows {
            let row = row(e);
            let view = Rc::new(RefCell::new(None));
            let mut on_updated: Rc<dyn Fn()> = Rc::new(|| {});
            if let Some(note) = e.shadow_note() {
                row.add_prefix(&warning_icon(&note));
            }
            if let Some(shell) = shell.as_deref().filter(|s| !e.supports(s)) {
                row.add_prefix(&incompatible_icon(e, shell));
            }
            if *system {
                row.add_suffix(&version_label(e));
            } else {
                let holder = gtk::Box::builder()
                    .spacing(8)
                    .valign(gtk::Align::Center)
                    .build();
                row.add_suffix(&holder);
                let update_icon = gtk::Image::builder()
                    .icon_name("software-update-available-symbolic")
                    .css_classes(["accent"])
                    .build();
                checked.push(Checked {
                    uuid: e.uuid.clone(),
                    version_name: e.version_name.clone(),
                    counter: e.counter,
                    url: e.url.clone(),
                    holder,
                    view: view.clone(),
                    row: row.clone(),
                    update_icon,
                    updated: Rc::default(),
                });
                let (target, banner, title) = (
                    checked.last().cloned().unwrap(),
                    restart_banner.clone(),
                    window_title.clone(),
                );
                on_updated = Rc::new(move || {
                    target.mark_updated();
                    banner.set_revealed(true);
                    title.set_subtitle("");
                });
            }
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            row.set_activatable(true);
            row.connect_activated({
                let (window, e, shell) = (window.clone(), e.clone(), shell.clone());
                move |_| {
                    let view = view.borrow();
                    details::present(
                        &window,
                        &e,
                        shell.as_deref(),
                        view.as_ref(),
                        on_updated.clone(),
                    )
                }
            });
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
        refused: RefCell::new(Vec::new()),
        run: Cell::new(0),
        cache: RefCell::new(Cache::load()),
        shell,
        group: groups[0].1.clone(),
        title: window_title,
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
        .build()
}

/// The mark on a row that has something to say, in its tooltip.
fn warning_icon(tip: &str) -> gtk::Image {
    gtk::Image::builder()
        .icon_name("dialog-warning-symbolic")
        .css_classes(["warning"])
        .tooltip_text(tip)
        .build()
}

/// The mark on a row whose extension does not list the running Shell among its versions.
fn incompatible_icon(e: &Extension, shell: &str) -> gtk::Image {
    gtk::Image::builder()
        .icon_name("dialog-warning-symbolic")
        .css_classes(["warning"])
        .tooltip_text(format!(
            "Not compatible with GNOME Shell {shell}: it supports {}",
            e.shell_versions.join(", ")
        ))
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
    checker.refused.borrow_mut().clear();
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
    let tokens = Tokens::load();
    // Only when something is about to be asked of a forge: the tokens matter then.
    if cached.iter().any(Option::is_none) {
        if let Some(token) = tokens.github.clone() {
            refused_check(checker, run, "GitHub", move || {
                upstream::token_refused(&token)
            });
        }
        if let Some(token) = tokens.gitlab.clone() {
            refused_check(checker, run, "GitLab", move || {
                gitlab::token_refused(&token)
            });
        }
    }
    let pending = Rc::new(Cell::new(checker.rows.len()));
    for (r, cached) in checker.rows.iter().zip(cached) {
        if cached.is_none() {
            r.loading();
        }
        let (uuid, url) = (r.uuid.clone(), r.url.clone());
        let tokens = tokens.clone();
        let (row, checker, pending) = (r.clone(), checker.clone(), pending.clone());
        glib::spawn_future_local(async move {
            let tokens_now = tokens.clone();
            let (u, url2, shell) = (uuid.clone(), url.clone(), checker.shell.clone());
            let (checks, fresh) = gio::spawn_blocking(move || match cached {
                Some(found) => (Checks::from_found(found), false),
                None => (
                    check::run(&u, url2.as_deref(), &tokens, shell.as_deref()),
                    true,
                ),
            })
            .await
            .unwrap_or_else(|_| {
                let crashed = || upstream::Error::Other("The check crashed".into());
                (
                    Checks {
                        upstream: Err(upstream::Failure {
                            error: crashed(),
                            source: upstream::Source::Web,
                        }),
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
            if let Some(host) = checks.rate_limited() {
                show_rate_limit(&checker.banner, host, &tokens_now);
            }
            row.render(check::view(
                &checks,
                row.version_name.as_deref(),
                row.counter,
            ));
            pending.set(pending.get() - 1);
            if pending.get() == 0 {
                checker.button.set_sensitive(true);
                show_updates(&checker);
                let mut cache = checker.cache.borrow_mut();
                let uuids: Vec<&str> = checker.rows.iter().map(|r| r.uuid.as_str()).collect();
                cache.retain(&uuids);
                cache.save();
            }
        });
    }
}

/// Ask `refused` (on a thread) whether `forge` refuses its token; if so, say so in the banner,
/// with the other forge's if that is refused too.
fn refused_check(
    checker: &Rc<Checker>,
    run: u32,
    forge: &'static str,
    refused: impl FnOnce() -> bool + Send + 'static,
) {
    let checker = checker.clone();
    glib::spawn_future_local(async move {
        if matches!(gio::spawn_blocking(refused).await, Ok(true)) && checker.run.get() == run {
            let mut names = checker.refused.borrow_mut();
            names.push(forge);
            names.sort_unstable();
            checker.token_banner.set_title(&match names.as_slice() {
                [one] => format!("Your {one} access token expired or was revoked."),
                _ => format!(
                    "Your {} access tokens expired or were revoked.",
                    names.join(" and ")
                ),
            });
            checker.token_banner.set_revealed(true);
        }
    });
}

/// The banner for a host that has had enough: where a token would raise the limit (GitHub's, and
/// gitlab.com's) and there is none, the way to add one; otherwise only to wait.
fn show_rate_limit(banner: &adw::Banner, host: &str, tokens: &Tokens) {
    let github = host.ends_with("github.com");
    let help = if github {
        tokens.github.is_none()
    } else {
        host == "gitlab.com" && tokens.gitlab.is_none()
    };
    let forge = if github { "GitHub" } else { host };
    banner.set_title(&if help {
        format!("{forge}'s rate limit is used up. An access token raises it.")
    } else {
        format!("{forge}'s rate limit is used up. Try again later.")
    });
    banner.set_button_label(help.then_some("Preferences"));
    banner.set_revealed(true);
}

/// Say in the title how many have an update, and put those first in the list (the order of the
/// rest kept).
fn show_updates(checker: &Checker) {
    let (first, rest): (Vec<&Checked>, Vec<&Checked>) =
        checker.rows.iter().partition(|r| r.has_update());
    checker.title.set_subtitle(&match first.len() {
        0 => String::new(),
        1 => "1 update available".to_owned(),
        n => format!("{n} updates available"),
    });
    for r in &checker.rows {
        checker.group.remove(&r.row);
    }
    let any = !first.is_empty();
    for r in first.into_iter().chain(rest) {
        checker.group.add(&r.row);
    }
    // Taking the rows out leaves the page scrolled to somewhere else.
    if any
        && let Some(page) = checker
            .group
            .ancestor(adw::PreferencesPage::static_type())
            .and_downcast::<adw::PreferencesPage>()
    {
        // After the layout: now the page would be put back to where focus was.
        glib::idle_add_local_once(move || page.scroll_to_top());
    }
}
