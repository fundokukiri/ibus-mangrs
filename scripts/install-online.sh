#!/bin/sh
# One-line online installer: auto-installs dependencies, downloads the source,
# builds, and installs.
#
#   curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
#
# Installs everything the build needs on Debian/Ubuntu (apt), Arch (pacman),
# Fedora (dnf) and openSUSE (zypper): build toolchain, IBus, GTK4, DBus
# headers, Rust (via rustup). Nothing is installed when it is already there —
# re-run to upgrade — and the build-only packages this run installed are
# removed again after a successful install (KEEP_BUILD_DEPS=1 keeps them;
# runtime libraries always stay). On GNOME the MăngRS input source is added
# to the session automatically. Needs curl (or wget) and sudo (or run as root).

set -eu

# Non-empty keeps the build-only packages after a successful install
# (default: they are removed again — only what THIS run installed).
KEEP_BUILD_DEPS=${KEEP_BUILD_DEPS:-}
INSTALLED=""
RUSTUP_INSTALLED=""

REPO=fundokukiri/ibus-mangrs
REF=master

die() {
    echo "ERROR: $*" >&2
    exit 1
}

DEMO=${MANGRS_DEMO:-}
for arg in "$@"; do
    case "$arg" in
        --demo) DEMO=1 ;;
        -h|--help)
            echo "usage: install-online.sh [--demo]"
            echo "  --demo  simulate the whole install; nothing is installed or touched"
            exit 0
            ;;
        *) die "unknown option '$arg': usage install-online.sh [--demo]" ;;
    esac
done

# ---- display helpers: colors only on a TTY, plain text when piped ----
if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
    C_OK=$(printf '\033[32m'); C_DIM=$(printf '\033[2m'); C_R=$(printf '\033[0m')
else
    C_OK=''; C_DIM=''; C_R=''
fi
STEP=''
step() { STEP="$1"; printf '%s-> %s%s\n' "$C_DIM" "$STEP" "$C_R"; }
ok()   { printf '%s  ✓ %s%s\n' "$C_OK" "$STEP" "$C_R"; }

# one carriage-return redraw; a bad total renders nothing instead of dividing by zero
draw_bar() {
    got=$1 tot=$2
    if [ "$tot" -le 0 ]; then return 0; fi
    W=24
    filled=$(( got * W / tot ))
    if [ "$filled" -gt "$W" ]; then filled=$W; fi
    pct=$(( got * 100 / tot ))
    if [ "$pct" -gt 100 ]; then pct=100; fi
    bar=$(printf '%*s' "$filled" '' | tr ' ' '#')
    pad=$(printf '%*s' "$(( W - filled ))" '' | tr ' ' '.')
    printf '\r  [%s%s] %3d%%  %s / %s' "$bar" "$pad" "$pct" "$(hr "$got")" "$(hr "$tot")"
}
hr() { awk -v b="$1" 'BEGIN{printf "%.1f MB", b/1048576}'; }

if [ -n "$DEMO" ]; then
    echo "MăngRS installer (DEMO MODE — nothing is installed, nothing is touched)"
fi

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
    opensuse-leap|opensuse-tumbleweed|suse|sled|sles) DISTRO=suse ;;
esac

if [ -z "$DISTRO" ]; then
    case " ${ID_LIKE:-} " in
        *" debian "*|*" ubuntu "*) DISTRO=debian ;;
        *" arch "*) DISTRO=arch ;;
        *" rhel "*|*" fedora "*) DISTRO=fedora ;;
        *" suse "*|*" opensuse "*) DISTRO=suse ;;
    esac
fi

# Last resort: go by the package manager that is actually installed.
if [ -z "$DISTRO" ]; then
    if command -v apt-get >/dev/null; then DISTRO=debian
    elif command -v pacman >/dev/null; then DISTRO=arch
    elif command -v dnf >/dev/null; then DISTRO=fedora
    elif command -v zypper >/dev/null; then DISTRO=suse
    fi
