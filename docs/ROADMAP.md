# Gextant — Roadmap

Planned work, not yet done; what is built is described by the code. Gextant updates GNOME Shell
extensions straight from their upstream repositories, so a fix or a release reaches you without
waiting for the review on extensions.gnome.org (EGO). It does not replace the Extensions app or
EGO: it covers what they do not.

## 0.1: list, check, update, install from a link

### The list

The list shows the installed extensions (name, UUID, version), the user's and then the
system's (`/usr/share`, no actions: they cannot be updated by the user), and for the user's what
two separate checks found, on start and from the header's refresh button: the upstream (GitHub
or GitLab) and extensions.gnome.org (EGO). Each shows as its latest version with its icon, the
icon coloured as the version is. Sources that agree make one chip with two icons; those that
differ sit side by side with their own versions. Green: what is installed is current for that
source; yellow: that source has something newer; grey: the two cannot be compared. The
installed version is shown by itself, with the theme's computer icon
(`computer-symbolic`, as Nautilus's My Computer extension uses), only where no chip has it. The generic
globe is the theme's `globe-symbolic`, with a bundled one for themes that have none (Adwaita).
A long
version is cut to 12 characters; the whole one is in the tooltip. Where there is no version to
show, a word says why (no repository, no releases, rate limit reached, check failed). What is
left on a row:

- a mode, for user extensions: **Off** (not checked), **Check** (the default: look for updates,
  tell, never install) or **Update** (install new versions by itself); today every user
  extension is in Check;
- an **Update** button where one is available, and **Set repository…** where none was found.

### Finding what is installed

Scan `$XDG_DATA_HOME/gnome-shell/extensions/*/metadata.json` (honour `XDG_DATA_HOME`: that is
what lets the sandbox use a folder of its own). Used fields: `uuid`, `name`, `url`,
`shell-version`, and the version, `version-name` if there, else `version`.

What real extensions look like (a Shell 51 install, 13 extensions):

- 11 have `version` only, 2 have `version-name` only (`2.5.0`; `2.1.2-17+17d5fd1-dirty.…`).
  `version` is a counter EGO assigns on upload: it is **not** the upstream's version and cannot
  be compared with a tag (see [Open questions](#open-questions-and-risks)).
- `url` is a repository for most (`https://github.com/owner/repo`), but also seen: a `.git`
  suffix, a `/wiki` suffix, a project page (`https://micheleg.github.io/dash-to-dock/`) and
  an own domain (`https://nightthemeswitcher.romainvigier.fr`).

### Finding the upstream

In this order, the first that holds:

1. The repository the user set for this extension (**Set repository…**), kept by Gextant.
2. `url` from `metadata.json`, reduced to the repository: `github.com/<owner>/<repo>`, or a
   GitLab project (`gitlab.com`, or a host named `gitlab.…` such as `gitlab.gnome.org`, with
   its subgroups), without `.git`, `/wiki`, `/-/…` and the like.
3. A project page on `<owner>.github.io/<repo>` (or `.gitlab.io`): the candidate on that forge.
4. A project page on another domain (Night Theme Switcher's own site) is fetched (https only,
   the first megabyte) and the first three repositories it links to are the candidates.
5. Otherwise: no upstream; the row asks for one.

EGO is asked as well, always and at the same time, for the running Shell's major version (from
the Extensions service on D-Bus): `extension-info/?uuid=<uuid>&shell_version=<major>`, public
JSON with no key. Its `version` is the counter in `metadata.json`, so that comparison is exact,
and it shows when the portal is behind the repository, the reason for this app. Each check
fails or answers on its own; only when both worked is the result kept. Not built: EGO's `url`
as an upstream for extensions whose metadata has none.

A guess (3, 4) is accepted only if the latest release has a zip named after the extension: its
UUID, or what is before the `@` (`gnome-extensions pack` names it `<uuid>.shell-extension.zip`;
Night Theme Switcher ships `nightthemeswitcher.zip`). Still to do: the same check for a
repository the user sets, by the downloaded zip's `metadata.json` (see Updating), and keeping
the accepted repository, so a later release naming another `url` never moves it.

GitHub and GitLab in 0.1; Codeberg and Forgejo are under [Later](#later).

### What to install

Checking reads GitHub's `releases/latest` (else the first tag). To install, the release's asset
named `*.shell-extension.zip` is what gets installed. Without such an asset, the release's
source archive if its `metadata.json` is at the root (or in a single subfolder). Anything else
(a repo that needs `make`, meson or npm to produce the extension) shows "no release to
install"; Gextant does not run build scripts (see [Ruled out](#ruled-out)).

Checking spends no API quota: it reads github.com's pages (`/releases/latest` redirects to the
tag; `tags.atom` for repos that only tag; the assets page for a guessed repository). The API is
the fallback, and only with an access token (Preferences, kept in the keyring): when the pages
are limited or do not know the repository (a private one). Downloads for updating should use
`/releases/latest/download/<file>`, which costs no quota either. When even that is limited, a
banner says so and opens Preferences. A second banner says when GitHub refuses the token
(expired or revoked), found with `/rate_limit`, once per check, which costs no quota. A 403 counts as a rate limit only when GitHub says so;
otherwise its message is shown (a token that may not see a repository gets one). What was found is kept in the cache folder
(`checks.json`): 6 hours for a release, 24 for "no repository" (which can cost a project
page); the refresh button asks again anyway. Failures and rate limits are not kept, and the
installed version is compared again each time. Not built: conditional requests (`ETag`), to
make asking again cheaper still.

### Updating

Manual by default: **Update** on a row, and **Update all** in the header. The steps, each
checked before the next, and the old copy untouched until the last:

1. Download the asset over HTTPS to a temporary folder.
2. Check the zip: no path leaving the folder, a `metadata.json` with the same `uuid`, and the
   running Shell's major version in its `shell-version` (the version from D-Bus,
   `org.gnome.Shell.Extensions` `ShellVersion`, as the Shell may be out of reach of a Flatpak).
   Otherwise refuse and say why.
3. Extract beside the extension's folder, compile the schemas (`glib-compile-schemas`) if the
   zip carries sources only, and swap the folders; the old one is kept as a backup until the
   next successful update.
4. Show a banner: **Log out to apply**. A Wayland Shell cannot reload an extension's code, so
   the new version runs after the next login. Whether the extension is enabled is not touched;
   its settings live in dconf and are not touched either.

**Update** as an extension's mode does the same by itself, when Gextant checks: on start and
every few hours while it runs. (A background check without the window is [Later](#later).) It
installs only what passes the checks above, and tells what it did.

### Install from a link

A header button, **Add from Repository…**: a dialog with a field for `https://github.com/owner/repo`.
It resolves the repository the same way as above, then shows what was found (name, UUID,
description, version, repository, the Shell versions it supports) and asks to confirm, with
the reminder that an extension runs unsandboxed inside the Shell, with your session's rights.
It installs the latest release; the mode starts as Check. A UUID that is already installed is
not replaced, except by the same repository (then it is an update). The new extension is not
enabled (switching comes next); the banner says to log out and enable it in the Extensions app.

### State

Per extension, keyed by UUID, in `$XDG_DATA_HOME/gextant/state.json`: the mode, the repository,
and what Gextant installed (tag, asset, when). Never in the extension's own folder, which
updates replace.

### Technical

GTK 4 and libadwaita through gtk-rs, views in Blueprint. Network and unpacking on a worker
thread, results back to the main loop over a channel. Dependencies only where they replace real
work: `ureq` (HTTPS), `zip`, `serde_json`; a `core` crate split from the app once there is
logic worth testing without GTK. Logic gets unit tests (URL reduction, version comparison, zip
checks); the UI is driven with the sandbox.

## Later

Roughly in this order, each with its own plan when it comes:

- **Switch on and off**: a switch in the row, through `org.gnome.Shell.Extensions`
  (`EnableExtension`, `DisableExtension`), as the Extensions app does.
- **Details**: the description, authors, supported Shell versions, the repository link, the
  release notes of the update on offer, the installed and the backed-up version.
- **Settings button**: opens the extension's preferences (`OpenExtensionPrefs`), as in the
  screenshot of the Extensions app; shown only for extensions that have some.
- **More forges**: Codeberg and Forgejo, with the same checks; GitLab on a host not named
  `gitlab.…`; later a plain git URL.
- **Background checks**: a check without the window and a notification, for extensions in
  Check or Update mode (a user timer or the Background portal; to be planned together with
  autostart).
- **Rollback**: back to the backed-up version from the row.
- **Tracking a branch** for repositories that publish no releases (the version being a commit).
  Before installing from one, the row can say what the web gives for free: the date of the last
  commit (`commits/HEAD.atom`), against the date the extension's files were written (both
  repos of that kind here, PiP on top and Legacy Theme Switcher, were last committed days
  before their install, so "no change" would be right); file times are unreliable after a
  copy or restore.
- **Search** for extensions in repositories, to install from: where to search (GitHub topics
  such as `gnome-shell-extension`, a curated index, EGO's list only to *find* repositories) and
  how to judge what is worth showing are not decided yet; planned on their own.
- **Packages**: Flatpak (needs `--filesystem=xdg-data/gnome-shell/extensions` and the network;
  `org.gnome.Shell.Extensions` over D-Bus for the Shell version), AUR (`gextant`,
  `gextant-bin`, `gextant-git`), `.deb`; the desktop file, metainfo and icon come with them.
- **A warning before the token expires**: the API tells when in its
  `github-authentication-token-expiration` header (on any answer to the token).
- **Translations** (gettext, Polish first), as the sibling apps do; CI (fmt, clippy, tests).

## Open questions and risks

- **Which version is installed?** For an extension EGO installed, `version` is EGO's counter,
  and a release's tag says nothing about it. For ones Gextant installed it is known (state).
  For the rest, candidates: (a) a release published after the extension's files were written
  counts as an update (the files' times are unreliable after a copy or a restore); (b) show
  the latest release and ask once "is this installed?" (**Mark as installed**), which is
  certain, a little work for the user. Leaning to (b), with (a) as the hint. Built: where
  `version-name` looks like the tag (`2.5.0` and `v2.5.0`) they are compared directly, and a
  tag that carries EGO's number (`v66`, `66`, `extensions.gnome.org-v110`, `2.2.0+10`) is compared
  with EGO's counter, which fits 8 of the 13 extensions tried (their authors tag EGO's number);
  wrong for an author who numbers on their own. A tag such as `v1.2.0` is not read.
- **EGO updates against ours.** The Shell can itself offer updates for extensions installed
  from EGO; its version of the extension could replace ours, even with an older one, as the
  upstream zip's `metadata.json` carries no EGO `version`. To test on a real Shell before
  *Update* mode ships; if it happens, say so in the row and tell how to turn the Shell's check
  off.
- **Trust.** An update is code from the upstream's author running in the Shell. That is why
  Update is a choice per extension and off until chosen, the repository is fixed once accepted,
  and a changed UUID is refused. Release signatures are not widely used by extension authors;
  nothing to check there yet.
- **Wayland** cannot reload an extension: every update waits for a login. On X11 the Shell can
  reload (`Alt+F2`, `r`); not worth a code path for now.
- **GitHub's rate limit** on github.com's pages is not documented; the banner is there for when
  it is hit. Results are kept for hours, which already spares most requests.
- **The pages' markup** (the assets page, `tags.atom`) is not an API: it can change. The API
  fallback does not depend on them.

## Ruled out

- **Running a repository's build** (`make`, meson, npm, `gnome-extensions pack`): arbitrary
  code at update time, from sources nobody reviewed. A build step is for the author's release.
- **Publishing or reviewing** extensions, or anything to do with EGO's review: out of scope.
- **Replacing the Extensions app**: Gextant is for updating and installing from the source;
  the on/off switch and settings button above are there to make it enough by itself for the
  extensions it manages, not to copy the rest.
