//! Gextant's settings (Preferences, General), in `$XDG_DATA_HOME/gextant/settings.json`.

use gtk::glib;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// How often to look for updates by itself.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Interval {
    Manually,
    #[default]
    Start,
    Hour,
    SixHours,
    Day,
}

impl Interval {
    /// In the order of the selector in Preferences.
    pub const ALL: [Interval; 5] = [
        Interval::Manually,
        Interval::Start,
        Interval::Hour,
        Interval::SixHours,
        Interval::Day,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Interval::Manually => "Manually",
            Interval::Start => "On start only",
            Interval::Hour => "Every hour",
            Interval::SixHours => "Every 6 hours",
            Interval::Day => "Every day",
        }
    }

    /// Seconds between checks while running, if it checks again at all.
    pub fn seconds(self) -> Option<u32> {
        match self {
            Interval::Manually | Interval::Start => None,
            Interval::Hour => Some(3600),
            Interval::SixHours => Some(6 * 3600),
            Interval::Day => Some(24 * 3600),
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub interval: Interval,
    /// Also look at prereleases (GitHub only: GitLab has none).
    pub prereleases: bool,
}

fn path() -> PathBuf {
    glib::user_data_dir().join("gextant/settings.json")
}

/// What was kept; the defaults if there is nothing, or nothing readable.
pub fn load() -> Settings {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|t| serde_json::from_str(&t).ok())
        .unwrap_or_default()
}

pub fn save(settings: &Settings) -> std::io::Result<()> {
    let path = path();
    std::fs::create_dir_all(path.parent().unwrap())?;
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, serde_json::to_string_pretty(settings)?)?;
    std::fs::rename(tmp, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_and_partial_files() {
        let d = Settings::default();
        assert_eq!((d.interval, d.prereleases), (Interval::Start, false));
        let s: Settings = serde_json::from_str(r#"{"prereleases":true}"#).unwrap();
        assert_eq!((s.interval, s.prereleases), (Interval::Start, true));
        let s: Settings = serde_json::from_str(r#"{"interval":"six-hours"}"#).unwrap();
        assert_eq!(s.interval, Interval::SixHours);
        assert!(serde_json::from_str::<Settings>(r#"{"interval":"never"}"#).is_err());
    }
}
