//! Updating an extension: download the new version from a source (GitHub, GitLab or
//! extensions.gnome.org), check it, and put it in place of the installed one, which is kept
//! as a backup. Nothing is run: the zip is only unpacked.

use crate::extensions::{self, Extension};
use crate::token::Tokens;
use crate::upstream::{AGENT, Source, read};
use gtk::glib;
use serde_json::Value;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zip::ZipArchive;

/// The most a download may be, and what it may unpack to.
const MAX_ZIP: u64 = 50 << 20;
const MAX_UNPACKED: u64 = 200 << 20;

/// What to install: the version `tag` of `repo` (`host/path`) on GitHub or GitLab, or what
/// extensions.gnome.org has for the running Shell.
#[derive(Clone)]
pub struct Plan {
    pub source: Source,
    pub repo: Option<String>,
    pub tag: String,
}

/// Install `plan` over the user's extension `ext`; the old copy is moved to the backups. `shell`
/// is the running Shell's major version. The new copy's `version-name` (or counter) on success.
pub fn update(ext: &Extension, plan: &Plan, shell: Option<&str>) -> Result<String, String> {
    if ext.system {
        return Err("A system extension cannot be updated by you".into());
    }
    let url = download_url(ext, plan, shell, &Tokens::load())?;
    let bytes = download(&url)?;
    let work = base().join("tmp").join(&ext.uuid);
    let _ = std::fs::remove_dir_all(&work);
    let result = unpack(&bytes, &ext.uuid, shell, &work).and_then(|new| {
        compile_schemas(&work);
        swap(ext, &work).map(|()| {
            if new.is_empty() {
                plan.tag.clone()
            } else {
                new
            }
        })
    });
    let _ = std::fs::remove_dir_all(&work);
    result
}

/// Gextant's own folder: temporary copies and backups. On the same disk as the extensions, so
/// that a folder is moved in one step.
fn base() -> PathBuf {
    glib::user_data_dir().join("gextant")
}

fn backup_dir(uuid: &str) -> PathBuf {
    base().join("backups").join(uuid)
}

fn download_url(
    ext: &Extension,
    plan: &Plan,
    shell: Option<&str>,
    tokens: &Tokens,
) -> Result<String, String> {
    let err = |e: crate::upstream::Error| match e {
        crate::upstream::Error::RateLimited(host) => format!("{host} says too many requests"),
        crate::upstream::Error::Missing => "The release was not found".to_owned(),
        crate::upstream::Error::Other(e) => e,
    };
    let (host, path) = match (&plan.source, &plan.repo) {
        (Source::Ego, _) => return ego_url(&ext.uuid, shell),
        (_, Some(repo)) => repo
            .split_once('/')
            .ok_or("No repository to install from")?,
        _ => return Err("No repository to install from".into()),
    };
    let tag = &plan.tag;
    if host == "github.com" {
        let page = format!("https://github.com/{path}/releases/expanded_assets/{tag}");
        let html = read(AGENT.get(page).call().map_err(|e| e.to_string())?)
            .map_err(err)?
            .unwrap_or_default();
        let prefix = format!("/{path}/releases/download/{tag}/");
        let names: Vec<&str> = html
            .split(&prefix)
            .skip(1)
            .filter_map(|s| s.split('"').next())
            .collect();
        return Ok(match pick(&names, &ext.uuid) {
            Some(name) => format!("https://github.com{prefix}{name}"),
            // No such file: the source of the release, if it is the extension.
            None => format!("https://github.com/{path}/archive/refs/tags/{tag}.zip"),
        });
    }
    let api = format!(
        "https://{host}/api/v4/projects/{}/releases/{tag}",
        path.replace('/', "%2F")
    );
    let mut request = AGENT.get(api);
    if let Some(token) = tokens.for_host(host) {
        request = request.header("PRIVATE-TOKEN", token);
    }
    let release: Option<Value> = read(request.call().map_err(|e| e.to_string())?)
        .map_err(err)?
        .and_then(|j| serde_json::from_str(&j).ok());
    let links = release
        .iter()
        .flat_map(|r| r["assets"]["links"].as_array().into_iter().flatten())
        .filter_map(|l| l["url"].as_str());
    let links: Vec<&str> = links.collect();
    let names: Vec<&str> = links.iter().filter_map(|u| u.rsplit('/').next()).collect();
    if let Some(name) = pick(&names, &ext.uuid)
        && let Some(url) = links.iter().find(|u| u.ends_with(&format!("/{name}")))
    {
        return Ok((*url).to_owned());
    }
    let last = path.rsplit('/').next().unwrap_or(path);
    Ok(format!(
        "https://{host}/{path}/-/archive/{tag}/{last}-{tag}.zip"
    ))
}

