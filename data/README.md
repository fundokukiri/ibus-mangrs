# Data files

    data/
    ├── README.md                  this file
    ├── gui/                       the settings window's text tables (compiled in)
    │   ├── gui-content.toml       Vietnamese, the install default
    │   └── gui-content.en.toml    English mirror
    ├── assets/                    resources compiled into the binary
    │   ├── emoji.toml             embedded emoji shortcode table
    │   └── logo.png               the MăngRS logo
    ├── system/                    files installed onto the system
    │   ├── ibus-mangrs.xml        IBus component registration
    │   ├── org.freedesktop.IBus.mangrs.desktop  desktop entry
    │   └── 70-uinput.rules        udev rule for /dev/uinput
    └── gnome-shell-extension/     the companion GNOME Shell extension

## `gui/` — gui-content.toml + gui-content.en.toml

Every user-visible text of the settings window (page/row titles, subtitles,
tooltips, links, the About page, the "(?)" explanations) — Vietnamese, with
the English mirror. Both are compiled into the binary with `include_str!` —
edit and rebuild, no code change needed. A missing or mistyped key falls
back to the default baked into the code, so a typo can never blank the
interface.

## `assets/emoji.toml`

The embedded emoji shortcode table (about 1940 entries with Vietnamese and
English keywords), compiled into the binary with `include_str!`. Generated
once by a one-off local script from the CLDR annotations (vi + en) — do not
edit by hand; regenerate instead (`tools/gen_emoji.py`). Users add or
override entries in `~/.config/ibus-mangrs/emoji.toml`, which wins over
this table.

## `assets/logo.png`

The MăngRS logo: embedded into the About page at build time, installed to
`/usr/share/icons/hicolor/512x512/apps/ibus-mangrs.png` and embedded by the
READMEs as an image. Replacing the file changes the logo everywhere.

## `system/ibus-mangrs.xml`

IBus component registration, installed to `/usr/share/ibus/component/` by
`scripts/install.sh`. The bus name, engine name and component paths must
match the code (see AGENTS.md) or the daemon silently never spawns the
engine.

## `system/org.freedesktop.IBus.mangrs.desktop`

The desktop entry, installed to `/usr/share/applications/` by
`scripts/install.sh`.

## `system/70-uinput.rules`

udev rule giving the active session's user access to `/dev/uinput` through
logind's uaccess ACL (the key injector). No group membership is involved:
`input` can read every keyboard of the machine. Installed to
`/etc/udev/rules.d/` by `scripts/install.sh` and to `/usr/lib/udev/rules.d/`
by the PKGBUILD. It was called `80-uinput.rules` before; udev reads every
file in the directory, so the installer deletes the old name when it
recognizes it as an older MăngRS rule. See the "`/dev/uinput` permissions"
section of the README.

## `gnome-shell-extension/`

Companion GNOME Shell extension "MăngRS Focus" (`mangrs-focus@fundokukiri`):
exposes the focused window's class over D-Bus so per-app rules can target
Wayland-native apps, which IBus reports only as an anonymous mutter relay.
Not installed by `scripts/install.sh`; install and verify it through its own
`README.md`.
