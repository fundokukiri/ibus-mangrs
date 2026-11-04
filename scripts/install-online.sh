#!/bin/sh
# One-line online installer: auto-installs dependencies, downloads the source,
# builds, and installs.
#
#   curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/dev/scripts/install-online.sh | sh
#
# Needs: sudo. Installs rustup + cargo, ibus, gtk4, and the build toolchain
# on your distro (apt / pacman / dnf). Re-run to upgrade.

set -eu

REPO=fundokukiri/ibus-mangrs
BRANCH=dev

die() {
    echo "ERROR: $*" >&2
    exit 1
}

# ---- detect distro ----
DISTRO=""
if [ -f /etc/os-release ]; then
    # shellcheck disable=SC1091
    . /etc/os-release
    DISTRO="${ID:-}"
fi

case "$DISTRO" in
    ubuntu|debian|linuxmint|pop) DISTRO=debian ;;
    fedora|rhel|centos|almalinux|rocky) DISTRO=fedora ;;
    arch|manjaro|endeavouros) DISTRO=arch ;;
esac

# ---- sudo check ----
if [ "$(id -u)" -ne 0 ]; then
    command -v sudo >/dev/null || die "sudo not found — install it first"
else
    echo ">> running as root; sudo will not be used"
fi
SUDO="${SUDO:-sudo}"

# ---- ensure curl ----
command -v curl >/dev/null || {
    echo ">> installing curl..."
    case "$DISTRO" in
        debian) "$SUDO" apt-get update -qq && "$SUDO" apt-get install -y -qq curl ;;
        arch)   "$SUDO" pacman -Sy --noconfirm curl ;;
        fedora) "$SUDO" dnf install -y -q curl ;;
        *)      die "unsupported distro '$DISTRO': install curl yourself" ;;
    esac
}

# ---- ensure rustup/cargo ----
if ! command -v cargo >/dev/null; then
    echo ">> installing rustup (Rust toolchain)..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    # cargo is installed by rustup; refresh PATH for this shell.
    if [ -f "$HOME/.cargo/env" ]; then
        # shellcheck disable=SC1090
        . "$HOME/.cargo/env"
    fi
    command -v cargo >/dev/null || die "rustup installed but cargo not found"
fi

# ---- ensure IBus is present ----
command -v ibus-daemon >/dev/null || {
    echo ">> IBus not found — installing the IBus package for your distro..."
    case "$DISTRO" in
        debian)
            "$SUDO" apt-get update -qq || true
            "$SUDO" apt-get install -y -qq ibus || die "failed to install ibus (apt)"
            ;;
        arch)
            "$SUDO" pacman -Sy --noconfirm ibus || die "failed to install ibus (pacman)"
            ;;
        fedora)
            "$SUDO" dnf install -y -q ibus || die "failed to install ibus (dnf)"
            ;;
        *)
            die "unsupported distro '$DISTRO': install ibus yourself and re-run"
            ;;
    esac
    command -v ibus-daemon >/dev/null || die "ibus package installed but ibus-daemon still not found"
}

# ---- install build + GUI dependencies (auto, per-dist

case "$DISTRO" in
    debian)
        echo ">> installing build toolchain, DBus, GTK4 and build deps (needs sudo)..."
        "$SUDO" apt-get update -qq || true
        "$SUDO" apt-get install -y -qq \
            build-essential \
            pkg-config \
            clang \
            gcc \
            git \
            libdbus-1-dev \
            libglib2.0-dev \
            libcairo2-dev \
            libpango1.0-dev \
            libgdk-pixbuf-2.0-dev \
            libgtk-4-dev \
            libgraphene-1.0-dev \
            libayatana-appindicator3-dev \
            libibus-1.0-dev \
            || die "failed to install dependencies (apt)"
        ;;
    arch)
        echo ">> installing build toolchain, GTK4 and build deps (needs sudo)..."
        "$SUDO" pacman -Sy --noconfirm \
            base-devel \
            gtk4 \
            gtk4-ime \
            pkgconf \
            clang \
            ibus \
            || die "failed to install dependencies (pacman)"
        ;;
    fedora)
        echo ">> installing build toolchain, GTK4 and build deps (needs sudo)..."
        "$SUDO" dnf install -y -q \
            rustup-core \
            gcc \
            clang \
            pkgconf-pkg-config \
            gtk4-devel \
            gtk4-immodules-extras \
            libappindicator-gtk3-devel \
            glib2-devel \
            cairo-devel \
            pango-devel \
            gdk-pixbuf2-devel \
            graphite2-devel \
            dbus-devel \
            || die "failed to install dependencies (dnf)"
        if ! command -v rustc >/dev/null; then
            rustup default stable || die "failed to install stable toolchain"
        fi
        ;;
    *)
        die "unsupported distro '$DISTRO': install rust, ibus, gtk4 and build tools yourself, then re-run"
        ;;
esac

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
echo ">> downloading ibus-mangrs ($BRANCH)..."
curl -fsSL "https://github.com/$REPO/archive/refs/heads/$BRANCH.tar.gz" | tar -xz -C "$tmp"
cd "$tmp/ibus-mangrs-$BRANCH"

echo ">> building (this takes a minute)..."
cargo build --release --features gui

echo ">> installing (needs sudo)..."
"$SUDO" ./scripts/install.sh
ibus-daemon -r --replace 2>/dev/null || ibus restart 2>/dev/null || true

echo
echo "done. enable the input source:"
echo "  Settings -> Keyboard -> Input Sources -> + -> Vietnamese -> MăngRS"
