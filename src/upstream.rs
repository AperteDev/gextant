//! An extension's upstream: the repository its `url` names (GitHub or GitLab), its latest
//! release, and whether that is newer than what is installed.

use crate::token::Tokens;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::LazyLock;
use std::time::Duration;
use ureq::ResponseExt;
use ureq::tls::{RootCerts, TlsConfig};

pub(crate) static AGENT: LazyLock<ureq::Agent> = LazyLock::new(|| {
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
    /// `github.com`, `gitlab.com`, or another host named `gitlab.…`.
    pub host: String,
    /// `owner/name` (on GitLab, with any subgroups in between).
    pub path: String,
}

/// The repository `url` points at, and whether that is only a guess: a project page on
/// `<owner>.github.io/<name>` is taken for `github.com/<owner>/<name>` (and `.gitlab.io` for
/// gitlab.com).
pub fn repo_from_url(url: &str) -> Option<(Repo, bool)> {
    let rest = url.split_once("://")?.1.split(['?', '#']).next()?;
    let mut parts = rest.split('/').filter(|s| !s.is_empty());
    let host = parts.next()?.to_ascii_lowercase();
    let segs: Vec<&str> = parts.collect();
    let repo = |host: &str, segs: &[&str]| {
        let (name, owners) = segs.split_last().filter(|_| segs.len() >= 2)?;
        let name = name.strip_suffix(".git").unwrap_or(name);
        Some(Repo {
            host: host.to_owned(),
            path: [owners, &[name]].concat().join("/"),
        })
    };
    match host.as_str() {
        "github.com" | "www.github.com" => {
            Some((repo("github.com", &segs[..segs.len().min(2)])?, false))
        }
        h if h == "gitlab.com" || h.starts_with("gitlab.") => {
            // A GitLab project may sit in subgroups; the page of one ends at `/-/` or a view.
            let views = [
                "-",
                "tree",
                "blob",
                "issues",
                "wikis",
                "releases",
                "tags",
                "merge_requests",
            ];
            let end = segs
                .iter()
                .position(|s| views.contains(s))
                .unwrap_or(segs.len());
            Some((repo(h, &segs[..end])?, false))
        }
        h => {
            let (owner, forge) = match (h.strip_suffix(".github.io"), h.strip_suffix(".gitlab.io"))
            {
                (Some(o), _) => (o, "github.com"),
                (_, Some(o)) => (o, "gitlab.com"),
                _ => return None,
            };
            Some((repo(forge, &[owner, segs.first()?])?, true))
        }
    }
}

/// The repositories a project page links to (the first three): where a `url` names a website
/// instead of a repository, the website says where the code is.
fn discover(url: &str) -> Vec<Repo> {
    if !url.starts_with("https://") {
        return Vec::new();
    }
    let Ok(mut r) = AGENT.get(url).call() else {
        return Vec::new();
    };
    if r.status() != 200 {
        return Vec::new();
    }
    let html = r
        .body_mut()
        .with_config()
        .limit(1 << 20)
        .read_to_string()
        .unwrap_or_default();
    let mut found = Vec::new();
    for href in html.split("href=").skip(1) {
        let Some(quote) = href.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let link = href[1..].split(quote).next().unwrap_or_default();
        if let Some((repo, false)) = repo_from_url(link)
            && !found.contains(&repo)
        {
            found.push(repo);
        }
    }
    found.truncate(3);
    found
}

pub(crate) struct Release {
    pub tag: String,
    /// The release ships a zip named after the extension's UUID (only looked for when asked).
    pub ships: bool,
}

pub enum Error {
    /// The host (`github.com`, `gitlab.com`…) says too many requests were made.
    RateLimited(String),
    /// The forge has no such repository for us: it does not exist, or it is private.
    Missing,
    Other(String),
}

impl From<ureq::Error> for Error {
    fn from(e: ureq::Error) -> Self {
        Error::Other(e.to_string())
    }
}

type Response = ureq::http::Response<ureq::Body>;

/// The host that answered.
pub(crate) fn host_of(r: &Response) -> String {
    r.get_uri().host().unwrap_or("The server").to_owned()
}

/// The body of an answer; `None` for 404.
pub(crate) fn read(mut r: Response) -> Result<Option<String>, Error> {
    let status = r.status().as_u16();
    let host = host_of(&r);
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
        _ => Err(failure(status, limited, &body, host)),
    }
}

