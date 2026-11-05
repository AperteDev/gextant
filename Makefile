# Per-user install for development; PREFIX=/usr DESTDIR=... stages a system-wide one.
PREFIX ?= $(HOME)/.local
BINDIR = $(PREFIX)/bin
DATADIR = $(PREFIX)/share
ICONS = $(DATADIR)/icons/hicolor
ID = dev.aperte.Gextant

install:
	cargo build --release
	$(MAKE) install-data

# Binary and data files of an already built release. DESTDIR stages them for a package.
install-data:
	install -Dm755 target/release/gextant $(DESTDIR)$(BINDIR)/gextant
	install -Dm644 data/$(ID).desktop $(DESTDIR)$(DATADIR)/applications/$(ID).desktop
	install -Dm644 data/$(ID).metainfo.xml $(DESTDIR)$(DATADIR)/metainfo/$(ID).metainfo.xml
	install -Dm644 data/$(ID).svg $(DESTDIR)$(ICONS)/scalable/apps/$(ID).svg
	install -Dm644 data/$(ID)-symbolic.svg $(DESTDIR)$(ICONS)/symbolic/apps/$(ID)-symbolic.svg

# The system-wide tree the tarball and the .deb are made of, in DESTDIR:
#   make stage PREFIX=/usr DESTDIR=$PWD/target/stage
stage:
	cargo build --release --locked
	$(MAKE) install-data

uninstall:
	rm -f $(BINDIR)/gextant $(DATADIR)/applications/$(ID).desktop \
	  $(DATADIR)/metainfo/$(ID).metainfo.xml \
	  $(ICONS)/scalable/apps/$(ID).svg $(ICONS)/symbolic/apps/$(ID)-symbolic.svg

# A separate copy for testing, next to the normal app: its own data in target/sandbox, which is
# also where it looks for extensions (target/sandbox/gnome-shell/extensions), so it never touches
# the ones GNOME Shell runs. Kept between runs; sandbox-reset wipes it.
sandbox:
	cargo build
	GEXTANT_SANDBOX=1 XDG_DATA_HOME=$(CURDIR)/target/sandbox XDG_CACHE_HOME=$(CURDIR)/target/sandbox/cache target/debug/gextant

sandbox-reset:
	rm -rf target/sandbox

.PHONY: install install-data stage uninstall sandbox sandbox-reset
