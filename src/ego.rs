//! extensions.gnome.org (EGO): the portal's own record of an extension, from its public JSON
//! (no key). Its `version` is the counter in an installed `metadata.json`, so the comparison with
//! it is exact, unlike a repository's tags. Used where the upstream says nothing.

use crate::upstream::{AGENT, Error, read};
use serde_json::Value;

/// The latest version EGO has of `uuid` for GNOME Shell `shell` (its major version); `None` if
/// the portal does not know the extension, or has none for that Shell.
pub fn latest(uuid: &str, shell: &str) -> Result<Option<u64>, Error> {
    let uuid = uuid.replace('@', "%40").replace('+', "%2B");
    let url =
        format!("https://extensions.gnome.org/extension-info/?uuid={uuid}&shell_version={shell}");
    let Some(json) = read(AGENT.get(url).call()?)? else {
        return Ok(None);
    };
    let info: Value = serde_json::from_str(&json).map_err(|e| Error::Other(e.to_string()))?;
    Ok(info["version"].as_u64())
}
