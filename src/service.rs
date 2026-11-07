//! Gextant without a window: the check at login (`--background`, from an autostart entry), the
//! repeated checks while the app is kept running, the notification and the dock's counter.

use crate::cache::{self, Cache};
use crate::check::{self, Checks};
use crate::extensions;
use crate::settings::{self, Interval};
use crate::token::Tokens;
use adw::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};

/// An extension with a newer version upstream.
pub struct Update {
    pub uuid: String,
    pub name: String,
    pub version: String,
}

thread_local! {
    /// Keeps the app running without a window while there are repeated checks.
    static HOLD: RefCell<Option<gio::ApplicationHoldGuard>> = const { RefCell::new(None) };
    static TIMER: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
    /// `uuid version` of what a notification already told, so the next check stays quiet.
    static TOLD: RefCell<HashSet<String>> = RefCell::new(HashSet::new());
    /// How many updates the last check found; `None` until there was one.
    static COUNT: Cell<Option<u32>> = const { Cell::new(None) };
    /// What the dock was last told; `None`: nothing of ours.
    static SENT: Cell<Option<u32>> = const { Cell::new(None) };
}

/// Make autostart, the hold and the timer what the settings ask for. At start and whenever a
/// setting is kept.
pub fn sync(app: &adw::Application) {
    let interval = settings::load().interval;
    autostart(interval.background());
    HOLD.with_borrow_mut(|hold| match interval.seconds() {
        Some(_) => {
            hold.get_or_insert_with(|| app.hold());
        }
        None => *hold = None,
    });
    TIMER.with_borrow_mut(|timer| {
        if let Some(old) = timer.take() {
            old.remove();
        }
        if let Some(seconds) = interval.seconds() {
            let app = app.clone();
            *timer = Some(glib::timeout_add_seconds_local(seconds, move || {
                // With a window open, its own timer checks.
                if app.windows().is_empty() {
                    scan_and_report(true, settings::load().notifications, None);
                }
                glib::ControlFlow::Continue
            }));
        }
    });
    dock();
}

/// The run `--background` asks for, at login: unless a window is open already, the check of the
/// login (from the cache if it is fresh) with a notification, or the first of the repeated ones.
pub fn background(app: &adw::Application) {
    if !app.windows().is_empty() {
        return;
    }
    match settings::load().interval {
        Interval::Login => scan_and_report(false, true, Some(app.hold())),
        i if i.seconds().is_some() => scan_and_report(false, settings::load().notifications, None),
        _ => {}
    }
}

/// Check on a thread and report; `hold` keeps the app alive until then.
fn scan_and_report(force: bool, notify: bool, hold: Option<gio::ApplicationHoldGuard>) {
    glib::spawn_future_local(async move {
        if let Ok(updates) = gio::spawn_blocking(move || scan(force)).await {
            report(&updates, notify);
        }
        drop(hold);
    });
}

/// Check every user extension, each on a thread of its own, as the window does.
fn scan(force: bool) -> Vec<Update> {
    let (pre, now, tokens) = (settings::load().prereleases, cache::now(), Tokens::load());
    let shell = extensions::shell_major();
    let mine: Vec<_> = extensions::installed()
        .into_iter()
        .filter(|e| !e.system)
        .collect();
    let mut cache = Cache::load();
    let checks: Vec<(Checks, bool)> = std::thread::scope(|s| {
        let jobs: Vec<_> = mine
            .iter()
            .map(|e| {
                let cached = if force {
                    None
                } else {
                    cache.get(&e.uuid, e.url.as_deref(), pre, now)
                };
                let (tokens, shell) = (&tokens, &shell);
                s.spawn(move || match cached {
                    Some(found) => (Checks::from_found(found), false),
                    None => (
                        check::run(&e.uuid, e.url.as_deref(), tokens, shell.as_deref(), pre),
                        true,
                    ),
                })
            })
            .collect();
        jobs.into_iter().filter_map(|j| j.join().ok()).collect()
    });
    let mut updates = Vec::new();
    for (e, (checks, fresh)) in mine.iter().zip(checks) {
        if let (Some(found), true) = (checks.found(), fresh) {
            cache.put(&e.uuid, e.url.as_deref(), pre, found, now);
        }
        let view = check::view(&checks, e.version_name.as_deref(), e.counter);
        if let Some(version) = view.update() {
            updates.push(Update {
                uuid: e.uuid.clone(),
                name: e.name.clone(),
                version: version.to_owned(),
            });
        }
    }
    let uuids: Vec<&str> = mine.iter().map(|e| e.uuid.as_str()).collect();
    cache.retain(&uuids);
    cache.save();
    updates
}

