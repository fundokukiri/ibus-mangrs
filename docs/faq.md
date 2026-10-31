# FAQ & Troubleshooting

## Installation

**The engine does not appear in Input Sources.**
Run `ibus write-cache --system` and then `ibus-daemon -drx`. Check that
`/usr/share/ibus/component/ibus-mangrs.xml` exists — if not, the install did not complete.

**Marks do not appear in some apps (Sublime Text, XWayland clients).**
`/dev/uinput` is missing or not writable. See the permissions section in
[installation.md](installation.md): udev rule + `input` group + `modprobe uinput`.

**I added myself to the `input` group but it still does not work.**
Group membership takes effect at the next login. Also make sure the kernel module is loaded:
`lsmod | grep uinput`, otherwise `sudo modprobe uinput` (persist with
`/etc/modules-load.d/uinput.conf`).

## Typing

**Marks appear in most apps but not in one specific app.**
That app gets its own display mode. Open the settings window → *Applications* and add a rule for
its process name, or switch the global mode. See [usage.md](usage.md).

**English words or identifiers get changed.**
They should not: invalid syllables auto-restore to the raw keystrokes, and words typed directly
after a URL/path character (`/ : @ . _ ~ = # & % ?`) stay raw. If a specific word is still
transformed, add it to `~/.config/ibus-mangrs/dictionary.txt` and enable the dictionary.

**A word disappears when I change settings mid-word.**
Should not happen since 0.1.1 — if it does, enable debug logging and report it.

**Clicking elsewhere while a word is still open inserts it in the wrong place.**
A mouse click cannot be seen by any IBus engine; finish the word (press space) before moving the
caret, or use non-preedit mode for that app.

**The uinput modes (Smooth, Super Smooth, Select) do nothing on GNOME Wayland.**
Expected: on a mutter-relayed Wayland session they resolve to preedit automatically. They are
intended for X11 sessions; making them work on Wayland requires a separate input-method route.

## Reporting a bug

Enable **Debug Logging** (GUI *General* tab, or `debug_log = true` in
`~/.config/ibus-mangrs/config.toml`), reproduce the issue, and attach
`~/.cache/ibus-mangrs/debug.log` to a GitHub issue. The log records engine decisions (focus,
capabilities, deletes, injections) with millisecond timestamps and never contains your typed
text — it can be cross-referenced with `journalctl` times.

Open issues: <https://github.com/fundokukiri/ibus-mangrs/issues>
