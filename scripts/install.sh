#!/bin/sh
# Install MăngRS into IBus. Needs root: `sudo ./scripts/install.sh`.
# Idempotent: re-run after a rebuild to upgrade.
set -eu

PREFIX=${PREFIX:-/usr}
LIBDIR="$PREFIX/lib/ibus-mangrs"
COMPONENT_DIR="$PREFIX/share/ibus/component"

here=$(cd "$(dirname "$0")/.." && pwd)
bin="$here/target/release/ibus-engine-ibus-mangrs"

[ -x "$bin" ] || {
    echo "missing $bin" >&2
    echo "build it first: cargo build --release --features gui" >&2
    exit 1
}

# IBus runs engines as the logged-in user, not as root, so the binary must be
# world-readable and executable.
install -Dm755 "$bin" "$LIBDIR/ibus-engine-ibus-mangrs"
install -Dm644 "$here/data/ibus-mangrs.xml" "$COMPONENT_DIR/ibus-mangrs.xml"
install -Dm644 "$here/data/vn-words.txt" "$PREFIX/share/ibus-mangrs/vn-words.txt"

# Rebuild the registry cache; without -t refresh the daemon serves the old list.
ibus write-cache --system 2>/dev/null || true

# /dev/uinput permission for the backspace injector (docs/install.md).
if [ -f "$here/data/80-uinput.rules" ]; then
    install -Dm644 "$here/data/80-uinput.rules" \
        /etc/udev/rules.d/80-uinput.rules
    udevadm control --reload-rules 2>/dev/null || true
    udevadm trigger /dev/uinput 2>/dev/null || true
    if [ -n "${SUDO_USER:-}" ] && ! id -nG "$SUDO_USER" 2>/dev/null | tr ' ' '\n' \
        | grep -qx input; then
        usermod -aG input "$SUDO_USER"
        echo "added $SUDO_USER to the input group (effective at next login)"
    fi
fi

echo "installed:"
echo "  $LIBDIR/ibus-engine-ibus-mangrs"
echo "  $COMPONENT_DIR/ibus-mangrs.xml"
echo
echo "next:"
echo "  1. ibus restart"
echo "  2. Settings -> Keyboard -> Input Sources -> + -> Vietnamese -> MăngRS"
echo "     (or: gsettings set org.gnome.desktop.input-sources sources \"[...]\")"
echo "  3. Super+Space to switch; the gear next to the engine opens the GUI"
