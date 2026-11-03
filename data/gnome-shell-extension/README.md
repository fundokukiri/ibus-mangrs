# MăngRS Focus — GNOME Shell extension

Names the focused window over D-Bus so the MăngRS engine can apply
per-application rules to Wayland-native apps (Chrome, Firefox, Sublime Text,
...). Without it, those apps reach the engine only as an anonymous mutter
relay: IBus reports no application name, GNOME Shell Introspect denies
regular processes, and the wlroots window protocol ibus-bamboo uses is not
implemented by Mutter. The extension runs inside the shell, where the
focused window and its `wm_class` are freely available.

The engine uses this automatically when present (extension first, X active
window second, nothing otherwise). Without the extension everything keeps
working — per-app rules just cannot target Wayland-native apps by name.

## Install

```sh
mkdir -p ~/.local/share/gnome-shell/extensions
cp -r data/gnome-shell-extension/mangrs-focus@fundokukiri ~/.local/share/gnome-shell/extensions/
gnome-extensions enable mangrs-focus@fundokukiri
```

If the names still do not show up after enabling, log out and back in
(a Wayland session cannot restart the shell in place).

## Update

Copy the files over the installed copy again after pulling:

```sh
cp -r data/gnome-shell-extension/mangrs-focus@fundokukiri ~/.local/share/gnome-shell/extensions/
```

Then log out and back in. `gnome-extensions disable` + `enable` is **not**
enough: GNOME 45+ caches the extension's ES module for the whole session, so
the old `extension.js` keeps running until the session restarts.

## Troubleshooting

- `UnknownMethod: Object does not exist at path
  /io/github/fundokukiri/Mangrs` means the extension owns the bus name but
  never exported the object. Read `journalctl --user -b | grep mangrs-focus`.
  Errors `At least 5 arguments required` or `Expected type function` came from
  the calling convention GNOME 46-48 used; the current `extension.js` already
  targets the GJS API of GNOME 51, so log out and back in to load it.
- `GetFocusedWindow` returning `('', '')` is normal when nothing has keyboard
  focus (the overview, a panel popup).

## Requirements

- GNOME 46-51 (ESM extension format).
- The MăngRS engine installed and running.

## Verify

```sh
gdbus call --session --dest io.github.fundokukiri.Mangrs \
  --object-path /io/github/fundokukiri/Mangrs \
  --method io.github.fundokukiri.Mangrs.GetFocusedWindow
```

Returns `(wm_class, title)` of the focused window, or empty strings when
nothing has focus (the overview).

## Engine-side behavior

- The engine asks the extension on every focus change whose client is a
  mutter relay (`gnome-shell`) or unnamed — the focus path only, never
  between keystrokes.
- The name feeds rule matching (substring of the WM_CLASS, case-insensitive)
  and the quick-add list; delete-transport safety decisions are untouched,
  so mutter-relayed clients keep their crash-safe handling.
- Without the extension the engine falls back to the X active window
  (`xprop`), which names X11/XWayland apps only.
