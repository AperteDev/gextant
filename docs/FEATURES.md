# Gextant — Features

What Gextant can do, in short. The details of each release are in the
[changelog](CHANGELOG.md), the plans in the [roadmap](ROADMAP.md).

## Now

**Installed extensions**

- The extensions on this computer in a list: the ones you installed first, then the system's, each
  with its name, UUID and version.
- Only looks: nothing is installed, updated or changed yet.

**Checking for updates**

- Your extensions are checked when Gextant starts, and again from the refresh button.
- Two separate checks: the extension's repository, and extensions.gnome.org (the portal).
- The repository is found from the `url` in the extension's `metadata.json`: GitHub, GitLab
  (gitlab.com, and hosts such as `gitlab.gnome.org`), project pages on `github.io` and `gitlab.io`,
  and a website that links to its repository.
- The portal is asked for your GNOME Shell's version, so its answer is exact.
- Both answers stay when one fails or has nothing to say.

**What a row shows**

- Each place is an icon with the version it has: GitHub, GitLab or the GNOME mark for the portal.
- Green: what you have is current. Blue: that place has something newer. Grey: the two cannot be
  compared. Yellow with a `?`: that place has no version to give; the tooltip says why.
- Places that agree on a version share one chip, so a repository ahead of the portal shows as two.
- Your installed version, with a computer icon, shows by itself only where no chip has it.
- Long versions are shortened, in full in the tooltip.
- A generic globe where no repository could be found.

**Versions**

- Compared as the repository tags them (`v2.5.0`), or against the number the portal gave an
  extension where the tag carries it (`v66`, `extensions.gnome.org-v110`, `2.2.0+10`).

**Rate limits and tokens**

- Repositories are read from the forges' pages, which cost no API quota.
- What was found is kept for a few hours, so starting the app does not ask everything again.
- Optional GitHub and GitLab access tokens in Preferences, kept in your keyring, with links that
  fill in the token's name and permissions; the GitLab one only ever goes to gitlab.com.
- Banners when a forge's limit is used up, and when a token has expired or was revoked.

## Planned

- Updating an extension from its repository, by hand, with a check of what is downloaded.
- A mode for each extension: not checked, checked, or updated automatically.
- Installing an extension from a repository link.
- Set the repository of an extension where none was found; mark what is installed.
- Switching an extension on and off; its details and its settings button.
- Checking in the background, with a notification; going back to the previous version.
- Repositories that publish no releases, followed by their branch.
- Codeberg and other forges.
- Searching for extensions.
- Flatpak, AUR and `.deb` packages; translations.