/// What a failed answer means. A 403 is a rate limit only when GitHub says so (its headers, or
/// its message): it also answers 403 to a token that may not see a repository, and to a burst
/// of requests (the secondary limit, which says so in its message).
fn failure(status: u16, limited: bool, body: &str, host: String) -> Error {
    if status == 401 {
        return Error::Other("The server refused the access token".into());
    }
    if status == 429 || (status == 403 && (limited || body.to_lowercase().contains("rate limit"))) {
        return Error::RateLimited(host);
    }
    let message = serde_json::from_str::<Value>(body)
        .ok()
        .and_then(|v| v["message"].as_str().map(str::to_owned));
    Error::Other(message.unwrap_or_else(|| format!("The server answered HTTP {status}")))
}

/// The latest release of `repo`. GitHub: from github.com's pages, which cost no API quota;
/// only when they fail is the API asked, if there is a `token`: when the pages are limited, or
/// do not know the repository (a private one). GitLab: see `gitlab`. `uuid` asks whether the
/// release ships that extension.
pub fn latest(repo: &Repo, tokens: &Tokens, uuid: Option<&str>) -> Result<Option<Release>, Error> {
    let token = tokens.for_host(&repo.host);
    if repo.host != "github.com" {
        return crate::gitlab::latest(repo, token, uuid);
    }
    match (latest_web(repo, uuid), token) {
        (Err(Error::RateLimited(_) | Error::Missing), Some(token)) => latest_api(repo, token, uuid),
        (r, _) => r,
    }
}

/// Is one of the `files` a zip named after the extension: its UUID, or what is before the `@`
/// (`gnome-extensions pack` makes `<uuid>.shell-extension.zip`; others ship `<name>.zip`)?
/// Only a sanity check; the zip's own `metadata.json` is what counts, when installing.
pub(crate) fn ships<'a>(files: impl IntoIterator<Item = &'a str>, uuid: &str) -> bool {
    let name = uuid.split('@').next().unwrap_or(uuid).to_lowercase();
    files.into_iter().any(|f| {
        let f = f.to_lowercase();
        f.ends_with(".zip") && f.starts_with(&name)
    })
}

/// Does GitHub refuse the token (expired, revoked, mistyped)? `/rate_limit` answers 401 to it
/// and costs no quota. Any other answer, or none, is not a refusal.
pub fn token_refused(token: &str) -> bool {
    AGENT
        .get("https://api.github.com/rate_limit")
        .header("Authorization", format!("Bearer {token}"))
        .call()
        .is_ok_and(|r| r.status() == 401)
}

/// The scheme and host of `url`: `https://api.github.com`.
fn origin(url: &str) -> &str {
    let start = url.find("://").map_or(0, |i| i + 3);
    url[start..].find('/').map_or(url, |i| &url[..start + i])
}

/// GET from an API with a `token` header (`name`: `value`), `None` for 404. A moved repository
/// answers 301 to its numeric URL: followed here, on the same host only, as the HTTP client
/// drops the token on a redirect (and the anonymous request that follows is limited a lot more).
pub(crate) fn api_get(mut url: String, header: (&str, String)) -> Result<Option<Value>, Error> {
    for _ in 0..3 {
        let r = AGENT
            .get(&url)
            .config()
            .max_redirects(0)
            .max_redirects_will_error(false)
            .build()
            .header("Accept", "application/json")
            .header(header.0, &header.1)
            .call()?;
        let to = r.headers().get("location").and_then(|l| l.to_str().ok());
        if let (301 | 302 | 307 | 308, Some(to)) = (r.status().as_u16(), to)
            && to.starts_with(origin(&url))
        {
            url = to.to_owned();
            continue;
        }
        return read(r)?
            .map(|t| serde_json::from_str(&t).map_err(|e| Error::Other(e.to_string())))
            .transpose();
    }
    Err(Error::Other("Too many redirects".into()))
}

