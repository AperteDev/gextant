# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.2.0] - 2026-11-07

### Changed

- Only a repository's published releases are checked: a tag without a release is ignored, so
  a repository that only has tags shows as having no releases
- An update stands out in the list: what is up to date is shown quietly (no longer green), a
  newer version is a blue pill, its extension has an update icon and moves to the top, and the
  title says how many updates there are

### Added

- Checks without the window: "At login and on start" checks once at login and notifies;
  "Every hour", "6 hours" and "day" keep Gextant running in the background (also after its
  window is closed) and check on a timer, notifying about new updates. Gextant adds the
  autostart entry itself while one of these is chosen
- Preferences: Update Notifications (on by default, shown where checks repeat) and Dock Counter
  (on by default), the number of updates on Gextant's icon in Ubuntu Dock and Dash to Dock
  while Gextant runs
- Preferences has two tabs: General, with how often to check for updates (manually, on start
  only by default, every hour, every 6 hours, every day) and a Prereleases switch (off by
  default; GitHub only, through the API); and Connections, with the access tokens
- A system extension whose UUID is also in your folder (a package linked there, say) stays in
  the list with a warning icon and, in its details, a note that your copy is the one in use
- A warning before an update replaces a link in your extensions folder with a copy of its own
- Update buttons in an extension's details, next to each source (GitHub, GitLab,
  extensions.gnome.org) that has a newer version: the download is checked (the same extension,
  for the running Shell, no path out of its folder), put in place of the installed one, which
  is kept as a backup, and the window says to log out to apply it
- An RPM for Fedora on COPR (`selfref/gextant`), built with each release
- A warning icon on an extension that does not support the running GNOME Shell, its tooltip
  naming the versions it does
- A details window, opened by clicking an extension: its description (one line, expandable, with
  basic Markdown shown), UUID, version, folder,
  the Shell versions it supports, the Shell's status for it (enabled, disabled, error, out of
  date) with a button to enable or disable it where it supports the running Shell, a button to
  open its settings, its website, what the checks found, and the rest of its `metadata.json`;
  its buttons (copy the UUID, open the folder or the settings, enable or disable, open the
  website) answer with a toast

## [0.1.0] - 2026-11-05

### Added

- The extensions installed on this computer, in a list: the user's (in
  `~/.local/share/gnome-shell/extensions`) first, then the system's, each with its name, UUID
  and version. Nothing is changed yet: the list only looks
- A check for new versions of the user's extensions, when Gextant starts and from the refresh
  button, against two places asked separately: the extension's repository, found from the `url`
  in its `metadata.json` (GitHub and GitLab, including `*.github.io` and `*.gitlab.io` project
  pages, and a project page on another domain that links to the repository), and
  extensions.gnome.org (for the running GNOME Shell's version)
- Each place shows as a chip, its icon with the version it has, in the colour of how it stands
  against what is installed: green if that is up to date, blue if it has something newer, grey
  if the two cannot be compared, yellow with a `?` where it has no version to give (no releases,
  a rate limit, a failed check, no repository found), the tooltip saying which. Places that
  agree make one chip with their icons together; the installed version shows by itself, with
  the computer icon, only where no chip has it. Long versions are shortened, in full in the
  tooltip
- Versions are compared as the repository's tags say them (`v2.5.0`), or, for the many
  extensions that carry only the number extensions.gnome.org gave them, against that number
  where the tag has it too (`v66`, `extensions.gnome.org-v110`, `2.2.0+10`)
- What was found is kept for 6 hours (a day where there is no repository), so a start does not
  ask every extension again; the refresh button always asks again
- Repositories are read from the forge's own pages, which cost no API quota. Preferences keep an
  optional access token for GitHub and one for GitLab (gitlab.com only, never sent to another
  GitLab server) in the keyring, used when the pages are limited or do not know a private
  repository; each comes with a link that fills in the token's name and permissions
- Banners when a forge's rate limit is used up (naming it, and offering Preferences where a
  token would help) and when a forge refuses a token (expired or revoked)
- An About dialog in the main menu, with a Support page that links to where the development can
  be supported (Ko-fi, GitHub Sponsors, Liberapay)
- Packages with each release: a Flatpak bundle, a tarball and a `.deb`, and the AUR packages
  `gextant`, `gextant-bin` and `gextant-git`; `make install` installs for the current user. The
  Flatpak reads the user's extensions folder on the host (read-only) and asks the Shell for its
  version over D-Bus; it does not list the system's extensions

[Unreleased]: https://github.com/AperteDev/gextant/compare/v0.2.0...HEAD
[0.2.0]: https://github.com/AperteDev/gextant/compare/v0.1.0...v0.2.0
[0.1.0]: https://github.com/AperteDev/gextant/releases/tag/v0.1.0
