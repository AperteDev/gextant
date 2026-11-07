//! Finding the installed extensions: every `<data dir>/gnome-shell/extensions/<uuid>/metadata.json`.

use gtk::prelude::*;
use gtk::{gio, glib};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Clone)]
pub struct Extension {
    pub uuid: String,
    pub name: String,
    /// The author's `version-name`, if any.
    pub version_name: Option<String>,
    /// `version`: a counter EGO assigns, not comparable with the upstream's releases.
    pub counter: Option<u64>,
    /// The `url` of the metadata: the upstream's page, used later to find its repository.
    pub url: Option<String>,
    /// `shell-version`: the GNOME Shell versions it says it supports; empty if it names none.
    pub shell_versions: Vec<String>,
    /// The extension's folder.
    pub path: PathBuf,
    /// The whole `metadata.json`, for the details.
    pub metadata: serde_json::Map<String, serde_json::Value>,
    /// Another copy of this UUID is found first, so the Shell loads that one: its folder, and
    /// whether it is a system one.
    pub shadowed_by: Option<(PathBuf, bool)>,
    /// From a system data dir (`/usr/share`), not the user's: not ours to update.
    pub system: bool,
}

/// The running GNOME Shell's major version (`51` of `51.1`), from the Extensions service it
/// offers on the session bus; `None` where there is no Shell to ask.
pub fn shell_major() -> Option<String> {
    let version = shell_proxy()?.cached_property("ShellVersion")?;
    Some(version.str()?.split('.').next()?.to_owned())
}

fn shell_proxy() -> Option<gio::DBusProxy> {
    gio::DBusProxy::for_bus_sync(
        gio::BusType::Session,
        gio::DBusProxyFlags::NONE,
        None,
        "org.gnome.Shell.Extensions",
        "/org/gnome/Shell/Extensions",
        "org.gnome.Shell.Extensions",
        gio::Cancellable::NONE,
    )
    .ok()
}

/// What the running Shell says of an extension it has loaded.
pub struct ShellInfo {
    /// The Shell's `ExtensionState`: 1 active, 2 inactive, 3 error, 4 out of date, …
    pub state: u32,
    pub enabled: bool,
    pub can_change: bool,
    pub error: String,
    pub has_prefs: bool,
}

/// Ask the Shell (`ListExtensions`) about `e`. `None` where there is no Shell to ask, or it
/// knows another copy of the UUID (the system's, say): the folder is compared.
pub fn shell_info(e: &Extension) -> Option<ShellInfo> {
    let all = shell_proxy()?
        .call_sync(
            "ListExtensions",
            None,
            gio::DBusCallFlags::NONE,
            2000,
            gio::Cancellable::NONE,
        )
        .ok()?;
    let all: HashMap<String, HashMap<String, glib::Variant>> = all.child_value(0).get()?;
    let m = all.get(&e.uuid)?;
    if m.get("path")?.str()? != e.path.to_str()? {
        return None;
    }
    Some(ShellInfo {
        state: m.get("state")?.get::<f64>()? as u32,
        enabled: m.get("enabled")?.get()?,
        can_change: m.get("canChange")?.get()?,
        error: m.get("error")?.str()?.to_owned(),
        has_prefs: m.get("hasPrefs")?.get()?,
    })
}

/// Ask the Shell to switch `uuid` on or off, as the Extensions app does; whether it did.
pub fn set_enabled(uuid: &str, enable: bool) -> bool {
    let method = if enable {
        "EnableExtension"
    } else {
        "DisableExtension"
    };
    shell_proxy().is_some_and(|proxy| {
        proxy
            .call_sync(
                method,
                Some(&(uuid,).to_variant()),
                gio::DBusCallFlags::NONE,
                2000,
                gio::Cancellable::NONE,
            )
            .ok()
            .and_then(|r| r.child_value(0).get::<bool>())
            .unwrap_or(false)
    })
}

/// Ask the Shell to open `uuid`'s preferences, as the Extensions app does.
pub fn open_prefs(uuid: &str) -> bool {
    let params = glib::Variant::tuple_from_iter([
        uuid.to_variant(),
        "".to_variant(),
        glib::VariantDict::new(None).end(),
    ]);
    shell_proxy().is_some_and(|proxy| {
        proxy
            .call_sync(
                "OpenExtensionPrefs",
                Some(&params),
                gio::DBusCallFlags::NONE,
                2000,
                gio::Cancellable::NONE,
            )
            .is_ok()
    })
}

impl Extension {
    /// Whether it supports GNOME Shell `shell` (a major version): by its major version, as the
    /// Shell compares (`45` and `45.1` both fit 45). One that names no versions is not doubted.
    pub fn supports(&self, shell: &str) -> bool {
        self.shell_versions.is_empty()
            || self
                .shell_versions
                .iter()
                .any(|v| v.split('.').next() == Some(shell))
    }