/// The API's answer: the latest release; a 404 is none (or no such repository). Only releases
/// count: a bare tag is a snapshot, not something the author published.
fn latest_api(repo: &Repo, token: &str, uuid: Option<&str>) -> Result<Option<Release>, Error> {
    let path = &repo.path;
    let api = |what: &str| {
        let url = format!("https://api.github.com/repos/{path}/{what}");
        api_get(url, ("Authorization", format!("Bearer {token}")))
    };
    if let Some(r) = api("releases/latest")? {
        let assets = r["assets"].as_array().into_iter().flatten();
        let ships = uuid.is_some_and(|u| ships(assets.filter_map(|a| a["name"].as_str()), u));
        let tag = r["tag_name"].as_str().unwrap_or_default().to_owned();
        return Ok(Some(Release { tag, ships }));
    }
    Ok(None)
}

/// `/releases/latest` redirects to the release's page, `/releases/tag/<tag>` (to
/// `/releases` when there is none: then there is nothing to check).
fn latest_web(repo: &Repo, uuid: Option<&str>) -> Result<Option<Release>, Error> {
    let base = format!("https://github.com/{}", repo.path);
    let r = AGENT.head(format!("{base}/releases/latest")).call()?;
    let tag = match r.status().as_u16() {
        200 => r
            .get_uri()
            .path()
            .rsplit_once("/releases/tag/")
            .map(|(_, t)| t.to_owned()),
        404 => return Err(Error::Missing),
        403 | 429 => return Err(Error::RateLimited(host_of(&r))),
        s => return Err(Error::Other(format!("The server answered HTTP {s}"))),
    };
    let Some(tag) = tag else {
        return Ok(None);
    };
    let ships = match uuid {
        // The assets are listed on a page of their own.
        Some(u) => read(
            AGENT
                .get(format!("{base}/releases/expanded_assets/{tag}"))
                .call()?,
        )?
        .is_some_and(|html| {
            let name = u.split('@').next().unwrap_or(u);
            let needle = format!("/releases/download/{tag}/{name}");
            html.to_lowercase().contains(&needle.to_lowercase())
        }),
        None => false,
    };
    Ok(Some(Release { tag, ships }))
}

#[derive(Clone, Copy, Debug, PartialEq)]
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

/// The number EGO gave a release, when its tag carries it as the last number after a `v` or
/// `+`, or is just that number: `v66`, `extensions.gnome.org-v110` (Dash to Dock), `2.2.0+10`
/// (Solaar), `66`. Not `v1.2.0`.
fn ego_number(tag: &str) -> Option<u64> {
    let head = tag.trim_end_matches(|c: char| c.is_ascii_digit());
    (head.is_empty() || head.ends_with(['v', 'V', '+'])).then(|| tag[head.len()..].parse().ok())?
}

/// Is release `tag` newer than what is installed? Against a `version-name`, as numbers (fewer
/// count as zeros). Without one, EGO's counter against the number in the tag, if it has one.
/// ponytail: a guess; an author numbering independently can show a false update or none;
/// "mark as installed" (roadmap) is the certain way.
pub fn compare(version_name: Option<&str>, counter: Option<u64>, tag: &str) -> Version {
    if let Some(mut have) = version_name.and_then(numbers) {
        let Some(mut latest) = numbers(tag) else {
            return Version::Unknown;
        };
        let len = have.len().max(latest.len());
        have.resize(len, 0);
        latest.resize(len, 0);
        return if latest > have {
            Version::Newer
        } else {
            Version::Same
        };
    }
    match (counter, ego_number(tag)) {
        (Some(have), Some(latest)) if latest > have => Version::Newer,
        (Some(_), Some(_)) => Version::Same,
        _ => Version::Unknown,
    }
}

/// Where an extension's code is looked for: what the row's icon shows.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Source {
    GitHub,
    GitLab,
    /// A website, where no repository was found behind it.
    Web,
    /// extensions.gnome.org, where the upstream said nothing.
    Ego,
}