/// Among the files of a release, the zip that is the extension: the one `gnome-extensions pack`
/// makes (`<uuid>.shell-extension.zip`), any `.shell-extension.zip`, or one named after it.
fn pick<'a>(names: &[&'a str], uuid: &str) -> Option<&'a str> {
    let short = uuid.split('@').next().unwrap_or(uuid).to_lowercase();
    let exact = format!("{uuid}.shell-extension.zip").to_lowercase();
    let find = |f: &dyn Fn(&str) -> bool| names.iter().copied().find(|n| f(&n.to_lowercase()));
    find(&|n| n == exact)
        .or_else(|| find(&|n| n.ends_with(".shell-extension.zip")))
        .or_else(|| find(&|n| n.ends_with(".zip") && n.starts_with(&short)))
}

/// EGO's own link to the version it has for this Shell.
fn ego_url(uuid: &str, shell: Option<&str>) -> Result<String, String> {
    let shell = shell.ok_or("The Shell's version is not known")?;
    let uuid = uuid.replace('@', "%40").replace('+', "%2B");
    let info =
        format!("https://extensions.gnome.org/extension-info/?uuid={uuid}&shell_version={shell}");
    let json = read(AGENT.get(info).call().map_err(|e| e.to_string())?)
        .map_err(|_| "extensions.gnome.org could not be asked".to_owned())?
        .ok_or("extensions.gnome.org does not know the extension")?;
    let info: Value = serde_json::from_str(&json).map_err(|e| e.to_string())?;
    let path = info["download_url"]
        .as_str()
        .ok_or("extensions.gnome.org has no download for this Shell")?;
    Ok(format!("https://extensions.gnome.org{path}"))
}

fn download(url: &str) -> Result<Vec<u8>, String> {
    if !url.starts_with("https://") {
        return Err("Only HTTPS downloads are made".into());
    }
    let mut r = AGENT
        .get(url)
        .config()
        .timeout_global(Some(Duration::from_secs(120)))
        .build()
        .call()
        .map_err(|e| e.to_string())?;
    if r.status().as_u16() != 200 {
        return Err(format!("The download failed: HTTP {}", r.status().as_u16()));
    }
    r.body_mut()
        .with_config()
        .limit(MAX_ZIP)
        .read_to_vec()
        .map_err(|e| format!("The download failed: {e}"))
}

/// Check the zip and unpack it into `dest`: a `metadata.json` of this extension (`uuid`) that
/// supports the running Shell, at the top or in the one folder that holds everything, and no
/// path leading out of `dest`. The new version's name.
fn unpack(bytes: &[u8], uuid: &str, shell: Option<&str>, dest: &Path) -> Result<String, String> {
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|_| "Not a zip file".to_owned())?;
    let mut names = Vec::new();
    for i in 0..zip.len() {
        let f = zip.by_index(i).map_err(|e| e.to_string())?;
        let path = f.enclosed_name().ok_or("The zip has an unsafe path")?;
        names.push(path);
    }
    // The shortest `metadata.json`: at the top, or one folder down.
    let (prefix, _) = names
        .iter()
        .filter(|p| p.file_name().is_some_and(|n| n == "metadata.json"))
        .filter_map(|p| Some((p.parent()?.to_path_buf(), p.components().count())))
        .filter(|(_, depth)| *depth <= 2)
        .min_by_key(|(_, depth)| *depth)
        .ok_or("There is no extension to install in it (its zip has no metadata.json)")?;
    let mut text = String::new();
    zip.by_name(
        &prefix
            .join("metadata.json")
            .to_string_lossy()
            .replace('\\', "/"),
    )
    .map_err(|e| e.to_string())?
    .read_to_string(&mut text)
    .map_err(|_| "metadata.json cannot be read".to_owned())?;
    let new = extensions::parse(&text, false).ok_or("metadata.json is not an extension's")?;
    if new.uuid != uuid {
        return Err(format!("The zip is for {}, not {uuid}", new.uuid));
    }
    if let Some(shell) = shell
        && !new.supports(shell)
    {
        return Err(format!(
            "The new version does not support GNOME Shell {shell} (it supports {})",
            new.shell_versions.join(", ")
        ));
    }
    let mut total = 0;
    for (i, path) in names.iter().enumerate() {
        let Ok(rel) = path.strip_prefix(&prefix) else {
            continue; // Outside the folder that holds the extension.
        };
        let mut f = zip.by_index(i).map_err(|e| e.to_string())?;
        // A link would point out of the folder when the Shell reads it.
        if f.is_symlink() || rel.as_os_str().is_empty() {
            continue;
        }
        let target = dest.join(rel);
        if f.is_dir() {
            std::fs::create_dir_all(&target).map_err(|e| e.to_string())?;
            continue;
        }
        total += f.size();
        if total > MAX_UNPACKED {
            return Err("The zip unpacks to too much".into());
        }
        std::fs::create_dir_all(target.parent().unwrap_or(dest)).map_err(|e| e.to_string())?;
        let mut out = std::fs::File::create(&target).map_err(|e| e.to_string())?;
        std::io::copy(&mut f.by_ref().take(MAX_UNPACKED), &mut out).map_err(|e| e.to_string())?;
        out.flush().map_err(|e| e.to_string())?;
    }
    Ok(new.version().unwrap_or_default())
}

