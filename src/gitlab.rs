//! GitLab (gitlab.com, and hosts named `gitlab.…`): the latest release from the project's pages,
//! as on GitHub; the API only to see the files of a release, when asked.

use crate::upstream::{AGENT, Error, Release, Repo, first_tag, host_of, read, ships};
use serde_json::Value;
use ureq::ResponseExt;

/// `/-/releases/permalink/latest` redirects to the release's page, `/-/releases/<tag>`; without
/// releases it is a 404, and the tags feed names the newest tag (a 404 there: no such project).
pub fn latest(repo: &Repo, uuid: Option<&str>) -> Result<Option<Release>, Error> {
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
        let feed = read(AGENT.get(format!("{base}/-/tags?format=atom")).call()?)?;
        let tag = feed.ok_or(Error::Missing)?;
        return Ok(first_tag(&tag).map(|tag| Release { tag, ships: false }));
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
                .is_some_and(|release| {
                    let links = release["assets"]["links"].as_array().into_iter().flatten();
                    let files = links.filter_map(|l| l["url"].as_str()?.rsplit('/').next());
                    ships(files, u)
                })
        }
        None => false,
    };
    Ok(Some(Release { tag, ships }))
}
