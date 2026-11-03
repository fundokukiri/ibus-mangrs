# Data files

## `ibus-mangrs.xml`

IBus component registration, installed to `/usr/share/ibus/component/` by
`scripts/install.sh`.

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
