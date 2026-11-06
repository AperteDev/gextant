//! GitLab (gitlab.com, and hosts named `gitlab.…`): the latest release from the project's pages,
//! as on GitHub; the API only to see the files of a release, when asked, and as the fallback
//! with a token (gitlab.com's only) when the pages are limited or do not know the project.

use crate::upstream::{AGENT, Error, Release, Repo, api_get, host_of, read, ships};
use serde_json::Value;
use ureq::ResponseExt;

pub fn latest(
    repo: &Repo,
    token: Option<&str>,
    uuid: Option<&str>,
) -> Result<Option<Release>, Error> {
    match (latest_web(repo, uuid), token) {
        (Err(Error::RateLimited(_) | Error::Missing), Some(token)) => latest_api(repo, token, uuid),
        (r, _) => r,
    }
}

/// Does GitLab refuse the token (expired, revoked, mistyped)? `/personal_access_tokens/self`
/// answers 401 to it. Any other answer, or none, is not a refusal. gitlab.com only.
pub fn token_refused(token: &str) -> bool {
    AGENT
        .get("https://gitlab.com/api/v4/personal_access_tokens/self")
        .header("PRIVATE-TOKEN", token)
        .call()
        .is_ok_and(|r| r.status() == 401)
}

/// Do the `files` of a release (the file name at the end of each link's URL) include the
/// extension?
fn ships_in(release: &Value, uuid: &str) -> bool {
    let links = release["assets"]["links"].as_array().into_iter().flatten();
    ships(
        links.filter_map(|l| l["url"].as_str()?.rsplit('/').next()),
        uuid,
    )
}

/// The API's answer: the newest release (only releases count, not bare tags).
fn latest_api(repo: &Repo, token: &str, uuid: Option<&str>) -> Result<Option<Release>, Error> {
    let base = format!(
        "https://{}/api/v4/projects/{}",
        repo.host,
        repo.path.replace('/', "%2F")
    );
    let api = |what: &str| {
        api_get(
            format!("{base}/{what}"),
            ("PRIVATE-TOKEN", token.to_owned()),
        )
    };
    if let Some(release) = api("releases?per_page=1")?.and_then(|r| r.get(0).cloned()) {
        let tag = release["tag_name"].as_str().unwrap_or_default().to_owned();
        let ships = uuid.is_some_and(|u| ships_in(&release, u));
        return Ok(Some(Release { tag, ships }));
    }
    Ok(None)
}

/// `/-/releases/permalink/latest` redirects to the release's page, `/-/releases/<tag>`; without
/// releases it is a 404: none to check, if the project exists (its tags feed tells).
fn latest_web(repo: &Repo, uuid: Option<&str>) -> Result<Option<Release>, Error> {
    let (host, path) = (&repo.host, &repo.path);
    let base = format!("https://{host}/{path}");
    let r = AGENT
        .head(format!("{base}/-/releases/permalink/latest"))
        .call()?;
    let tag = match r.status().as_u16() {
        200 => r
            .get_uri()
            .path()
            .rsplit_once("/-/releases/")
            .map(|(_, t)| t.to_owned()),
        404 => None,
        403 | 429 => return Err(Error::RateLimited(host_of(&r))),
        s => return Err(Error::Other(format!("The server answered HTTP {s}"))),
    };
    let Some(tag) = tag else {
        read(AGENT.get(format!("{base}/-/tags?format=atom")).call()?)?.ok_or(Error::Missing)?;
        return Ok(None);
    };
    let ships = match uuid {
        // `tag` is as the redirect gave it: already percent-encoded.
        Some(u) => {
            let api = format!(
                "https://{host}/api/v4/projects/{}/releases/{tag}",
                path.replace('/', "%2F")
            );
            read(AGENT.get(api).call()?)?
                .and_then(|json| serde_json::from_str::<Value>(&json).ok())
                .is_some_and(|release| ships_in(&release, u))
        }
        None => false,
    };
    Ok(Some(Release { tag, ships }))
}