fi
[ -n "$DISTRO" ] || die "unsupported distro '${ID:-unknown}': install Rust, IBus, GTK4 and build tools yourself, then re-run"
echo ">> distro family: $DISTRO"

# Fedora Silverblue/ostree variants layer packages into a new deployment:
# plain dnf install does not work there. Point at the manual path instead of
# half-installing.
if [ "$DISTRO" = fedora ] && command -v rpm-ostree >/dev/null; then
    die "Fedora Silverblue (rpm-ostree) detected: install the dependencies with 'rpm-ostree install gcc pkgconf-pkg-config ibus dbus-devel gtk4-devel libadwaita-devel' (reboot), then build from source — see README, 'Install from a local checkout'"
fi

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
        fedora|suse) rpm -q "$1" >/dev/null 2>&1 ;;
    esac
}

# Install only the packages still missing; a re-run installs nothing.
ensure_pkgs() {
    missing=""
    for p in "$@"; do
        pkg_installed "$p" || missing="$missing $p"
    done
    if [ -n "$DEMO" ]; then
        # demo reports what would happen; nothing is installed, INSTALLED stays empty
        if [ -z "$missing" ]; then
            echo ">> dependencies already installed"
        else
            echo "(demo) would install:$missing"
            sleep 1.5
        fi
        return 0
    fi
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
        suse)
            $SUDO zypper --non-interactive install $missing \
                || die "failed to install packages (zypper)"
            ;;
        *) die "unsupported distro '$DISTRO'" ;;
    esac
    INSTALLED="$INSTALLED $missing"
}

# GTK4 dev pulls the cairo/pango/glib/gdk-pixbuf/graphene headers in on every
# distro; DBus is needed by the `dbus` crate, not by GTK.
case "$DISTRO" in
    debian)
        # gcc is the linker (no C is compiled); libc6-dev ships the crt objects.
        DEPS="gcc libc6-dev pkg-config tar gzip curl ibus desktop-file-utils
            libdbus-1-dev libgtk-4-dev libadwaita-1-dev"
        ;;
    arch)
        DEPS="gcc pkgconf tar gzip curl ibus dbus gtk4 libadwaita desktop-file-utils"
        ;;
    fedora)
        DEPS="gcc pkgconf-pkg-config tar gzip curl ibus dbus-devel
            gtk4-devel libadwaita-devel desktop-file-utils"
        ;;
    suse)
        DEPS="gcc pkg-config tar gzip curl ibus dbus-1-devel gtk4-devel
            libadwaita-devel desktop-file-utils"
        ;;
esac

step "Preparing dependencies"
# shellcheck disable=SC2086
ensure_pkgs $DEPS
ok

# ---- ensure rustup/cargo ----
step "Checking toolchain"
if ! command -v cargo >/dev/null; then
    if [ -n "$DEMO" ]; then
        sleep 1
    else
        echo ">> installing Rust (rustup)..."
        RUSTUP_INSTALLED=1
        curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y \
            || die "rustup install failed"
    fi
fi
if [ -f "$HOME/.cargo/env" ]; then
    # shellcheck disable=SC1091
    . "$HOME/.cargo/env"
fi
export PATH="$HOME/.cargo/bin:$PATH"
if [ -z "$DEMO" ]; then
    command -v cargo >/dev/null || die "cargo not found after installing Rust"
fi

# ---- verify the headers the build actually links against ----
for mod in dbus-1 gtk4 libadwaita-1 glib-2.0 gdk-pixbuf-2.0 pango cairo graphene-1.0; do
    pkg-config --exists "$mod" \
        || die "missing development headers for $mod: a package above did not provide it"
done

# gtk4 crate enables v4_10 and libadwaita-rs v1_5; building against older
# libraries fails deep in rustc (Ubuntu 22.04 ships GTK 4.6), so stop early
# with a readable message.
gtk_ver=$(pkg-config --modversion gtk4)
case "$gtk_ver" in
    4.1[0-9]*|4.[2-9][0-9]*|[5-9].*) ;;
    *) die "GTK $gtk_ver is too old: the settings GUI needs 4.10+ (Ubuntu 22.04 ships 4.6, use 24.04 or newer)" ;;