impl Source {
    pub fn icon(self) -> &'static str {
        match self {
            Source::GitHub => "gextant-github-symbolic",
            Source::GitLab => "gextant-gitlab-symbolic",
            Source::Web => "globe-symbolic",
            Source::Ego => "gextant-gnome-symbolic",
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Source::GitHub => "GitHub",
            Source::GitLab => "GitLab",
            Source::Web => "Website",
            Source::Ego => "GNOME Extensions",
        }
    }

    pub(crate) fn of(repo: &Repo) -> Self {
        if repo.host == "github.com" {
            Source::GitHub
        } else {
            Source::GitLab
        }
    }
}

/// What the upstream (a repository) has: the part worth keeping between runs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Upstream {
    /// The tag of the latest release.
    Tag {
        tag: String,
        source: Source,
        /// `host/path` of the repository, to install from. Empty in what an older run kept.
        #[serde(default)]
        repo: String,
    },
    NoRepo(Option<Source>),
    NoRelease(Source),
}

/// A failed look at an upstream, and which source it was.
pub struct Failure {
    pub error: Error,
    pub source: Source,
}

/// Look for the upstream of extension `uuid`, named by its metadata `url`, and its latest release.
pub fn find(uuid: &str, url: Option<&str>, tokens: &Tokens) -> Result<Upstream, Failure> {
    let Some(url) = url else {
        return Ok(Upstream::NoRepo(None));
    };
    // A `url` that is not a repository may be a project page that links to it. Such a guess
    // counts only if the release ships this extension.
    let candidates = match repo_from_url(url) {
        Some(found) => vec![found],
        None => discover(url).into_iter().map(|r| (r, true)).collect(),
    };
    let mut result = Ok(Upstream::NoRepo(Some(Source::Web)));
    for (repo, guessed) in candidates {
        match latest(&repo, tokens, guessed.then_some(uuid)) {
            Err(error @ Error::RateLimited(_)) => {
                return Err(Failure {
                    error,
                    source: Source::of(&repo),
                });
            }
            Err(Error::Missing) => {}
            Err(error) => {
                result = Err(Failure {
                    error,
                    source: Source::of(&repo),
                })
            }
            Ok(None) if !guessed => result = Ok(Upstream::NoRelease(Source::of(&repo))),
            Ok(Some(r)) if !guessed || r.ships => {
                return Ok(Upstream::Tag {
                    tag: r.tag,
                    source: Source::of(&repo),
                    repo: format!("{}/{}", repo.host, repo.path),
                });
            }
            Ok(_) => {}
        }
    }
    result
}

/// A tag as the version it is: without a leading `v`; and, for an extension that has only EGO's
/// counter, the number the tag carries (`extensions.gnome.org-v110` is `110`).
pub fn display_version(tag: &str, version_name: Option<&str>, counter: Option<u64>) -> String {
    match ego_number(tag) {
        Some(n) if version_name.is_none() && counter.is_some() => n.to_string(),
        _ => tag.strip_prefix(['v', 'V']).unwrap_or(tag).to_owned(),
    }
}

