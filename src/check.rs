//! The two checks of an extension, asked separately: its upstream (a repository) and
//! extensions.gnome.org (EGO); and what a row shows of both against what is installed.

use crate::ego;
use crate::token::Tokens;
use crate::upstream::{
    self, Error, Failure, Source, Upstream, Version, compare, compare_counter, display_version,
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
    pub upstream: Result<Upstream, Failure>,
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
            Err(Failure {
                error: Error::RateLimited(host),
                ..
            }) => Some(host),
            _ => None,
        }
    }
}

/// Ask both, at once: the upstream named by the metadata `url`, and EGO (for GNOME Shell `shell`,
/// if known).
pub fn run(
    uuid: &str,
    url: Option<&str>,
    tokens: &Tokens,
    shell: Option<&str>,
    prereleases: bool,
) -> Checks {
    std::thread::scope(|s| {
        let ego = s.spawn(|| match shell {
            Some(shell) => ego::latest(uuid, shell),
            None => Ok(None),
        });
        let upstream = upstream::find(uuid, url, tokens, prereleases);
        let ego = ego
            .join()
            .unwrap_or_else(|_| Err(Error::Other("The check crashed".into())));
        Checks { upstream, ego }
    })
}

/// A version a row shows, with the sources that have it: two that agree make one chip. Where a
/// source has no version to show, its chip says `?` instead, and the tooltip why.
pub struct Chip {
    /// Each source, with what it calls the version (or why it has none).
    pub sources: Vec<(Source, String)>,
    pub version: String,
    /// Against what is installed: `Newer` means the installed one is outdated.
    pub state: Version,
    /// The source has no version: it could not be fetched, or there is none.
    pub missing: bool,
    pub tip: String,
}

impl Chip {
    /// The style class of its colour: yellow where there is no version, else green, blue or grey
    /// as the version stands against what is installed.
    pub fn class(&self) -> &'static str {
        match (self.missing, self.state) {
            (true, _) => "warning",
            (_, Version::Same) => "success",
            (_, Version::Newer) => "accent",
            (_, Version::Unknown) => "dim-label",
        }
    }
}

impl View {
    /// The version a source has that is newer than what is installed, if one has.
    pub fn update(&self) -> Option<&str> {
        let chip = self
            .chips
            .iter()
            .find(|c| !c.missing && c.state == Version::Newer)?;
        Some(&chip.version)
    }
}

