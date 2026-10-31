#!/bin/sh
# One-line online installer: downloads the source, builds it, installs.
#
#   curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/dev/scripts/install-online.sh | sh
#
# Needs: rust (cargo), ibus, gtk4 and sudo. The /dev/uinput udev rule and
# the input group are set up by scripts/install.sh. Re-run to upgrade.
set -eu

REPO=fundokukiri/ibus-mangrs
BRANCH=dev

command -v curl >/dev/null || {
    echo "missing curl — install the curl package of your distro first" >&2
    exit 1
}
command -v cargo >/dev/null || {
    echo "missing rust/cargo — install it first:" >&2
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh" >&2
    exit 1
}
command -v ibus-daemon >/dev/null || {
    echo "ibus not found — install the ibus package of your distro first" >&2
    exit 1
}

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo ">> downloading ibus-mangrs ($BRANCH)..."
curl -fsSL "https://github.com/$REPO/archive/refs/heads/$BRANCH.tar.gz" | tar -xz -C "$tmp"
cd "$tmp/ibus-mangrs-$BRANCH"

echo ">> building (this takes a minute)..."
cargo build --release --features gui

echo ">> installing (needs sudo)..."
sudo ./scripts/install.sh
ibus restart

echo
echo "done. enable the input source:"
echo "  Settings -> Keyboard -> Input Sources -> + -> Vietnamese -> MăngRS"
