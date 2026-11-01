# Data files

## `ibus-mangrs.xml`

IBus component registration, installed to `/usr/share/ibus/component/` by
`scripts/install.sh`.

## `80-uinput.rules`

udev rule giving the `input` group access to `/dev/uinput` (the backspace
injector), installed to `/etc/udev/rules.d/` by `scripts/install.sh` and to
`/usr/lib/udev/rules.d/` by the PKGBUILD. See `docs/install.md`.
