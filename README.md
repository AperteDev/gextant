# Gextant

Updates GNOME Shell extensions straight from their upstream repositories, without waiting for the
review on extensions.gnome.org. It lists the extensions you have installed, finds each one's
repository from its `metadata.json`, checks for new releases and updates them on request, or by
itself for the extensions you choose. A repository link installs a new extension.

Gextant is at the very beginning: only the window exists. See the [roadmap](docs/ROADMAP.md).

## Building

Needs Rust, GTK 4, libadwaita, `blueprint-compiler` and `glib-compile-resources`.

```sh
cargo run            # the app, looking at your real extensions
make sandbox         # a copy with its own data and extensions folder, in target/sandbox
make sandbox-reset   # wipe the sandbox
```

## License

GPL-3.0-or-later, see [LICENSE](LICENSE).