esac
if ! pkg-config --modversion libadwaita-1 >/dev/null 2>&1; then
    die "libadwaita development headers missing: the settings GUI needs libadwaita 1.4+"
fi
adw_ver=$(pkg-config --modversion libadwaita-1)
case "$adw_ver" in
    1.[4-9]*|1.[1-9][0-9]*|[2-9].*) ;;
    *) die "libadwaita $adw_ver is too old: the settings GUI needs 1.4+" ;;
esac
echo ">> GTK $gtk_ver, $(cargo --version)"
ok

# ---- cleanup: drop the build-only packages THIS run installed ----
# The binary still links the runtime libraries (GTK4, libadwaita, dbus, ibus),
# so only the toolchain/header packages recorded by ensure_pkgs go. Packages
# that were already on the machine are never touched, and the runtime libs are
# explicitly re-marked as user-installed first so the package manager's
# autoremove cannot strip them from under an unpackaged binary.
cleanup_build_deps() {
    if [ -n "$KEEP_BUILD_DEPS" ]; then
        echo ">> KEEP_BUILD_DEPS set: build dependencies stay installed"
        return 0
    fi
    [ -n "$INSTALLED" ] || { echo ">> nothing was installed by this run"; return 0; }
    case "$DISTRO" in
        arch)   REMOVE_PKGS="gcc pkgconf desktop-file-utils" ;;
        debian) REMOVE_PKGS="gcc libc6-dev pkg-config libdbus-1-dev libgtk-4-dev libadwaita-1-dev desktop-file-utils" ;;
        fedora) REMOVE_PKGS="gcc pkgconf-pkg-config dbus-devel gtk4-devel libadwaita-devel desktop-file-utils" ;;
        suse)   REMOVE_PKGS="gcc pkg-config dbus-1-devel gtk4-devel libadwaita-devel desktop-file-utils" ;;
    esac
    victims=""
    # shellcheck disable=SC2086
    for p in $REMOVE_PKGS; do
        case " $INSTALLED " in
            *" $p "*) victims="$victims $p" ;;
        esac
    done
    if [ -z "$victims" ]; then
        echo ">> build dependencies were already present; nothing to clean"
        return 0
    fi
    echo ">> removing build dependencies installed by this run:$victims"
    case "$DISTRO" in
        arch)
            $SUDO pacman -Rs --noconfirm $victims \
                || echo ">> (pacman cleanup failed; packages kept)" ;;
        debian)
            $SUDO apt-get install -y -qq libgtk-4-1 libadwaita-1-0 libdbus-1-3 >/dev/null 2>&1 || true
            $SUDO env DEBIAN_FRONTEND=noninteractive apt-get purge -y -qq $victims || true
            $SUDO env DEBIAN_FRONTEND=noninteractive apt-get autoremove -y -qq || true
            ;;
        fedora)
            $SUDO dnf install -y -q gtk4 libadwaita dbus-libs >/dev/null 2>&1 || true
            $SUDO dnf remove -y -q $victims || true
            $SUDO dnf autoremove -y -q || true
            ;;
        suse)
            $SUDO zypper --non-interactive remove $victims || true
            ;;
    esac
}

