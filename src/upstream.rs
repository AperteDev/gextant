//! An extension's upstream: the repository its `url` names (GitHub only for now), its latest
//! release, and whether that is newer than what is installed.

use serde_json::Value;
use std::sync::LazyLock;
use std::time::Duration;
use ureq::ResponseExt;
use ureq::tls::{RootCerts, TlsConfig};

static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
    ureq::Agent::new_with_config(
        ureq::Agent::config_builder()
            .user_agent(concat!("gextant/", env!("CARGO_PKG_VERSION")))
            .timeout_global(Some(Duration::from_secs(20)))
            .tls_config(
                TlsConfig::builder()
                    .root_certs(RootCerts::PlatformVerifier)
                    .build(),
            )
            .build(),
    )
});

#[derive(Debug, PartialEq)]
pub struct Repo {
    pub owner: String,
    pub name: String,
}

/// The repository `url` points at, and whether that is only a guess: a project page on
/// `<owner>.github.io/<name>` is taken for `github.com/<owner>/<name>`.
pub fn repo_from_url(url: &str) -> Option<(Repo, bool)> {
    let rest = url.split_once("://")?.1;
    let rest = rest.split(['?', '#']).next()?;
    let mut parts = rest.split('/').filter(|s| !s.is_empty());
    let host = parts.next()?.to_ascii_lowercase();
    let first = parts.next()?;
    let repo = |owner: &str, name: &str| Repo {
        owner: owner.to_owned(),
        name: name.strip_suffix(".git").unwrap_or(name).to_owned(),
    };
    if host == "github.com" || host == "www.github.com" {
        Some((repo(first, parts.next()?), false))
    } else {
        let owner = host.strip_suffix(".github.io")?;
        Some((repo(owner, first), true))
    }
}

pub struct Release {
    pub tag: String,
    /// The release ships a zip named after the extension's UUID (only looked for when asked).
    pub ships: bool,
}

pub enum Error {
    /// GitHub's limit on requests was used up.
    RateLimited,
    Other(String),
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        match e {
            ureq::Error::StatusCode(403 | 429) => Error::RateLimited,
            ureq::Error::StatusCode(401) => Error::Other("GitHub refused the access token".into()),
            e => Error::Other(e.to_string()),
        }
    }
}

type Response = Result<ureq::http::Response<ureq::Body>, ureq::Error>;

/// The body of a response; `None` for 404.
fn text(r: Response) -> Result<Option<String>, Error> {
    match r {
        Ok(mut r) => r
            .body_mut()
            .read_to_string()
            .map(Some)
            .map_err(|e| Error::Other(e.to_string())),
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(e.into()),
    }
}

/// The latest release of `repo`. With a `token`, from the API (5,000 requests an hour, private
/// repositories too); without, from github.com's pages, which the API's 60 an hour do not
/// cover. `uuid` asks whether the release ships that extension.
pub fn latest(
    repo: &Repo,
    token: Option<&str>,
    uuid: Option<&str>,
) -> Result<Option<Release>, Error> {
    match token {
        Some(token) => latest_api(repo, token, uuid),
        None => latest_web(repo, uuid),
    }
}

/// ponytail: GitHub lists tags by name, not by date; a repo whose newest tag sorts low is
/// misread; pick by version number if it shows up.
fn latest_api(repo: &Repo, token: &str, uuid: Option<&str>) -> Result<Option<Release>, Error> {
    let Repo { owner, name } = repo;
    let api = |path: &str| -> Result<Option<Value>, Error> {
        let r = AGENT
            .get(format!(
                "https://api.github.com/repos/{owner}/{name}/{path}"
            ))
            .header("Accept", "application/vnd.github+json")
            .header("Authorization", format!("Bearer {token}"))
            .call();
        text(r)?
            .map(|t| serde_json::from_str(&t).map_err(|e| Error::Other(e.to_string())))
            .transpose()
    };
    if let Some(r) = api("releases/latest")? {
        let mut assets = r["assets"].as_array().into_iter().flatten();
        let ships = uuid.is_some_and(|u| {
            assets.any(|a| {
                a["name"]
                    .as_str()
                    .is_some_and(|n| n.starts_with(u) && n.ends_with(".zip"))
            })
        });
        let tag = r["tag_name"].as_str().unwrap_or_default().to_owned();
        return Ok(Some(Release { tag, ships }));
    }
    let tags = api("tags?per_page=1")?;
    let tag = tags.as_ref().and_then(|t| t[0]["name"].as_str());
    Ok(tag.map(|tag| Release {
        tag: tag.to_owned(),
        ships: false,
    }))
}

