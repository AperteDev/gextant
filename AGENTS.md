# Agent notes

## Roadmap

- `docs/ROADMAP.md` holds the planned work only (features, open questions, what was ruled out);
  what is built is described by the code. Read it before working on a planned feature, and
  update it in the same change: remove what is done, add a plan or an open question, move a
  dropped plan under "Ruled out" with why.

## Siblings

- Gextant follows the conventions of Notav and Todav (`AperteDev/notav`, `AperteDev/todav`,
  cloned next to this repository): app id `dev.aperte.Gextant`, Blueprint views in `ui/`,
  GPL-3.0-or-later, sandbox copy for testing. When something is not decided here, look at how
  they do it.

## Dependencies

- Standard library and platform SDKs first; a third-party crate only where it replaces real
  work, with the reason in its commit.

## Testing

- `make sandbox` runs a separate copy (`GEXTANT_SANDBOX`, app id `dev.aperte.Gextant.Sandbox`)
  with `XDG_DATA_HOME=target/sandbox` (and `XDG_CACHE_HOME=target/sandbox/cache`): its data and the extensions it manages
  (`target/sandbox/gnome-shell/extensions/`) are separate from the real ones, so an update
  never touches what the Shell runs. Put test extensions there; `make sandbox-reset` wipes it.
- To see and drive the app, use the `waydriver` MCP server: `start_session` with `command` the
  absolute path of `scripts/waydriver-app.sh` and `app_name` `gextant-waydriver`; build first
  (`cargo build`).

## Releases

- Add user-visible changes to `docs/CHANGELOG.md` under `## [Unreleased]`, in the same commit as
  the change, in [Keep a Changelog](https://keepachangelog.com/en/1.1.0/)'s sections (`Added`,
  `Changed`, `Deprecated`, `Removed`, `Fixed`, `Security`). Keep `docs/FEATURES.md` (what
  Gextant does, one short line each, and what is planned) in step in the same commit: a new
  feature goes under "Now" and leaves "Planned"; a new plan in `docs/ROADMAP.md` gets its line
  under "Planned".
- Don't bump versions by hand: releases are cut by the manual Release workflow
  (`.github/workflows/release.yml`, with the version as its input). `scripts/release.py` sets the
  version in `Cargo.toml` and `Cargo.lock` and moves the `[Unreleased]` changes under it; the
  workflow commits and tags that and publishes the GitHub release with them as its notes. It
  goes on only if CI (`.github/workflows/ci.yml`: `cargo fmt --check`, clippy with warnings as
  errors, the unit tests, all built with the version set) passes. Run the same checks before
  committing.
- There are no packages yet. When they come (Flatpak, AUR, `.deb`), the Release workflow gets
  their jobs as in Notav's, and `scripts/release.py` the files that carry a version (metainfo,
  spec), as Notav's does.
