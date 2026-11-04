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
