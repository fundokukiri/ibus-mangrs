#!/usr/bin/env bash
# Local end-to-end functional tests for the MăngRS engine.
#
# Runs the real release binary on a private D-Bus session bus (dbus-run-session)
# and drives it exactly the way ibus-daemon and a focused client would, through
# the cargo integration tests in tests/e2e_<feature>.rs. Nothing here touches
# the system IBus, the user's config, or the real desktop: every case gets a
# temp XDG_CONFIG_HOME/XDG_CACHE_HOME, and no uinput key injection is ever
# triggered (injected keys would land in whatever window has focus).
#
# usage:
#   test/e2e/run.sh              # all features
#   test/e2e/run.sh preedit      # one feature file (tests/e2e_preedit.rs)
#   MANGRS_E2E_FILTER=word test/e2e/run.sh   # substring filter inside the cases
set -euo pipefail
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
BIN="$REPO/target/release/ibus-engine-ibus-mangrs"
FEATURE="${1:-}"

if [ ! -x "$BIN" ]; then
    echo "== building release binary =="
    if ! (cd "$REPO" && cargo build --release); then
        echo "== retrying without RUSTC_WRAPPER =="
        (cd "$REPO" && env -u RUSTC_WRAPPER cargo build --release)
    fi
fi

cd "$REPO"
# --ignored: the suites are #[ignore]d so plain `cargo test` never fires
# engines; MANGRS_E2E=1 is the second gate against the real session bus.
if [ -n "$FEATURE" ]; then
    exec dbus-run-session -- env MANGRS_E2E=1 MANGRS_E2E_BIN="$BIN" \
        cargo test --release --test "e2e_$FEATURE" -- --ignored --test-threads=1 --nocapture
fi
# The positional name filter selects every e2e_* test across the targets.
exec dbus-run-session -- env MANGRS_E2E=1 MANGRS_E2E_BIN="$BIN" \
    cargo test --release e2e_ -- --ignored --test-threads=1 --nocapture
