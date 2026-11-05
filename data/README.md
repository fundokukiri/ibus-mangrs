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

## `80-uinput.rules`

udev rule giving the `input` group access to `/dev/uinput` (the backspace
injector), installed to `/etc/udev/rules.d/` by `scripts/install.sh` and to
`/usr/lib/udev/rules.d/` by the PKGBUILD. See `docs/install.md`.

## `gui-content.toml`

Every user-visible text of the settings window (page/row titles, subtitles,
tooltips, links, the About page, the "(?)" explanations). It is compiled into
the binary with `include_str!` — edit it and rebuild, no code change needed.
A missing or mistyped key falls back to the default baked into the code, so a
typo can never blank the interface. `logo.png` in this folder is embedded the
same way; replacing the file changes the logo.
