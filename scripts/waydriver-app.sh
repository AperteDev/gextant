#!/bin/bash
# The sandbox copy (as `make sandbox`) for a waydriver-mcp session: give its path as
# start_session's command, with app_name "gextant-waydriver". Build it first (`cargo build`).
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
export GEXTANT_SANDBOX=1 XDG_DATA_HOME="$root/target/sandbox"
exec -a gextant-waydriver "$root/target/debug/gextant" "$@"
