//! What the last checks found, kept for a few hours so a start does not ask the forges about every
//! extension again. In the cache folder, so it can be deleted any time; a failure is never kept.

use crate::check::Found;
use crate::upstream::Upstream;
use gtk::glib;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const HOUR: u64 = 3600;

/// How long a result stays good: a missing repository is looked for again less often (it costs
/// a project page each time), a release more often, as that is what changes.
fn lifetime(found: &Found) -> u64 {
    match found.upstream {
        Upstream::NoRepo(_) => 24 * HOUR,
        _ => 6 * HOUR,
    }
}

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

#[derive(Serialize, Deserialize)]
struct Entry {
    /// When it was found (unix time), and for which `url`: another one asks again.
    at: u64,
    url: String,
    /// With prereleases or without: the other setting asks again.
    #[serde(default)]
    pre: bool,
    found: Found,
}

#[derive(Default, Serialize, Deserialize)]
pub struct Cache(HashMap<String, Entry>);

fn path() -> PathBuf {
    glib::user_cache_dir().join("gextant/checks.json")
}

impl Cache {
    /// What was kept; empty if there is nothing, or nothing readable.
    pub fn load() -> Self {
        std::fs::read_to_string(path())
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok())
            .unwrap_or_default()
    }

    /// Best effort: a cache that cannot be written is only a slower start.
    pub fn save(&self) {
        let path = path();
        let _ = std::fs::create_dir_all(path.parent().unwrap());
        let tmp = path.with_extension("tmp");
        if let Ok(text) = serde_json::to_string(self)
            && std::fs::write(&tmp, text).is_ok()
        {
            let _ = std::fs::rename(tmp, path);
        }
    }

    pub fn get(&self, uuid: &str, url: Option<&str>, pre: bool, now: u64) -> Option<Found> {
        let e = self.0.get(uuid)?;
        let fresh = now.saturating_sub(e.at) < lifetime(&e.found);
        (fresh && e.pre == pre && e.url == url.unwrap_or_default()).then(|| e.found.clone())
    }

    pub fn put(&mut self, uuid: &str, url: Option<&str>, pre: bool, found: Found, now: u64) {
        let url = url.unwrap_or_default().to_owned();
        self.0.insert(
            uuid.to_owned(),
            Entry {
                at: now,
                url,
                pre,
                found,
            },
        );
    }

    /// Forget the extensions that are not installed any more.
    pub fn retain(&mut self, uuids: &[&str]) {
        self.0.retain(|uuid, _| uuids.contains(&uuid.as_str()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::upstream::Source;

    fn found(upstream: Upstream) -> Found {
        Found {
            upstream,
            ego: Some(7),
        }
    }

    fn tag(t: &str) -> Found {
        found(Upstream::Tag {
            tag: t.into(),
            source: Source::GitHub,
            repo: String::new(),
        })
    }

    #[test]
    fn results_expire_and_follow_the_url() {
        let mut c = Cache::default();
        let (u, url) = ("a@b", Some("https://github.com/o/r"));
        c.put(u, url, false, tag("v1"), 1000);
        assert_eq!(c.get(u, url, false, 1000 + 6 * HOUR - 1), Some(tag("v1")));
        assert_eq!(c.get(u, url, false, 1000 + 6 * HOUR), None);
        assert_eq!(c.get(u, url, true, 1000), None); // asked with prereleases: ask again
        assert_eq!(
            c.get(u, Some("https://github.com/o/other"), false, 1000),
            None
        );
        c.put(u, None, false, found(Upstream::NoRepo(None)), 1000);
        assert_eq!(
            c.get(u, None, false, 1000 + 23 * HOUR),
            Some(found(Upstream::NoRepo(None)))
        );
        assert_eq!(c.get(u, None, false, 1000 + 24 * HOUR), None);
    }

    #[test]
    fn survives_a_round_trip_and_forgets_removed_extensions() {
        let mut c = Cache::default();
        c.put(
            "a@b",
            None,
            false,
            found(Upstream::NoRelease(Source::GitLab)),
            5,
        );
        c.put("c@d", None, false, tag("2"), 5);
        c.retain(&["a@b"]);
        let back: Cache = serde_json::from_str(&serde_json::to_string(&c).unwrap()).unwrap();
        assert_eq!(
            back.get("a@b", None, false, 6),
            Some(found(Upstream::NoRelease(Source::GitLab)))
        );
        assert_eq!(back.get("c@d", None, false, 6), None);
    }
}
