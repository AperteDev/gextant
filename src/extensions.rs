//! Finding the installed extensions: every `<data dir>/gnome-shell/extensions/<uuid>/metadata.json`.

use gtk::prelude::*;
use gtk::{gio, glib};
use std::path::Path;

pub struct Extension {
    pub uuid: String,
    pub name: String,
    /// The author's `version-name`, if any.
    pub version_name: Option<String>,
    /// `version`: a counter EGO assigns, not comparable with the upstream's releases.
    pub counter: Option<u64>,
    /// The `url` of the metadata: the upstream's page, used later to find its repository.
    pub url: Option<String>,
    /// From a system data dir (`/usr/share`), not the user's: not ours to update.
    pub system: bool,
}

/// The running GNOME Shell's major version (`51` of `51.1`), from the Extensions service it
/// offers on the session bus; `None` where there is no Shell to ask.
pub fn shell_major() -> Option<String> {
    let proxy = gio::DBusProxy::for_bus_sync(
        gio::BusType::Session,
        gio::DBusProxyFlags::NONE,
        None,
        "org.gnome.Shell.Extensions",
        "/org/gnome/Shell/Extensions",
        "org.gnome.Shell.Extensions",
        gio::Cancellable::NONE,
    )
    .ok()?;
    let version = proxy.cached_property("ShellVersion")?;
    Some(version.str()?.split('.').next()?.to_owned())
}

impl Extension {
    /// What to show: the `version-name`, else the counter.
    pub fn version(&self) -> Option<String> {
        self.version_name
            .clone()
            .or_else(|| self.counter.map(|c| c.to_string()))
    }
}

/// The user's extensions, then the system's. A UUID found twice is listed once, from the first
/// place, the user's, as the Shell loads it.
pub fn installed() -> Vec<Extension> {
    let user = std::iter::once((glib::user_data_dir(), false));
    let system = glib::system_data_dirs().into_iter().map(|d| (d, true));
    let mut found: Vec<Extension> = Vec::new();
    for (dir, system) in user.chain(system) {
        let Ok(entries) = std::fs::read_dir(dir.join("gnome-shell/extensions")) else {
            continue;
        };
        for entry in entries.flatten() {
            if let Some(ext) = read(&entry.path(), system)
                && !found.iter().any(|e| e.uuid == ext.uuid)
            {
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
    parse(&text, system)
}

fn parse(text: &str, system: bool) -> Option<Extension> {
    let m: serde_json::Value = serde_json::from_str(text).ok()?;
    let str_of = |key: &str| m.get(key)?.as_str().map(str::to_owned);
    let uuid = str_of("uuid")?;
    Some(Extension {
        name: str_of("name").unwrap_or_else(|| uuid.clone()),
        uuid,
        version_name: str_of("version-name"),
        counter: m.get("version").and_then(|v| v.as_u64()),
        url: str_of("url"),
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
    fn needs_a_uuid() {
        assert!(parse(r#"{"name":"A"}"#, false).is_none());
        assert!(parse("not json", false).is_none());
    }
}
