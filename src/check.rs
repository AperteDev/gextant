//! The two checks of an extension, asked separately: its upstream (a repository) and
//! extensions.gnome.org (EGO); and what a row shows of both against what is installed.

use crate::ego;
use crate::upstream::{
    self, Error, Source, Upstream, Version, compare, compare_counter, display_version,
};
use serde::{Deserialize, Serialize};

/// What the two checks found: what is kept between runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Found {
    pub upstream: Upstream,
    /// The latest version EGO has for this Shell, if it knows the extension.
    pub ego: Option<u64>,
}

/// The two checks, each with its own result: one failing leaves the other's answer to show.
pub struct Checks {
    pub upstream: Result<Upstream, Error>,
    pub ego: Result<Option<u64>, Error>,
}

impl Checks {
    pub fn from_found(found: Found) -> Self {
        Checks {
            upstream: Ok(found.upstream),
            ego: Ok(found.ego),
        }
    }

    /// What may be kept: only if both worked.
    pub fn found(&self) -> Option<Found> {
        match (&self.upstream, &self.ego) {
            (Ok(upstream), Ok(ego)) => Some(Found {
                upstream: upstream.clone(),
                ego: *ego,
            }),
            _ => None,
        }
    }

    /// The host that said too many requests were made, if one did.
    pub fn rate_limited(&self) -> Option<&str> {
        match &self.upstream {
            Err(Error::RateLimited(host)) => Some(host),
            _ => None,
        }
    }
}

/// Ask both, at once: the upstream named by the metadata `url`, and EGO (for GNOME Shell `shell`,
/// if known).
pub fn run(uuid: &str, url: Option<&str>, token: Option<&str>, shell: Option<&str>) -> Checks {
    std::thread::scope(|s| {
        let ego = s.spawn(|| match shell {
            Some(shell) => ego::latest(uuid, shell),
            None => Ok(None),
        });
        let upstream = upstream::find(uuid, url, token);
        let ego = ego
            .join()
            .unwrap_or_else(|_| Err(Error::Other("The check crashed".into())));
        Checks { upstream, ego }
    })
}

/// A version a row shows, with the sources that have it: two that agree make one chip.
pub struct Chip {
    /// Each source, with what it calls the version.
    pub sources: Vec<(Source, String)>,
    pub version: String,
    /// Against what is installed: `Newer` means the installed one is outdated.
    pub state: Version,
    pub tip: String,
}

/// Said instead of a version where a check has none.
pub struct Note {
    pub text: &'static str,
    /// `dim-label`, `warning` or `error`.
    pub class: &'static str,
    pub tip: Option<String>,
}

pub struct View {
    /// The installed version, when no chip shows it already.
    pub installed: Option<String>,
    pub chips: Vec<Chip>,
    pub note: Option<Note>,
}

fn rank(v: Version) -> u8 {
    match v {
        Version::Same => 0,
        Version::Unknown => 1,
        Version::Newer => 2,
    }
}

/// A version as short as a row has room for: `2.1.2-17+17d5fd1-dirty` is `2.1.2-17+17…`. The
/// whole one goes in the tooltip.
pub fn abbreviate(version: &str) -> String {
    const MAX: usize = 12;
    if version.chars().count() <= MAX {
        return version.to_owned();
    }
    let head: String = version.chars().take(MAX - 1).collect();
    format!("{head}…")
}