/// EGO's `latest` version against the installed counter.
pub fn compare_counter(counter: Option<u64>, latest: u64) -> Version {
    match counter {
        Some(have) if latest > have => Version::Newer,
        Some(_) => Version::Same,
        None => Version::Unknown,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn repo(url: &str) -> Option<(String, String, bool)> {
        repo_from_url(url).map(|(r, g)| (r.host, r.path, g))
    }

    #[test]
    fn urls_reduce_to_a_repository() {
        let gh = |o: &str, n: &str, g| Some(("github.com".to_owned(), format!("{o}/{n}"), g));
        let gl = |h: &str, p: &str, g| Some((h.to_owned(), p.to_owned(), g));
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
        assert_eq!(
            repo("https://gitlab.com/rmnvgr/nightthemeswitcher-gnome-shell-extension"),
            gl(
                "gitlab.com",
                "rmnvgr/nightthemeswitcher-gnome-shell-extension",
                false
            )
        );
        assert_eq!(
            repo("https://gitlab.gnome.org/World/Shell/ext.git/-/issues/3"),
            gl("gitlab.gnome.org", "World/Shell/ext", false)
        );
        assert_eq!(
            repo("https://me.gitlab.io/ext/"),
            gl("gitlab.com", "me/ext", true)
        );
        assert_eq!(repo("https://gitlab.com/rmnvgr"), None);
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
            msg(failure(403, false, denied, "github.com".into())),
            "Resource not accessible by personal access token"
        );
        assert_eq!(
            msg(failure(403, true, denied, "github.com".into())),
            "limited"
        );
        assert_eq!(
            msg(failure(
                403,
                false,
                r#"{"message":"You have exceeded a secondary rate limit."}"#,
                "github.com".into()
            )),
            "limited"
        );
        assert_eq!(msg(failure(429, false, "", "gitlab.com".into())), "limited");
        assert_eq!(
            msg(failure(401, false, "", "github.com".into())),
            "The server refused the access token"
        );
        assert_eq!(
            msg(failure(500, false, "", "github.com".into())),
            "The server answered HTTP 500"
        );
    }

    #[test]
    fn the_origin_of_a_url() {
        assert_eq!(
            origin("https://api.github.com/repos/a/b"),
            "https://api.github.com"
        );
        assert_eq!(origin("https://gitlab.com"), "https://gitlab.com");
    }

    #[test]
    fn a_zip_named_after_the_extension() {
        let uuid = "nightthemeswitcher@romainvigier.fr";
        assert!(ships(["nightthemeswitcher.zip"], uuid));
        assert!(ships(
            [format!("{uuid}.shell-extension.zip").as_str()],
            uuid
        ));
        assert!(!ships(["nightthemeswitcher.tar.gz", "other.zip"], uuid));
    }

    #[test]
    fn the_portal_is_compared_by_counter() {
        assert_eq!(compare_counter(Some(13), 13), Version::Same);
        assert_eq!(compare_counter(Some(12), 13), Version::Newer);
        assert_eq!(compare_counter(Some(14), 13), Version::Same);
        assert_eq!(compare_counter(None, 13), Version::Unknown);
    }

    #[test]
    fn tags_show_as_versions() {
        let d = display_version;
        assert_eq!(d("v2.5.0", Some("2.5.0"), None), "2.5.0");
        assert_eq!(d("extensions.gnome.org-v110", None, Some(110)), "110");
        assert_eq!(d("2.2.0+10", None, Some(9)), "10");
        assert_eq!(d("v1.2.0", None, Some(9)), "1.2.0");
    }

    #[test]
    fn versions_compare() {
        let c = |v, tag| compare(v, None, tag);
        assert_eq!(compare(None, Some(66), "v66"), Version::Same);
        assert_eq!(compare(None, Some(66), "v67"), Version::Newer);
        assert_eq!(compare(None, Some(25), "12"), Version::Same);
        assert_eq!(compare(None, Some(66), "v1.2.0"), Version::Unknown);
        assert_eq!(
            compare(None, Some(110), "extensions.gnome.org-v110"),
            Version::Same
        );
        assert_eq!(
            compare(None, Some(111), "extensions.gnome.org-v110"),
            Version::Same
        );
        assert_eq!(compare(None, Some(9), "2.2.0+10"), Version::Newer);
        assert_eq!(compare(None, Some(10), "2.2.0+10"), Version::Same);
        assert_eq!(c(Some("2.5.0"), "v2.6.0"), Version::Newer);
        assert_eq!(c(Some("2.5.0"), "2.5"), Version::Same);
        assert_eq!(c(Some("2.5.1"), "v2.5.0"), Version::Same);
        assert_eq!(c(Some("2.10"), "v2.9"), Version::Same);
        assert_eq!(c(Some("2.1.2-17+17d5fd1-dirty"), "v2.1.3"), Version::Newer);
        assert_eq!(c(None, "v2.5.0"), Version::Unknown);
        assert_eq!(c(Some("2.5.0"), "release"), Version::Unknown);
    }
}
