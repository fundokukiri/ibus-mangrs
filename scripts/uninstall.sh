#!/bin/sh
# Remove MăngRS from IBus. Config in ~/.config/ibus-mangrs is kept unless
# PURGE_CONFIG=1. Runs online too:
#   curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/uninstall.sh | sh
# Preview without deleting anything: --demo (or MANGRS_DEMO=1).
set -eu
PREFIX=${PREFIX:-/usr}
PURGE_CONFIG=${PURGE_CONFIG:-}

die() {
    echo "ERROR: $*" >&2
    exit 1
}

DEMO=${MANGRS_DEMO:-}
for arg in "$@"; do
    case "$arg" in
        --demo) DEMO=1 ;;
        -h|--help)
            echo "usage: uninstall.sh [--demo]"
            echo "  --demo  simulate the removal; nothing is deleted"
            exit 0
            ;;
        *) die "unknown option '$arg': usage uninstall.sh [--demo]" ;;
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

if [ -n "$DEMO" ]; then
    echo "MăngRS uninstaller (DEMO MODE — nothing is deleted)"
fi

# ---- root / sudo; demo never removes anything, so it never needs it ----
if [ -n "$DEMO" ]; then
    SUDO=""
elif [ "$(id -u)" -eq 0 ]; then
    SUDO=""
else
    command -v sudo >/dev/null || die "sudo not found: install it or run this as root"
    SUDO=sudo
fi

step "Removing engine files"
if [ -n "$DEMO" ]; then
    echo "(demo) nothing is removed"
else
    $SUDO rm -f "$PREFIX/lib/ibus-mangrs/ibus-engine-mangrs" \
          "$PREFIX/share/ibus/component/ibus-mangrs.xml" \
          "$PREFIX/share/ibus-mangrs/vn-words.txt"
    # vn-words.txt shipped with 0.1.3 and earlier; the engine never read it
    rmdir "$PREFIX/share/ibus-mangrs" 2>/dev/null || true
    rmdir "$PREFIX/lib/ibus-mangrs" 2>/dev/null || true
fi
ok

step "Removing icon and desktop entry"
if [ -n "$DEMO" ]; then
    echo "(demo) nothing is removed"
else
    $SUDO rm -f "$PREFIX/share/icons/hicolor/512x512/apps/ibus-mangrs.png" \
          "$PREFIX/share/applications/org.freedesktop.IBus.mangrs.desktop"
    # keep the caches consistent, the way install.sh built them
    if command -v gtk4-update-icon-cache >/dev/null; then
        gtk4-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
    elif command -v gtk-update-icon-cache >/dev/null; then
        gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
    fi
    update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true
fi
ok

step "Removing uinput rule"
if [ -n "$DEMO" ]; then
    echo "(demo) nothing is removed"
else
    # /etc for install.sh, /usr/lib for the package; the first one overrides.
    $SUDO rm -f /etc/udev/rules.d/80-uinput.rules \
          "$PREFIX/lib/udev/rules.d/80-uinput.rules"
    $SUDO udevadm control --reload-rules 2>/dev/null || true
    $SUDO udevadm trigger --name-match=/dev/uinput 2>/dev/null || true
fi
ok

step "Rebuilding IBus cache"
if [ -z "$DEMO" ]; then
    ibus write-cache --system 2>/dev/null || true
fi
ok

if [ -n "$PURGE_CONFIG" ]; then
    cfg_dir="${XDG_CONFIG_HOME:-$HOME/.config}/ibus-mangrs"
    step "Removing config"
    if [ -n "$DEMO" ]; then
        echo "(demo) would remove $cfg_dir"
    elif [ -d "$cfg_dir" ]; then
        # user-owned data: never sudo
        rm -rf "$cfg_dir"
    fi
    ok
fi

echo
echo "removed. run 'ibus restart', then remove the input source in Settings."
if [ -n "$DEMO" ]; then
    if [ -n "$PURGE_CONFIG" ]; then
        echo "(demo) config removed"
    else
        echo "(demo) config kept at ~/.config/ibus-mangrs"
    fi
elif [ -n "$PURGE_CONFIG" ]; then
    echo "config removed"
else
    echo "config kept at ~/.config/ibus-mangrs"
fi
