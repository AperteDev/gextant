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
            .http_status_as_error(false)
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
    /// github.com has no such repository for us: it does not exist, or it is private.
    Missing,
    Other(String),
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Other(e.to_string())
    }
}

type Response = ureq::http::Response<ureq::Body>;

/// The body of an answer; `None` for 404.
fn read(mut r: Response) -> Result<Option<String>, Error> {
    let status = r.status().as_u16();
    let limited = r
        .headers()
        .get("x-ratelimit-remaining")
        .is_some_and(|v| v == "0")
        || r.headers().contains_key("retry-after");
    let body = r
        .body_mut()
        .read_to_string()
        .map_err(|e| Error::Other(e.to_string()))?;
    match status {
        200..=299 => Ok(Some(body)),
        404 => Ok(None),
        _ => Err(failure(status, limited, &body)),
    }
}

/// What a failed answer means. A 403 is a rate limit only when GitHub says so (its headers, or
/// its message): it also answers 403 to a token that may not see a repository, and to a burst
/// of requests (the secondary limit, which says so in its message).
fn failure(status: u16, limited: bool, body: &str) -> Error {
    if status == 401 {
        return Error::Other("GitHub refused the access token".into());
    }
    if status == 429 || (status == 403 && (limited || body.to_lowercase().contains("rate limit"))) {
        return Error::RateLimited;
    }
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned));
    Error::Other(message.unwrap_or_else(|| format!("GitHub answered HTTP {status}")))
}

/// The latest release of `repo`, from github.com's pages, which cost no API quota. Only when
/// they fail is the API asked, if there is a `token`: when the pages are limited, or do not
/// know the repository (a private one). `uuid` asks whether the release ships that extension.
pub fn latest(
    repo: &Repo,
    token: Option<&str>,
    uuid: Option<&str>,
) -> Result<Option<Release>, Error> {
    match (latest_web(repo, uuid), token) {
        (Err(Error::RateLimited | Error::Missing), Some(token)) => latest_api(repo, token, uuid),
        (r, _) => r,
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
            .call()?;
        read(r)?
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
    let r = AGENT.head(format!("{base}/releases/latest")).call()?;
    let tag = match r.status().as_u16() {
        200 => r
            .get_uri()
            .path()
            .rsplit_once("/releases/tag/")
            .map(|(_, t)| t.to_owned()),
        404 => return Err(Error::Missing),
        403 | 429 => return Err(Error::RateLimited),
        s => return Err(Error::Other(format!("GitHub answered HTTP {s}"))),
    };
    let Some(tag) = tag else {
        let feed = read(AGENT.get(format!("{base}/tags.atom")).call()?)?;
        let tag = feed.as_deref().and_then(first_tag);
        return Ok(tag.map(|tag| Release { tag, ships: false }));
    };
    let ships = match uuid {
        // The assets are listed on a page of their own, with `@` percent-encoded.
        Some(u) => read(
            AGENT
                .get(format!("{base}/releases/expanded_assets/{tag}"))
                .call()?,
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
        Err(Error::Missing) => Outcome::NoRepo,
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
    fn a_403_is_a_rate_limit_only_when_github_says_so() {
        let msg = |e: Error| match e {
            Error::Other(m) => m,
            _ => "limited".into(),
        };
        let denied = r#"{"message":"Resource not accessible by personal access token"}"#;
        assert_eq!(
            msg(failure(403, false, denied)),
            "Resource not accessible by personal access token"
        );
        assert_eq!(msg(failure(403, true, denied)), "limited");
        assert_eq!(
            msg(failure(
                403,
                false,
                r#"{"message":"You have exceeded a secondary rate limit."}"#
            )),
            "limited"
        );
        assert_eq!(msg(failure(429, false, "")), "limited");
        assert_eq!(
            msg(failure(401, false, "")),
            "GitHub refused the access token"
        );
        assert_eq!(msg(failure(500, false, "")), "GitHub answered HTTP 500");
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