pub fn view(checks: &Checks, version_name: Option<&str>, counter: Option<u64>) -> View {
    let installed = version_name
        .map(str::to_owned)
        .or_else(|| counter.map(|c| c.to_string()));
    // Source, what it calls the version, the version, how it compares.
    let mut found: Vec<(Source, String, String, Version)> = Vec::new();
    let mut note = None;
    match &checks.upstream {
        Ok(Upstream::Tag { tag, source }) => found.push((
            *source,
            tag.clone(),
            display_version(tag, version_name, counter),
            compare(version_name, counter, tag),
        )),
        Ok(Upstream::NoRelease(_)) => {
            note = Some(Note {
                text: "No releases",
                class: "dim-label",
                tip: None,
            })
        }
        Ok(Upstream::NoRepo(_)) | Err(Error::Missing) => {
            note = Some(Note {
                text: "No repository found",
                class: "dim-label",
                tip: None,
            })
        }
        Err(Error::RateLimited(host)) => {
            note = Some(Note {
                text: "Rate limit reached",
                class: "warning",
                tip: Some(format!("{host} says too many requests were made")),
            })
        }
        Err(Error::Other(e)) => {
            note = Some(Note {
                text: "Check failed",
                class: "error",
                tip: Some(e.clone()),
            })
        }
    }
    if let Ok(Some(v)) = checks.ego {
        found.push((
            Source::Ego,
            v.to_string(),
            v.to_string(),
            compare_counter(counter, v),
        ));
    }
    // Not having an upstream is nothing to say where EGO has an answer.
    if !found.is_empty() && note.as_ref().is_some_and(|n| n.class == "dim-label") {
        note = None;
    }

    let mut chips: Vec<Chip> = Vec::new();
    for (source, raw, version, state) in found {
        match chips.iter_mut().find(|c| c.version == version) {
            Some(chip) => {
                chip.sources.push((source, raw));
                if rank(state) > rank(chip.state) {
                    chip.state = state;
                }
            }
            None => chips.push(Chip {
                sources: vec![(source, raw)],
                version,
                state,
                tip: String::new(),
            }),
        }
    }
    for chip in &mut chips {
        let mut lines: Vec<String> = chip
            .sources
            .iter()
            .map(|(source, raw)| format!("{}: {raw}", source.name()))
            .collect();
        if let Some(installed) = &installed {
            let verdict = match chip.state {
                Version::Same => "up to date",
                Version::Newer => "update available",
                Version::Unknown => "cannot be compared",
            };
            lines.push(format!("Installed: {installed} ({verdict})"));
        }
        chip.tip = lines.join("\n");
    }
    let shown = chips.iter().any(|c| Some(&c.version) == installed.as_ref());
    View {
        installed: installed.filter(|_| !shown),
        chips,
        note,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(t: &str, source: Source) -> Result<Upstream, Error> {
        Ok(Upstream::Tag {
            tag: t.into(),
            source,
        })
    }

    fn versions(v: &View) -> Vec<(usize, &str, Version)> {
        v.chips
            .iter()
            .map(|c| (c.sources.len(), c.version.as_str(), c.state))
            .collect()
    }

    #[test]
    fn long_versions_are_abbreviated() {
        assert_eq!(abbreviate("2.1.2"), "2.1.2");
        assert_eq!(abbreviate("123456789012"), "123456789012");
        assert_eq!(abbreviate("2.1.2-17+17d5fd1-dirty.fix"), "2.1.2-17+17…");
    }

    #[test]
    fn sources_that_agree_make_one_chip() {
        let c = Checks {
            upstream: tag("v66", Source::GitHub),
            ego: Ok(Some(66)),
        };
        let v = view(&c, None, Some(66));
        assert_eq!(versions(&v), [(2, "66", Version::Same)]);
        assert_eq!(v.installed, None);
        assert!(v.note.is_none());
    }

    #[test]
    fn sources_that_differ_make_two() {
        let c = Checks {
            upstream: tag("v67", Source::GitHub),
            ego: Ok(Some(66)),
        };
        let v = view(&c, None, Some(66));
        assert_eq!(
            versions(&v),
            [(1, "67", Version::Newer), (1, "66", Version::Same)]
        );
        assert_eq!(v.installed, None);
    }

    #[test]
    fn the_installed_version_shows_when_no_chip_has_it() {
        let c = Checks {
            upstream: tag("v67", Source::GitLab),
            ego: Ok(Some(67)),
        };
        let v = view(&c, None, Some(66));
        assert_eq!(versions(&v), [(2, "67", Version::Newer)]);
        assert_eq!(v.installed.as_deref(), Some("66"));
    }

    #[test]
    fn no_upstream_is_not_said_where_the_portal_answers() {
        let none = Checks {
            upstream: Ok(Upstream::NoRepo(None)),
            ego: Ok(None),
        };
        assert_eq!(
            view(&none, None, Some(1)).note.unwrap().text,
            "No repository found"
        );
        let ego = Checks {
            upstream: Ok(Upstream::NoRelease(Source::GitHub)),
            ego: Ok(Some(13)),
        };
        let v = view(&ego, None, Some(13));
        assert!(v.note.is_none());
        assert_eq!(versions(&v), [(1, "13", Version::Same)]);
    }

    #[test]
    fn a_limited_upstream_leaves_the_portal_chip() {
        let c = Checks {
            upstream: Err(Error::RateLimited("gitlab.com".into())),
            ego: Ok(Some(13)),
        };
        let v = view(&c, None, Some(13));
        assert_eq!(v.note.unwrap().text, "Rate limit reached");
        assert_eq!(v.chips.len(), 1);
    }
}
