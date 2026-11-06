# Gextant — Features

What Gextant can do, in short. The details of each release are in the
[changelog](CHANGELOG.md), the plans in the [roadmap](ROADMAP.md).

## Now

**Installed extensions**

- The extensions on this computer in a list: the ones you installed first, then the system's, each
  with its name, UUID and version.
- A warning icon, with a tooltip, on an extension that does not support your GNOME Shell's version.
- Click an extension for its details: description, version, folder, supported Shell versions, the
  Shell's status for it, with buttons to enable or disable it (where it supports your Shell) and to
  open its settings, website, what the checks found, and the rest of its metadata.
- Only looks: nothing is installed, updated or changed yet.

**Checking for updates**

- Your extensions are checked when Gextant starts, and again from the refresh button.
- Two separate checks: the extension's repository, and extensions.gnome.org (the portal).
- The repository is found from the `url` in the extension's `metadata.json`: GitHub, GitLab
  (gitlab.com, and hosts such as `gitlab.gnome.org`), project pages on `github.io` and `gitlab.io`,
  and a website that links to its repository.
- The portal is asked for your GNOME Shell's version, so its answer is exact.
- Both answers stay when one fails or has nothing to say.

**Updating**

- In an extension's details, each source that has a newer version has an Update button: GitHub,
  GitLab or the portal. It installs that version over yours, after checking the download (the
  right extension, for your Shell, nothing outside its folder), and keeps your old copy as a
  backup. The new version runs after you log out and in.
- Only your extensions; the system's are not touched. Where yours is a link (a package linked
  into your folder), you are asked first, as an update replaces the link with a copy.
- A system extension that your folder shadows (the Shell loads yours) is listed with a warning
  icon and says so in its details.

**What a row shows**

- Each place is an icon with the version it has: GitHub, GitLab or the GNOME mark for the portal.
- Quiet (dimmed icon, plain version): what you have is current. A blue pill: that place has
  something newer. Grey: the two cannot be compared. Yellow with a `?`: that place has no version
  to give; the tooltip says why.
- An extension with an update gets an update icon, goes to the top of the list, and the title says
  how many there are.
- Places that agree on a version share one chip, so a repository ahead of the portal shows as two.
- Your installed version, with a computer icon, shows by itself only where no chip has it.
- Long versions are shortened, in full in the tooltip.
- A generic globe where no repository could be found.

**Versions**

- Only releases count; a tag nobody released is ignored.
- Compared as the repository tags them (`v2.5.0`), or against the number the portal gave an
  extension where the tag carries it (`v66`, `extensions.gnome.org-v110`, `2.2.0+10`).

**Preferences**

- General: how often to check for updates (manually, on start only, which is the default, every
  hour, every 6 hours, every day) and whether to check prereleases too (off by default; GitHub
  only, and each check then uses the API, which a token makes roomier).
- Connections: the access tokens.

**Rate limits and tokens**

- Repositories are read from the forges' pages, which cost no API quota.
- What was found is kept for a few hours, so starting the app does not ask everything again.
- Optional GitHub and GitLab access tokens in Preferences (Connections), kept in your keyring, with links that
  fill in the token's name and permissions; the GitLab one only ever goes to gitlab.com.
- Banners when a forge's limit is used up, and when a token has expired or was revoked.

**About**

- An About dialog in the main menu, with the version, the website, the issue tracker and the
  license; its Support page links to where the development can be supported (Ko-fi, GitHub
  Sponsors, Liberapay).

## Planned

- Updating from the list too, and all at once.
- A mode for each extension: not checked, checked, or updated automatically.
- Installing an extension from a repository link.
- Set the repository of an extension where none was found; mark what is installed.
- A switch for each extension in the list.
- Forcing an extension that does not support your Shell to load anyway (to be investigated).
- Checking in the background, with a notification; going back to the previous version.
- Repositories that publish no releases, followed by their branch.
- Codeberg and other forges.
- Searching for extensions.
- Flathub; translations.
