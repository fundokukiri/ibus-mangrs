# Data files

## `ibus-mangrs.xml`

IBus component registration, installed to `/usr/share/ibus/component/` by
`scripts/install.sh`.

## `emoji.toml`

The embedded emoji shortcode table (about 1940 entries with Vietnamese and
English keywords), compiled into the binary with `include_str!`. Generated
once by a one-off local script from the CLDR annotations (vi + en) — do not
edit by hand; regenerate instead. Users add or override entries in
`~/.config/ibus-mangrs/emoji.toml`, which wins over this table.

## `gnome-shell-extension/`

Companion GNOME Shell extension "MăngRS Focus" (`mangrs-focus@fundokukiri`):
exposes the focused window's class over D-Bus so per-app rules can target
Wayland-native apps, which IBus reports only as an anonymous mutter relay.
Not installed by `scripts/install.sh`; install and verify it through its own
`README.md`.

## `70-uinput.rules`

udev rule giving the active session's user access to `/dev/uinput` through
logind's uaccess ACL (the key injector). No group membership is involved:
`input` can read every keyboard of the machine. Installed to
`/etc/udev/rules.d/` by `scripts/install.sh` and to `/usr/lib/udev/rules.d/`
by the PKGBUILD. It was called `80-uinput.rules` before; udev reads every
file in the directory, so the installer deletes the old name when it
recognizes it as an older MăngRS rule. See the "`/dev/uinput` permissions"
section of the README.

## `gui-content.toml`

Every user-visible text of the settings window (page/row titles, subtitles,
tooltips, links, the About page, the "(?)" explanations). It is compiled into
the binary with `include_str!` — edit it and rebuild, no code change needed.
A missing or mistyped key falls back to the default baked into the code, so a
typo can never blank the interface. `logo.png` in this folder is embedded the
same way; replacing the file changes the logo.