# ---- enable the input source (GNOME) ----
# The one-liner promises a working keyboard when it returns: put MăngRS first
# in the GNOME input sources instead of asking for the Settings dance. Other
# desktops keep the printed manual step.
enable_gnome_input_source() {
    if [ -n "$DEMO" ]; then
        return 0
    fi
    command -v gsettings >/dev/null || return 0
    current=$(gsettings get org.gnome.desktop.input-sources sources 2>/dev/null) || return 0
    case "$current" in
        *"ibus-mangrs"*)
            echo ">> GNOME input sources already contain MăngRS"
            return 0
            ;;
    esac
    # "@a(ss) [('xkb', 'us')]" -> "('xkb', 'us')" (the type annotation and the
    # brackets do not survive a plain string append; strip in this exact order
    # or the bracket hides behind the padding space)
    list=${current#*@a\(ss\)}
    list=${list# }
    list=${list#[}
    list=${list%]}
    list=${list% }
    if [ -z "$(printf '%s' "$list" | tr -d ' ')" ]; then
        new="[('ibus', 'ibus-mangrs')]"
    else
        new="[('ibus', 'ibus-mangrs'), $list]"
    fi
    if gsettings set org.gnome.desktop.input-sources sources "$new" 2>/dev/null; then
        echo ">> MăngRS put first in GNOME input sources"
    fi
}

# ---- download (a real percentage bar when the size is known) ----
download_src() {
    url="https://github.com/$REPO/archive/refs/heads/$REF.tar.gz"
    if [ -n "$DEMO" ]; then
        # simulated download so the bar is visible without touching the network
        total=$((68 * 1024 * 1024))
        i=0
        while [ "$i" -le 100 ]; do
            if [ -t 1 ]; then draw_bar $(( total * i / 100 )) "$total"; fi
            sleep 0.05
            i=$(( i + 4 ))
        done
        if [ -t 1 ]; then draw_bar "$total" "$total"; printf '\n'; fi
        return 0
    fi
    hdr="$tmp/h.txt"
    if [ -t 1 ]; then
        curl -fsSL -D "$hdr" -o "$tmp/src.tar.gz" "$url" &
        cpid=$!
        # content-length arrives with the response headers; 0 means unknown size
        total=0
        n=0
        while [ "$n" -lt 30 ]; do
            if [ -s "$hdr" ]; then
                total=$(awk 'tolower($1)=="content-length:"{v=$2}END{sub(/\r$/,"",v);print v+0}' "$hdr")
            fi
            if [ "$total" -gt 0 ]; then break; fi
            if ! kill -0 "$cpid" 2>/dev/null; then break; fi
            n=$(( n + 1 ))
            sleep 0.1
        done
        while kill -0 "$cpid" 2>/dev/null; do
            got=$(wc -c < "$tmp/src.tar.gz")
            if [ "$total" -gt 0 ]; then draw_bar "$got" "$total"; else printf '\r  %s' "$(hr "$got")"; fi
            sleep 0.15
        done
        wait "$cpid" || die "download failed"
        if [ "$total" -gt 0 ]; then draw_bar "$total" "$total"; printf '\n'; fi
    else
        curl -fsSL -o "$tmp/src.tar.gz" "$url" || die "download failed"
    fi
    tar -xzf "$tmp/src.tar.gz" -C "$tmp" || die "download failed"
}

# ---- download, build, install ----
step "Downloading source"
if [ -z "$DEMO" ]; then
    tmp=$(mktemp -d)
    trap 'rm -rf "$tmp"' EXIT
fi
download_src
ok
if [ -z "$DEMO" ]; then
    cd "$tmp/ibus-mangrs-$REF"
fi

step "Building"
if [ -n "$DEMO" ]; then
    sleep 2
else
    cargo build --release --features gui || die "build failed"
fi
ok

step "Installing"
if [ -n "$DEMO" ]; then
    sleep 1
else
    # shellcheck disable=SC2086
    $SUDO ./scripts/install.sh
fi
ok

step "Cleaning build dependencies"
cleanup_build_deps
ok

step "Configuring GNOME input source"
enable_gnome_input_source
ok

step "Restarting IBus"
if [ -n "$DEMO" ]; then
    sleep 0.5
else
    ibus-daemon -r --replace 2>/dev/null || ibus restart 2>/dev/null || true
fi
ok

echo
echo "done. enable the input source:"
echo "  GNOME: done automatically above — switch with Super+Space if needed"
echo "  other desktops: Settings -> Keyboard -> Input Sources -> + -> Vietnamese -> MăngRS"
echo "  (log out and back in once if you were just added to the input group)"
if [ -n "$RUSTUP_INSTALLED" ]; then
    echo "  (Rust was installed via rustup and stays; remove with 'rustup self uninstall' if unwanted)"
fi
if [ -n "$DEMO" ]; then
    echo "demo complete — nothing was installed"
fi