    /// Why this copy is not the one in use, if another is: where that one is, and whether it is
    /// a link to this one (a package linked into the user's folder).
    pub fn shadow_note(&self) -> Option<String> {
        let (path, system) = self.shadowed_by.as_ref()?;
        let whose = if *system { "The system's" } else { "Your" };
        let linked = std::fs::canonicalize(path).ok() == std::fs::canonicalize(&self.path).ok();
        let how = if linked {
            "a link to this one"
        } else {
            "used instead"
        };
        Some(format!(
            "Not in use: {whose} copy in {} is {how}",
            path.display()
        ))
    }

    /// What to show: the `version-name`, else the counter.
    pub fn version(&self) -> Option<String> {
        self.version_name
            .clone()
            .or_else(|| self.counter.map(|c| c.to_string()))
    }
}

/// Where the user's data is, as the Shell sees it. In a Flatpak `XDG_DATA_HOME` is the app's
/// own private folder; the extensions are in the host's (shared with `--filesystem=xdg-data/…`).
fn user_data_dir() -> PathBuf {
    if !in_flatpak() {
        return glib::user_data_dir();
    }
    std::env::var_os("HOST_XDG_DATA_HOME")
        .map_or_else(|| glib::home_dir().join(".local/share"), PathBuf::from)
}

pub fn in_flatpak() -> bool {
    Path::new("/.flatpak-info").exists()
}

/// The user's extensions, then the system's. A UUID found twice is listed twice: the first,
/// the user's, is the one the Shell loads; the other says it is shadowed. A Flatpak sees the runtime's `/usr/share`, not the
/// host's, so it lists the user's only.
pub fn installed() -> Vec<Extension> {
    let user = std::iter::once((user_data_dir(), false));
    let system_dirs = if in_flatpak() {
        Vec::new()
    } else {
        glib::system_data_dirs()
    };
    let system = system_dirs.into_iter().map(|d| (d, true));
    let mut found: Vec<Extension> = Vec::new();
    for (dir, system) in user.chain(system) {
        let Ok(entries) = std::fs::read_dir(dir.join("gnome-shell/extensions")) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Some(mut ext) = read(&entry.path(), system) {
                ext.shadowed_by = found
                    .iter()
                    .find(|e| e.uuid == ext.uuid)
                    .map(|e| (e.path.clone(), e.system));
                found.push(ext);
            }
        }
    }
    found.sort_by_cached_key(|e| e.name.to_lowercase());
    found
}

/// The extension in folder `dir`; none if it has no readable `metadata.json` with a `uuid`.
fn read(dir: &Path, system: bool) -> Option<Extension> {
    let text = std::fs::read_to_string(dir.join("metadata.json")).ok()?;
    let mut ext = parse(&text, system)?;
    ext.path = dir.to_owned();
    Some(ext)
}

pub(crate) fn parse(text: &str, system: bool) -> Option<Extension> {
    let serde_json::Value::Object(m) = serde_json::from_str(text).ok()? else {
        return None;
    };
    let str_of = |key: &str| m.get(key)?.as_str().map(str::to_owned);
    let uuid = str_of("uuid")?;
    Some(Extension {
        name: str_of("name").unwrap_or_else(|| uuid.clone()),
        uuid,
        version_name: str_of("version-name"),
        counter: m.get("version").and_then(|v| v.as_u64()),
        url: str_of("url"),
        shell_versions: m
            .get("shell-version")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default(),
        path: PathBuf::new(),
        shadowed_by: None,
        metadata: m,
        system,
    })
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn version_name_wins_over_version() {
        let e = parse(
            r#"{"uuid":"a@b","name":"A","version":7,"version-name":"1.2.0"}"#,
            false,
        )
        .unwrap();
        assert_eq!(
            (e.name.as_str(), e.version().as_deref()),
            ("A", Some("1.2.0"))
        );
        let e = parse(r#"{"uuid":"a@b","version":7}"#, true).unwrap();
        assert_eq!(
            (e.name.as_str(), e.version().as_deref()),
            ("a@b", Some("7"))
        );
        assert!(e.system);
    }

    #[test]
    fn supports_the_shell_by_major_version() {
        let e = parse(
            r#"{"uuid":"a@b","shell-version":["3.38","45","46.1"]}"#,
            false,
        )
        .unwrap();
        assert!(e.supports("45") && e.supports("46"));
        assert!(!e.supports("47") && !e.supports("38"));
        assert!(parse(r#"{"uuid":"a@b"}"#, false).unwrap().supports("51"));
    }

    #[test]
    fn needs_a_uuid() {
        assert!(parse(r#"{"name":"A"}"#, false).is_none());
        assert!(parse("not json", false).is_none());
    }
}