/// What a check found: the dock's number, and a notification if `notify` and there is
/// something it has not told yet.
pub fn report(updates: &[Update], notify: bool) {
    COUNT.set(Some(updates.len() as u32));
    dock();
    let keys: Vec<String> = updates
        .iter()
        .map(|u| format!("{} {}", u.uuid, u.version))
        .collect();
    let new =
        TOLD.with_borrow_mut(|told| keys.iter().fold(false, |n, k| told.insert(k.clone()) | n));
    let Some(app) = gio::Application::default().filter(|_| notify && new) else {
        return;
    };
    let list: Vec<String> = updates
        .iter()
        .map(|u| format!("{} {}", u.name, check::abbreviate(&u.version)))
        .collect();
    let title = match updates.len() {
        1 => "1 extension update available".to_owned(),
        n => format!("{n} extension updates available"),
    };
    let notification = gio::Notification::new(&title);
    notification.set_body(Some(&list.join(", ")));
    notification.set_default_action("app.show");
    app.send_notification(Some("updates"), &notification);
}

/// One update less, once it is installed here.
pub fn updated() {
    COUNT.set(COUNT.get().map(|n| n.saturating_sub(1)));
    dock();
}

/// The number on the app's icon in a dock, through the Unity Launcher API that Ubuntu Dock and
/// Dash to Dock read; none if the setting is off.
pub fn dock() {
    let want = COUNT.get().filter(|_| settings::load().dock_counter);
    let count = want.unwrap_or(0);
    if SENT.get().unwrap_or(0) == count {
        return;
    }
    let Some(conn) = gio::Application::default().and_then(|a| a.dbus_connection()) else {
        return;
    };
    let props = HashMap::from([
        ("count".to_owned(), i64::from(count).to_variant()),
        ("count-visible".to_owned(), (count > 0).to_variant()),
    ]);
    let args = (format!("application://{}.desktop", crate::app_id()), props).to_variant();
    let sent = conn.emit_signal(
        None,
        "/com/canonical/unity/launcher",
        "com.canonical.Unity.LauncherEntry",
        "Update",
        Some(&args),
    );
    if sent.is_ok() {
        SENT.set(Some(count));
    }
}

/// The autostart entry that runs `gextant --background` at login, there or not as asked. Not in
/// the sandbox copy (it would start at the real login) and not in a Flatpak (that takes the
/// Background portal).
fn autostart(want: bool) {
    if std::env::var_os("GEXTANT_SANDBOX").is_some() || extensions::in_flatpak() {
        return;
    }
    let file = glib::user_config_dir()
        .join("autostart")
        .join(format!("{}.desktop", crate::app_id()));
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let entry = format!(
        "[Desktop Entry]\nType=Application\nName=Gextant\nExec=\"{}\" --background\nIcon={}\n\
         NoDisplay=true\nX-GNOME-Autostart-enabled=true\n",
        exe.display(),
        crate::app_id()
    );
    if !want {
        let _ = std::fs::remove_file(&file);
    } else if std::fs::read_to_string(&file).ok().as_deref() != Some(&entry) {
        let _ = std::fs::create_dir_all(file.parent().unwrap());
        let _ = std::fs::write(&file, entry);
    }
}