pub struct View {
    /// The installed version, when no chip shows it already.
    pub installed: Option<String>,
    pub chips: Vec<Chip>,
    /// `host/path` of the repository the upstream's version is from: where to install it from.
    pub repo: Option<String>,
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

/// What a source gave: a version, or why there is none.
enum Entry {
    Version {
        source: Source,
        raw: String,
        version: String,
        state: Version,
    },
    Missing {
        source: Source,
        why: String,
    },
}

pub fn view(checks: &Checks, version_name: Option<&str>, counter: Option<u64>) -> View {
    let installed = version_name
        .map(str::to_owned)
        .or_else(|| counter.map(|c| c.to_string()));
    let missing = |source, why: &str| Entry::Missing {
        source,
        why: why.to_owned(),
    };
    let mut entries: Vec<Entry> = Vec::new();
    entries.push(match &checks.upstream {
        Ok(Upstream::Tag { tag, source, .. }) => Entry::Version {
            source: *source,
            raw: tag.clone(),
            version: display_version(tag, version_name, counter),
            state: compare(version_name, counter, tag),
        },
        Ok(Upstream::NoRelease(source)) => missing(
            *source,
            "The repository has no releases, so there is no version to compare.",
        ),
        Ok(Upstream::NoRepo(Some(_))) => missing(
            Source::Web,
            "No repository was found behind the extension's website.",
        ),
        Ok(Upstream::NoRepo(None)) => missing(
            Source::Web,
            "The extension's metadata names no website or repository.",
        ),
        Err(Failure {
            error: Error::RateLimited(host),
            source,
        }) => missing(
            *source,
            &format!("{host} says too many requests were made. Try again later."),
        ),
        Err(Failure {
            error: Error::Other(e),
            source,
        }) => missing(*source, &format!("The check failed: {e}")),
        Err(Failure {
            error: Error::Missing,
            source,
        }) => missing(*source, "The repository was not found."),
    });
    match &checks.ego {
        Ok(Some(v)) => entries.push(Entry::Version {
            source: Source::Ego,
            raw: v.to_string(),
            version: v.to_string(),
            state: compare_counter(counter, *v),
        }),
        // A portal that does not know the extension is not a problem: not every one is on it.
        Ok(None) => {}
        Err(e) => {
            let why = match e {
                Error::RateLimited(host) => format!("{host} says too many requests were made."),
                Error::Other(e) => format!("extensions.gnome.org could not be asked: {e}"),
                Error::Missing => "extensions.gnome.org does not know the extension.".to_owned(),
            };
            entries.push(missing(Source::Ego, &why));
        }
    }

    let mut chips: Vec<Chip> = Vec::new();
    for entry in entries {
        match entry {
            Entry::Version {
                source,
                raw,
                version,
                state,
            } => {
                match chips
                    .iter_mut()
                    .find(|c| !c.missing && c.version == version)
                {
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
                        missing: false,
                        tip: String::new(),
                    }),
                }
            }
            // Never merged: each has its own reason.
            Entry::Missing { source, why } => chips.push(Chip {
                sources: vec![(source, why)],
                version: "?".to_owned(),
                state: Version::Unknown,
                missing: true,
                tip: String::new(),
            }),
        }
    }
    for chip in &mut chips {
        let mut lines: Vec<String> = chip
            .sources
            .iter()
            .map(|(source, text)| format!("{}: {text}", source.name()))
            .collect();
        if let Some(installed) = &installed {
            let verdict = match (chip.missing, chip.state) {
                (true, _) => None,
                (_, Version::Same) => Some("up to date"),
                (_, Version::Newer) => Some("update available"),
                (_, Version::Unknown) => Some("cannot be compared"),
            };
            lines.push(match verdict {
                Some(v) => format!("Installed: {installed} ({v})"),
                None => format!("Installed: {installed}"),
            });
        }
        chip.tip = lines.join("\n");
    }
    let shown = chips
        .iter()
        .any(|c| !c.missing && Some(&c.version) == installed.as_ref());
    let repo = match &checks.upstream {
        Ok(Upstream::Tag { repo, .. }) if !repo.is_empty() => Some(repo.clone()),
        _ => None,
    };
    View {
        installed: installed.filter(|_| !shown),
        chips,
        repo,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tag(t: &str, source: Source) -> Result<Upstream, Failure> {
        Ok(Upstream::Tag {
            tag: t.into(),
            source,
            repo: String::new(),
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
    fn a_source_without_a_version_shows_a_question_mark() {
        let c = Checks {
            upstream: Ok(Upstream::NoRelease(Source::GitHub)),
            ego: Ok(Some(13)),
        };
        let v = view(&c, None, Some(13));
        assert_eq!(v.chips.len(), 2);
        assert!(v.chips[0].missing && v.chips[0].version == "?");
        assert_eq!(v.chips[0].sources[0].0, Source::GitHub);
        assert!(v.chips[0].tip.contains("no releases"));
        assert_eq!(
            (v.chips[1].version.as_str(), v.chips[1].state),
            ("13", Version::Same)
        );
        // The "?" is not the installed version, so that shows by itself only if no chip has it.
        assert_eq!(v.installed, None);
    }

    #[test]
    fn no_repository_is_the_globe() {
        let c = Checks {
            upstream: Ok(Upstream::NoRepo(None)),
            ego: Ok(None),
        };
        let v = view(&c, None, Some(1));
        assert_eq!(v.chips.len(), 1);
        assert_eq!(v.chips[0].sources[0].0, Source::Web);
        assert_eq!(v.installed.as_deref(), Some("1"));
    }

    #[test]
    fn a_limited_upstream_says_which_host_and_leaves_the_portal_chip() {
        let upstream = Err(Failure {
            error: Error::RateLimited("gitlab.com".into()),
            source: Source::GitLab,
        });
        let c = Checks {
            upstream,
            ego: Ok(Some(13)),
        };
        assert_eq!(c.rate_limited(), Some("gitlab.com"));
        let v = view(&c, None, Some(13));
        assert_eq!(v.chips.len(), 2);
        assert_eq!(v.chips[0].sources[0].0, Source::GitLab);
        assert!(v.chips[0].tip.contains("gitlab.com says too many requests"));
    }
}