/// `/releases/latest` redirects to the release's page, `/releases/tag/<tag>` (to
/// `/releases` when there is none: then the tags feed names the newest tag).
fn latest_web(repo: &Repo, uuid: Option<&str>) -> Result<Option<Release>, Error> {
    let base = format!("https://github.com/{}/{}", repo.owner, repo.name);
    let tag = match AGENT.head(format!("{base}/releases/latest")).call() {
        Ok(r) => r
            .get_uri()
            .path()
            .rsplit_once("/releases/tag/")
            .map(|(_, t)| t.to_owned()),
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let Some(tag) = tag else {
        let feed = text(AGENT.get(format!("{base}/tags.atom")).call())?;
        let tag = feed.as_deref().and_then(first_tag);
        return Ok(tag.map(|tag| Release { tag, ships: false }));
    };
    let ships = match uuid {
        // The assets are listed on a page of their own, with `@` percent-encoded.
        Some(u) => text(
            AGENT
                .get(format!("{base}/releases/expanded_assets/{tag}"))
                .call(),
        )?
        .is_some_and(|html| {
            let dir = format!("/releases/download/{tag}/");
            [u.to_owned(), u.replace('@', "%40")]
                .iter()
                .any(|u| html.contains(&format!("{dir}{u}")))
        }),
        None => false,
    };
    Ok(Some(Release { tag, ships }))
}

/// The newest tag of a repository's `tags.atom`: the title of its first entry.
fn first_tag(feed: &str) -> Option<String> {
    let entry = &feed[feed.find("<entry>")?..];
    let title = &entry[entry.find("<title>")? + 7..];
    Some(title[..title.find("</title>")?].trim().to_owned())
}

#[derive(Debug, PartialEq)]
pub enum Version {
    Same,
    Newer,
    /// Not comparable: the installed version is only EGO's counter, or a tag isn't a number.
    Unknown,
}

/// The leading `1.2.3` of a version (after a `v`): `v2.1.2-17+abc` is `[2, 1, 2]`.
fn numbers(v: &str) -> Option<Vec<u64>> {
    let v = v.strip_prefix(['v', 'V']).unwrap_or(v);
    let end = v
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .unwrap_or(v.len());
    v[..end]
        .trim_end_matches('.')
        .split('.')
        .map(|n| n.parse().ok())
        .collect()
}

/// Is release `tag` newer than what is installed? Fewer numbers count as zeros. Without a
/// `version-name`, a tag that is a single number (`v66`) is compared with EGO's counter: many
/// authors tag the number EGO gave. ponytail: a guess; an author numbering independently can
/// show a false update or none; "mark as installed" (roadmap) is the certain way.
pub fn compare(version_name: Option<&str>, counter: Option<u64>, tag: &str) -> Version {
    let Some(mut latest) = numbers(tag) else {
        return Version::Unknown;
    };
    let mut have = match (version_name.and_then(numbers), counter) {
        (Some(have), _) => have,
        (None, Some(c)) if latest.len() == 1 => vec![c],
        _ => return Version::Unknown,
    };
    let len = have.len().max(latest.len());
    have.resize(len, 0);
    latest.resize(len, 0);
    if latest > have {
        Version::Newer
    } else {
        Version::Same
    }
}

pub enum Outcome {
    /// `latest`: the release's tag.
    Checked {
        latest: String,
        version: Version,
    },
    NoRepo,
    NoRelease,
    RateLimited,
    Failed(String),
}

pub fn check(
    uuid: &str,
    version_name: Option<&str>,
    counter: Option<u64>,
    url: Option<&str>,
    token: Option<&str>,
) -> Outcome {
    let Some((repo, guessed)) = url.and_then(repo_from_url) else {
        return Outcome::NoRepo;
    };
    // A guess counts only if the release ships this extension: a zip named after its UUID
    // (`gnome-extensions pack` makes `<uuid>.shell-extension.zip`).
    match latest(&repo, token, guessed.then_some(uuid)) {
        Err(Error::RateLimited) => Outcome::RateLimited,
        Err(Error::Other(e)) => Outcome::Failed(e),
        Ok(None) => Outcome::NoRelease,
        Ok(Some(r)) if guessed && !r.ships => Outcome::NoRepo,
        Ok(Some(r)) => Outcome::Checked {
            version: compare(version_name, counter, &r.tag),
            latest: r.tag,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(url: &str) -> Option<(String, String, bool)> {
        repo_from_url(url).map(|(r, g)| (r.owner, r.name, g))
    }

    #[test]
    fn urls_reduce_to_a_repository() {
        let gh = |o: &str, n: &str, g| Some((o.to_owned(), n.to_owned(), g));
        assert_eq!(
            repo("https://github.com/aunetx/blur-my-shell"),
            gh("aunetx", "blur-my-shell", false)
        );
        assert_eq!(
            repo("https://github.com/CraigFoote/x.git"),
            gh("CraigFoote", "x", false)
        );
        assert_eq!(
            repo("https://github.com/GSConnect/gnome-shell-extension-gsconnect/wiki"),
            gh("GSConnect", "gnome-shell-extension-gsconnect", false)
        );
        assert_eq!(
            repo("https://micheleg.github.io/dash-to-dock/"),
            gh("micheleg", "dash-to-dock", true)
        );
        assert_eq!(repo("https://nightthemeswitcher.romainvigier.fr"), None);
        assert_eq!(repo("https://github.com/aunetx"), None);
        assert_eq!(repo("not a url"), None);
    }

    #[test]
    fn first_tag_of_a_feed() {
        let feed = "<feed><title>Tags</title><entry><id>x</id><title>v1.2</title></entry><entry><title>v1.1</title></entry></feed>";
        assert_eq!(first_tag(feed).as_deref(), Some("v1.2"));
        assert_eq!(first_tag("<feed><title>Tags</title></feed>"), None);
    }

    #[test]
    fn versions_compare() {
        let c = |v, tag| compare(v, None, tag);
        assert_eq!(compare(None, Some(66), "v66"), Version::Same);
        assert_eq!(compare(None, Some(66), "v67"), Version::Newer);
        assert_eq!(compare(None, Some(25), "12"), Version::Same);
        assert_eq!(compare(None, Some(66), "v1.2.0"), Version::Unknown);
        assert_eq!(c(Some("2.5.0"), "v2.6.0"), Version::Newer);
        assert_eq!(c(Some("2.5.0"), "2.5"), Version::Same);
        assert_eq!(c(Some("2.5.1"), "v2.5.0"), Version::Same);
        assert_eq!(c(Some("2.10"), "v2.9"), Version::Same);
        assert_eq!(c(Some("2.1.2-17+17d5fd1-dirty"), "v2.1.3"), Version::Newer);
        assert_eq!(c(None, "v2.5.0"), Version::Unknown);
        assert_eq!(c(Some("2.5.0"), "release"), Version::Unknown);
    }
}
