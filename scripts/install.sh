#!/bin/sh
# Install MăngRS into IBus. Needs root: `sudo ./scripts/install.sh`.
# Idempotent: re-run after a rebuild to upgrade.
set -eu

PREFIX=${PREFIX:-/usr}
LIBDIR="$PREFIX/lib/ibus-mangrs"
COMPONENT_DIR="$PREFIX/share/ibus/component"

here=$(cd "$(dirname "$0")/.." && pwd)
bin="$here/target/release/ibus-engine-mangrs"

[ -x "$bin" ] || {
    echo "missing $bin" >&2
    echo "build it first: cargo build --release --features gui" >&2
    exit 1
}

# IBus runs engines as the logged-in user, not as root, so the binary must be
# world-readable and executable.
install -Dm755 "$bin" "$LIBDIR/ibus-engine-mangrs"
install -Dm644 "$here/data/ibus-mangrs.xml" "$COMPONENT_DIR/ibus-mangrs.xml"

# Rebuild the registry cache; without -t refresh the daemon serves the old list.
ibus write-cache --system 2>/dev/null || true

# App icon + desktop entry for the settings window (the Wayland window icon
# comes from the desktop file matching the app id, not from the binary).
install -Dm644 "$here/data/logo.png"     "$PREFIX/share/icons/hicolor/512x512/apps/ibus-mangrs.png"
install -Dm644 "$here/data/org.freedesktop.IBus.mangrs.desktop"     "$PREFIX/share/applications/org.freedesktop.IBus.mangrs.desktop"
# GTK4 ships the tool as gtk4-update-icon-cache; the GTK3 name only exists
# where GTK3 is installed, so try both instead of silently skipping the cache.
if command -v gtk4-update-icon-cache >/dev/null; then
    gtk4-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
elif command -v gtk-update-icon-cache >/dev/null; then
    gtk-update-icon-cache -f -t "$PREFIX/share/icons/hicolor" 2>/dev/null || true
fi
update-desktop-database "$PREFIX/share/applications" 2>/dev/null || true

# /dev/uinput access for the real-key injector. The active session's user gets
# the device through logind's uaccess ACL (data/70-uinput.rules), which ends
# with the session. Older installers added the user to the `input` group
# instead; that group can read every keyboard of the machine, so it is removed.
rule_src="$here/data/70-uinput.rules"
rule_dst=/etc/udev/rules.d/70-uinput.rules

# The rule used to be 80-uinput.rules. udev reads every file in the directory,
# so a differently named old copy still applies its `GROUP="input"` line and
# must be removed — only when it is ours (marker, old comment, or that line).
for legacy in /etc/udev/rules.d/80-uinput.rules "$PREFIX/lib/udev/rules.d/80-uinput.rules"; do
    [ -f "$legacy" ] || continue
    if grep -q 'ibus-mangrs' "$legacy" 2>/dev/null ||
        grep -q 'Backspace keys through /dev/uinput' "$legacy" 2>/dev/null ||
        grep -q 'GROUP="input"' "$legacy" 2>/dev/null; then
        rm -f "$legacy"
        echo "removed the old uinput rule: $legacy"
    else
        echo "warning: $legacy was not written by MăngRS; leaving it." >&2
        echo "         It may keep granting /dev/uinput to the input group." >&2
    fi
done

if [ -f "$rule_src" ]; then
    install -Dm644 "$rule_src" "$rule_dst"
    udevadm control --reload-rules ||
        echo "warning: udevadm control --reload-rules failed" >&2
    udevadm trigger --name-match=/dev/uinput ||
        echo "warning: udevadm trigger /dev/uinput failed" >&2
    if [ -n "${SUDO_USER:-}" ]; then
        if id -nG "$SUDO_USER" 2>/dev/null | tr ' ' '\n' | grep -qx input; then
            if gpasswd -d "$SUDO_USER" input >/dev/null 2>&1; then
                echo "removed $SUDO_USER from the input group (effective at next login)"
            else
                echo "warning: could not remove $SUDO_USER from the input group" >&2
            fi
            echo "  The input group can read every keystroke of the machine; MăngRS does not"
            echo "  need it. Add it back only if another tool needs raw input access:"
            echo "    sudo usermod -aG input $SUDO_USER"
        fi
        if command -v getfacl >/dev/null 2>&1; then
            if getfacl -p /dev/uinput 2>/dev/null | grep -q "^user:$SUDO_USER:"; then
                echo "uinput: session ACL present for $SUDO_USER"
            else
                echo "uinput: no ACL for $SUDO_USER yet; logind grants it once the session"
                echo "        is active, so log out and back in if injection stays off."
            fi
        fi
    fi
fi

echo "installed:"
echo "  $LIBDIR/ibus-engine-mangrs"
echo "  $COMPONENT_DIR/ibus-mangrs.xml"
echo "  $PREFIX/share/icons/hicolor/512x512/apps/ibus-mangrs.png"
echo "  $PREFIX/share/applications/org.freedesktop.IBus.mangrs.desktop"
echo
echo "next:"
echo "  1. ibus restart"
echo "  2. Settings -> Keyboard -> Input Sources -> + -> Vietnamese -> MăngRS"
echo "     (or: gsettings set org.gnome.desktop.input-sources sources \"[...]\")"
echo "  3. Super+Space to switch; the gear next to the engine opens the GUI"