/// A zip with the schemas' sources only (as from a repository) needs them compiled. Best effort:
/// the tool may be missing, and then the extension's settings may not work.
/// ponytail: an unfound `glib-compile-schemas` (a Flatpak) is not reported.
fn compile_schemas(dir: &Path) {
    let schemas = dir.join("schemas");
    if schemas.is_dir() && !schemas.join("gschemas.compiled").exists() {
        let _ = std::process::Command::new("glib-compile-schemas")
            .arg(&schemas)
            .status();
    }
}

/// Put the unpacked `work` where `ext` is, the old one going to the backups (the last one kept).
/// Restored if the second step fails.
fn swap(ext: &Extension, work: &Path) -> Result<(), String> {
    let backup = backup_dir(&ext.uuid);
    let _ = std::fs::remove_dir_all(&backup);
    std::fs::create_dir_all(backup.parent().unwrap()).map_err(|e| e.to_string())?;
    std::fs::rename(&ext.path, &backup)
        .map_err(|e| format!("Could not move the old version away: {e}"))?;
    if let Err(e) = std::fs::rename(work, &ext.path) {
        let _ = std::fs::rename(&backup, &ext.path);
        return Err(format!("Could not put the new version in place: {e}"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;

    fn make(files: &[(&str, &str)]) -> Vec<u8> {
        let mut w = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, text) in files {
            w.start_file(*name, SimpleFileOptions::default()).unwrap();
            w.write_all(text.as_bytes()).unwrap();
        }
        w.finish().unwrap().into_inner()
    }

    const META: &str =
        r#"{"uuid":"a@b","name":"A","version-name":"2.0","shell-version":["50","51"]}"#;

    #[test]
    fn unpacks_a_checked_zip() {
        let dest = std::env::temp_dir().join(format!("gextant-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dest);
        let zip = make(&[
            ("x/metadata.json", META),
            ("x/extension.js", "1"),
            ("other", "2"),
        ]);
        assert_eq!(unpack(&zip, "a@b", Some("51"), &dest).as_deref(), Ok("2.0"));
        assert!(dest.join("extension.js").exists() && !dest.join("other").exists());
        let _ = std::fs::remove_dir_all(&dest);
    }

    #[test]
    fn refuses_what_is_not_the_extension() {
        let d = std::env::temp_dir().join("gextant-test-refused");
        let zip = make(&[("metadata.json", META)]);
        assert!(unpack(&zip, "c@d", Some("51"), &d).is_err()); // another UUID
        assert!(unpack(&zip, "a@b", Some("49"), &d).is_err()); // not for this Shell
        assert!(unpack(b"nope", "a@b", None, &d).is_err());
        let zip = make(&[("metadata.json", META), ("../evil", "x")]);
        assert!(unpack(&zip, "a@b", Some("51"), &d).is_err()); // a path out of the folder
        assert!(!d.exists());
    }

    #[test]
    fn picks_the_zip_of_the_extension() {
        let names = [
            "src.tar.gz",
            "a-2.0.zip",
            "a@b.shell-extension.zip",
            "x.shell-extension.zip",
        ];
        assert_eq!(pick(&names, "a@b"), Some("a@b.shell-extension.zip"));
        assert_eq!(pick(&names[..2], "a@b"), Some("a-2.0.zip"));
        assert_eq!(pick(&names[..1], "a@b"), None);
    }
}
