# Gextant

Updates GNOME Shell extensions straight from their upstream repositories, without waiting for the
review on extensions.gnome.org. It lists the extensions you have installed, finds each one's
repository from its `metadata.json`, checks for new releases and updates them on request, or by
itself for the extensions you choose. A repository link installs a new extension.

Gextant is at an early stage: it lists your extensions and checks them for updates, against their
repositories (GitHub, GitLab) and extensions.gnome.org; it does not install anything yet. See
what it does in [features](docs/FEATURES.md), what is coming in the [roadmap](docs/ROADMAP.md) and
what changed in the [changelog](docs/CHANGELOG.md).

## Building

Needs Rust, GTK 4, libadwaita, `blueprint-compiler` and `glib-compile-resources`.

```sh
cargo run            # the app, looking at your real extensions
make sandbox         # a copy with its own data and extensions folder, in target/sandbox
make sandbox-reset   # wipe the sandbox
```

## License

GPL-3.0-or-later, see [LICENSE](LICENSE).
