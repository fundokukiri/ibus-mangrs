#!/bin/sh
# One-line online installer: auto-installs dependencies, downloads the source,
# builds, and installs.
#
#   curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
#
# Installs everything the build needs on Debian/Ubuntu (apt), Arch (pacman) and
# Fedora (dnf): build toolchain, IBus, GTK4, DBus headers, Rust (via rustup).
# Needs curl and sudo (or run as root). Re-run to upgrade.

set -eu

REPO=fundokukiri/ibus-mangrs
REF=master

die() {
    echo "ERROR: $*" >&2
    exit 1
}

# ---- detect distro ----
DISTRO=""
if [ -r /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
fi

case "${ID:-}" in
    debian|ubuntu|linuxmint|pop|elementary|kali|zorin) DISTRO=debian ;;
    arch|manjaro|endeavouros|garuda|artix) DISTRO=arch ;;
    fedora|rhel|centos|almalinux|rocky|ol) DISTRO=fedora ;;
esac

if [ -z "$DISTRO" ]; then
    case " ${ID_LIKE:-} " in
        *" debian "*|*" ubuntu "*) DISTRO=debian ;;
        *" arch "*) DISTRO=arch ;;
        *" rhel "*|*" fedora "*) DISTRO=fedora ;;
    esac
fi

# Last resort: go by the package manager that is actually installed.
if [ -z "$DISTRO" ]; then
    if command -v apt-get >/dev/null; then DISTRO=debian
    elif command -v pacman >/dev/null; then DISTRO=arch
    elif command -v dnf >/dev/null; then DISTRO=fedora
    fi
fi
[ -n "$DISTRO" ] || die "unsupported distro '${ID:-unknown}': install Rust, IBus, GTK4 and build tools yourself, then re-run"
echo ">> distro family: $DISTRO"

# ---- root / sudo ----
if [ "$(id -u)" -eq 0 ]; then
    SUDO=""
    echo ">> running as root; sudo will not be used"
else
    command -v sudo >/dev/null || die "sudo not found: install it or run this as root"
    SUDO=sudo
fi

# ---- package helpers ----
pkg_installed() {
    case "$DISTRO" in
        debian) dpkg-query -W -f='${Status}' "$1" 2>/dev/null | grep -q "ok installed" ;;
        arch) pacman -Qi "$1" >/dev/null 2>&1 ;;
        fedora) rpm -q "$1" >/dev/null 2>&1 ;;
    esac
}

# Install only the packages still missing; a re-run installs nothing.
ensure_pkgs() {
    missing=""
    for p in "$@"; do
        pkg_installed "$p" || missing="$missing $p"
    done
    if [ -z "$missing" ]; then
        echo ">> dependencies already installed"
        return 0
    fi
    echo ">> installing:$missing"
    # shellcheck disable=SC2086
    case "$DISTRO" in
        debian)
            $SUDO apt-get update -qq || die "apt-get update failed"
            $SUDO env DEBIAN_FRONTEND=noninteractive apt-get install -y -qq $missing \
                || die "failed to install packages (apt)"
            ;;
        arch)
            $SUDO pacman -Sy --needed --noconfirm $missing \
                || die "failed to install packages (pacman)"
            ;;
        fedora)
            $SUDO dnf install -y -q $missing || die "failed to install packages (dnf)"
            ;;
        *) die "unsupported distro '$DISTRO'" ;;
    esac
}

# GTK4 dev pulls the cairo/pango/glib/gdk-pixbuf/graphene headers in on every
# distro; DBus is needed by the `dbus` crate, not by GTK.
case "$DISTRO" in
    debian)
        DEPS="build-essential pkg-config git tar gzip curl ibus desktop-file-utils
            libdbus-1-dev libgtk-4-dev"
        ;;
    arch)
        DEPS="gcc make pkgconf git tar gzip curl ibus dbus gtk4 desktop-file-utils"
        ;;
    fedora)
        DEPS="gcc make pkgconf-pkg-config git tar gzip curl ibus dbus-devel
            gtk4-devel desktop-file-utils"
        ;;
esac

# shellcheck disable=SC2086
ensure_pkgs $DEPS

# ---- ensure rustup/cargo ----
if ! command -v cargo >/dev/null; then
    echo ">> installing Rust (rustup)..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
        || die "rustup install failed"
fi
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi
export PATH="$HOME/.cargo/bin:$PATH"
command -v cargo >/dev/null || die "cargo not found after installing Rust"

# ---- verify the headers the build actually links against ----
for mod in dbus-1 gtk4 glib-2.0 gdk-pixbuf-2.0 pango cairo graphene-1.0; do
    pkg-config --exists "$mod" \
        || die "missing development headers for $mod: a package above did not provide it"
done

# gtk4 crate enables v4_8/v4_10; building against an older GTK fails deep in
# rustc (Ubuntu 22.04 ships GTK 4.6), so stop early with a readable message.
gtk_ver=$(pkg-config --modversion gtk4)
case "$gtk_ver" in
    4.1[0-9]*|4.[2-9][0-9]*|[5-9].*) ;;
    *) die "GTK $gtk_ver is too old: the settings GUI needs 4.10+ (Ubuntu 22.04 ships 4.6, use 24.04 or newer)" ;;
esac
echo ">> GTK $gtk_ver, $(cargo --version)"

# ---- download, build, install ----
tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo ">> downloading ibus-mangrs ($REF)..."
curl -fsSL "https://github.com/$REPO/archive/refs/heads/$REF.tar.gz" | tar -xz -C "$tmp" \
    || die "download failed"
cd "$tmp/ibus-mangrs-$REF"

echo ">> building (this takes a few minutes)..."
cargo build --release --features gui || die "build failed"

echo ">> installing (needs sudo)..."
# shellcheck disable=SC2086
$SUDO ./scripts/install.sh

ibus-daemon -r --replace 2>/dev/null || ibus restart 2>/dev/null || true

echo
echo "done. enable the input source:"
echo "  Settings -> Keyboard -> Input Sources -> + -> Vietnamese -> MăngRS"
echo "  (log out and back in once if you were just added to the input group)"
