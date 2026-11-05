# Contributing

Thanks for your interest in Gextant!

## Ideas, bugs and questions

They are very welcome: open an [issue](https://github.com/AperteDev/gextant/issues) for a bug, a
feature request, an idea or a question. For bugs, mention your GNOME version and how to
reproduce the problem; for a wrong or missing version, the extension's UUID and the `url` in its
`metadata.json` help most.

## Pull requests

Gextant is developed along a plan, and I prefer to make feature changes myself: for a feature,
open an issue describing it rather than a pull request. A pull request for a bug fix or another
small change may be accepted, but please **don't open one before it has been agreed** in an
issue; an unannounced pull request may clash with work in progress or go in a different
direction, so it might not be merged.

When one has been agreed:

- Keep it to one change, and follow the style of the surrounding code.
- Run `cargo fmt`, `cargo clippy` and `cargo test`.
- Use [Conventional Commits](https://www.conventionalcommits.org/) for commit messages, e.g.
  `fix: read the version of a tag that has no v`.

By contributing, you agree that your contribution is licensed under the
[GPL-3.0-or-later](../LICENSE).
