#!/bin/sh
# Build, install and restart IBus in one step. Run as your user:
#   ./scripts/autoinstall.sh
# Re-run after every source change (idempotent).
set -eu

here=$(cd "$(dirname "$0")/.." && pwd)
cd "$here"

cargo build --release --features gui
sudo "$here/scripts/install.sh"
ibus restart
