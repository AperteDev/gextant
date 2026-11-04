# A separate copy for testing, next to the normal app: its own data in target/sandbox, which is
# also where it looks for extensions (target/sandbox/gnome-shell/extensions), so it never touches
# the ones GNOME Shell runs. Kept between runs; sandbox-reset wipes it.
sandbox:
	cargo build
	GEXTANT_SANDBOX=1 XDG_DATA_HOME=$(CURDIR)/target/sandbox XDG_CACHE_HOME=$(CURDIR)/target/sandbox/cache target/debug/gextant

sandbox-reset:
	rm -rf target/sandbox

.PHONY: sandbox sandbox-reset
