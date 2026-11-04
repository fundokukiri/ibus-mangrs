<p align="center">
  <img src="data/logo.png" alt="MăngRS logo" width="360">
</p>

# MăngRS

**English** | [Tiếng Việt](vi/README-vi.md)

A Vietnamese input method engine for **IBus**, written in **Rust** on top of
[`bamboo-core`](https://crates.io/crates/bamboo-core). Telex, VNI, VIQR and Microsoft layouts, ten display modes (Preedit, NonPreedit, Surrounding, two
ForwardKeyEvent modes, three uinput modes, OFF, Default Typing), a GTK4 settings window, macros,
a custom dictionary and per-app rules.

> **Platform status:** developed and tested on **Arch Linux** (IBus 1.5.34, GNOME 51, Wayland;
> X11 smoke-tested). Other distributions and desktop environments have **not** been tested yet —
> IBus ≥ 1.5 and GTK4 are the only hard requirements, so it should work wherever IBus works.
> Reports from other distros are welcome.

---

## Features

- **Four input methods**: Telex (`tieengs` → `tiếng`), VNI (`tie6ng1`), VIQR, Microsoft layout.
- **Eleven output charsets**: Unicode (default), Unicode NFD, TCVN3, VNI, VIQR, VISCII, VPS,
  CP1258, BKHCM, Vietware X, Vietware Full.
- **Eight display modes** (see below), selectable per application via App Rules.
- **Per-app rules for Wayland-native apps** (Chrome, Firefox, Sublime Text, ...) through the
  companion GNOME Shell extension: IBus reports those apps only as an anonymous mutter relay,
  so the extension names the focused window from inside the Shell. Optional — without it rules
  still match apps IBus names itself and X11 windows.
- **Typing options**: modern tone placement (`hòa` vs traditional `hoà`), free tone marking
  (`hoangf` → `hoàng`), auto-correction of invalid syllables (the phonotactic check),
  auto-restore of non-Vietnamese words, raw-key bypass after URL/email/path characters
  (`/ : @ . _ ~ = # & % ?`), `w` → `ư`, `[`/`]` → `ơ`/`ư`, `--` → `—`, auto-capitalize,
  double space → period.
- **Pending-word behavior on mouse clicks and idle** (three actions, selectable in the
  panel): Bamboo (default — a word left idle for 800 ms commits in place and a late tone
  key still applies the mark: `tung` + wait + `f` → `tùng`), Keep word (a click hides it,
  clicking back restores it) and Relocate (the word is always delivered to the position
  where it was typed). Vietnamese abbreviations stay intact (`ddc` → `đc`, `ddk` → `đk`).
- **Macros** with `$TIME` / `$DATE` expansion and smart capitalization; shorthand keys that are
  valid Vietnamese words never expand while you are typing that word normally.
- **Custom dictionary**: words that must never be auto-restored.
- **GTK4 settings window** (5 pages, Vietnamese); every change hot-reloads within ~1 second.
- **Backup/restore** of all settings as one JSON file.
- 227 unit tests including a 678-case keystroke→text golden table (102 Telex + 96 VNI + 24 VIQR rows x 3 case variants, 12 Microsoft rows).

## Display modes

| Mode                  | How it works                                                                            | Best for                                                                          |
| --------------------- | --------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Preedit (default)     | word shows inline with underline, commits at word end                                   | widest compatibility                                                              |
| Non-preedit           | every key commits immediately, tail rewritten on marks                                  | Firefox, GTK apps                                                                 |
| Surrounding text      | non-preedit + verifies against the app's own text report                                | Sublime Text                                                                      |
| Smooth (uinput)       | raw keys forwarded while typing, latin tail erased with real Backspaces at word end     | terminals, quirky apps (X11)                                                      |
| Super Smooth (uinput) | same, without the safety check before rewriting                                         | fast typing (X11)                                                                 |
| Select (uinput)       | Shift+← selects the tail, Delete erases it, word commits                                | apps where Backspaces misbehave (X11)                                             |
| ForwardKeyEvent       | non-preedit whose tail rewrite deletes with Backspace key events forwarded through IBus | apps that ignore the delete signal: Chrome, Electron, GNOME Console, Sublime Text |
| ForwardKeyEvent II    | the same, but the tail is selected with forwarded Shift+← and the commit replaces it    | apps where a forwarded Backspace is mishandled, X11/XIM apps (JetBrains)          |
| OFF                   | Vietnamese off, macros still work                                                       | mixed Vietnamese/English sessions                                                 |
| Default Typing        | engine fully inert, every key forwarded                                                 | apps that resent any processing                                                   |

Switch modes from the input-source menu, the settings window, an assignable cycle hotkey, or per
app with **App Rules**. The uinput modes need `/dev/uinput` and fall back to preedit wherever
real-key injection cannot reach the application (on GNOME Wayland they resolve to preedit; they
are intended for X11 sessions).

## Requirements

| Distro          | Installed by the one-line installer                 |
| --------------- | --------------------------------------------------- |
| Arch            | `ibus` `gtk4` `dbus`, `gcc` `make` `pkgconf`        |
| Debian / Ubuntu | `ibus` `libgtk-4-dev` `libdbus-1-dev`, build-essential |
| Fedora          | `ibus` `gtk4-devel` `dbus-devel`, `gcc` `make`      |

The installer detects the distro (apt / pacman / dnf), installs only what is missing and leaves
Rust to rustup; GTK 4.10+ is required (Ubuntu 22.04 ships 4.6). Only the Arch path is tested —
the Debian/Ubuntu and Fedora package names are the standard ones but unverified there. IBus must
be your active input-method framework (on GNOME it is).

## Build

```sh
cargo build --release --features gui
```

The engine core comes from the `bamboo-core` crate on crates.io. Source browsing of the pinned
`v0.5.3` is available via `git submodule update --init` (optional; the build uses crates.io).

## Install

### One-line install (online)

Downloads the source, builds it and installs; missing build dependencies (IBus, GTK4, the build
toolchain, Rust) are installed for you, so only `curl` and `sudo` are needed up front. The
`/dev/uinput` udev rule and the `input` group are set up too:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
```

(Piping a script to `sh` runs code from the network — review it first if that matters to you.)

### From a local checkout

Build, install and restart IBus:

```sh
cargo build --release --features gui
sudo ./scripts/install.sh
ibus restart
```

The install script copies the engine to `/usr/lib/ibus-mangrs/`, registers the component in
`/usr/share/ibus/component/`, installs the `/dev/uinput` udev rule and adds your user to the
`input` group (group membership takes effect at next login). Uninstall:
`sudo ./scripts/uninstall.sh` — keeps your config.

Arch package (uses the system udev directory for the udev rule):

```sh
makepkg -si
```

### /dev/uinput permissions (manual fallback)

The engine presses **real Backspace keys** through `/dev/uinput` when an app cannot delete text
over IBus. Without write permission those apps cannot type tone marks. Create
`/etc/udev/rules.d/80-uinput.rules` (shipped as `data/80-uinput.rules`):

```
KERNEL=="uinput", MODE="0660", GROUP="input"
```

Then:

```sh
sudo modprobe uinput                      # load the kernel module now
sudo usermod -aG input $USER              # takes effect at next login
sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput
echo uinput | sudo tee /etc/modules-load.d/uinput.conf   # load at every boot
```

Verify: `ls -l /dev/uinput` must show `crw-rw---- root input`.

## Enable the input source

1. `ibus restart` (after installing or upgrading the binary).
2. **Settings → Keyboard → Input Sources → +** → _Vietnamese_ → **MăngRS**
   (or `ibus engine ibus-mangrs`).
3. `Super+Space` switches input sources; the gear next to the engine opens the settings window.

## Usage

| Action                    | How                                                                                    |
| ------------------------- | -------------------------------------------------------------------------------------- |
| Toggle Vietnamese         | input-source menu → _Enable Vietnamese_, or `Super+Space`                              |
| Open settings window      | input-source menu → _Cài đặt…_, or `ibus-engine-ibus-mangrs -gui`                      |
| Change input method       | menu → _Input Method_ (Telex / VNI / VIQR / Microsoft), or GUI                         |
| Change charset            | menu → _Charset_ (11 charsets), or GUI                                                 |
| Change display mode       | menu → _Display_ (4 main modes), or GUI (10 modes), or the cycle hotkey                |
| Display mode cycle hotkey | GUI _Kiểu gõ_ tab → _Phím tắt đổi chế độ_ (e.g. `Control+Alt+v`)                       |
| Quick settings            | menu → _Quick Settings_ (macros, auto-capitalize, abbreviations, foreign-word restore) |
| Type English / code       | just type — invalid words revert to raw keystrokes; URLs/emails/paths stay raw         |
| Per-app display mode      | GUI _Dữ liệu_ tab → _Luật theo ứng dụng_ (see App Rules below)                         |

## App Rules (per-app display mode)

A rule forces one display mode for one application and overrides the global
mode. Rules are evaluated in order and the **first match wins**, so list
specific patterns above general ones.

**In the settings window:** _Dữ liệu_ tab → _Luật theo ứng dụng_. Pick an app in
the dropdown, pick a mode, press _Thêm luật_: the rule is appended as a
`name=mode` line and saved with the rest of the settings (edits made directly
in the text box save when you click outside it). The dropdown lists
only apps the engine has already seen (newest 50, `~/.cache/ibus-mangrs/clients`)
plus two pseudo-targets: `gnome-shell (app Wayland)` — every Wayland app while
the focus extension is not installed — and `(app không báo tên)` — apps that
report no name at all. Type once in an app to make it appear.

**By hand:** one `[[app_rules]]` block per rule in
`~/.config/ibus-mangrs/config.toml` (hot-reloaded within ~1 s):

```toml
[[app_rules]]
pattern = "chrome"
mode = "preedit"

[[app_rules]]
pattern = "sublime_text"
mode = "surrounding"
```

**Matching:** case-insensitive substring of the app name; a toolkit prefix
(`gtk4-im:`, `wayland:`, `x11:`) is ignored. The name comes from the companion
focus extension first, then the X active window (`xprop`), then the prgname
IBus reports. X11/XIM apps (JetBrains and other Java apps) report only the
fixed name `xim`, so they are named from the extension/`xprop` instead — a
rule like `pattern = "jetbrains"` targets them too. Short patterns match
broadly (`term` also matches `xterm`). `pattern = "none"` targets apps with no
name at all — the only way to match them.

**Modes:** `preedit` (default), `nonpreedit`, `surrounding`, `smooth`,
`supersmooth`, `select` (these three need `/dev/uinput` and are meant for X11),
`forwardkeyevent`, `forwardselect` (alias `forwardkeyevent2`), `off`,
`defaulttyping`. The two forward modes commit like `nonpreedit` but delete the
rewritten tail by forwarding key events through IBus instead of the delete
signal, which reaches clients that ignore `DeleteSurroundingText` (Chrome,
Electron, GNOME Console) even on Wayland, where `/dev/uinput` does not, and
X11/XIM apps (JetBrains), whose input context has no capabilities at all:
`forwardkeyevent` forwards Backspace, `forwardselect` selects the tail with
Shift+← and lets the commit replace it. They work only if the app accepts
forwarded key events, so try them per app.

**Built-in rules**, shown in the text box when `config.toml` has no `app_rules`
key (edit or delete them there; saving writes them to the file): Chrome,
Chromium, Brave, Edge, Vivaldi, Electron apps (VS Code, Slack, Discord) and
GNOME Console → `preedit`; Sublime Text → `surrounding`. Those clients drop
`DeleteSurroundingText`, so rewriting there duplicates text.

## X11/XIM options

Two compatibility switches in the settings window (_Kiểu gõ_ tab →
_Tương thích X11/XIM_). Both are **off by default**, and both are needed only
for apps that reach the engine in unusual ways:

| Switch                            | `config.toml`               | What it does                                                                                                                                                                                                                                                                                                                                                                                   |
| --------------------------------- | --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Delete without reporting surrounding elements | `x11_delete_without_report` | Lets a direct X11 client (Sublime Text, other GTK3 X11 apps) delete the rewritten tail over IBus even though it never reported its text — what ibus-bamboo does. Without it those clients keep the raw keystrokes when real-key injection is unavailable (`tungf`). Risk: without a report the delete count is unclamped, so a caret moved behind the engine's back can lose a few characters. |
| Preedit cho app XIM               | `preedit_for_xim`           | Keeps the underlined mode for XIM clients (JetBrains and other Java apps, which all report the fixed name `xim`). XIM servers draw the composing text in their own window and commit it at the word end. Without it those apps cannot use the default mode at all and need a `forwardkeyevent` rule. Risk: an app that negotiated `XIMPreeditNothing` shows nothing until the word ends.       |

## Configuration files

```
~/.config/ibus-mangrs/config.toml     # all settings, hot-reloaded within ~1 s
~/.config/ibus-mangrs/macros.toml     # one key=expansion per line, $TIME / $DATE
~/.config/ibus-mangrs/dictionary.txt  # words never auto-restored
```

## Troubleshooting

- **Engine not listed in Input Sources**: run `ibus write-cache --system` then `ibus-daemon -drx`;
  make sure `/usr/share/ibus/component/ibus-mangrs.xml` exists.
- **Marks do not appear in some apps** (Sublime, XWayland clients): `/dev/uinput` is missing or
  not writable — see the permissions section above.
- **Mode changes seem ignored**: an App Rule for the app overrides the global mode.
- **App Rule ignored for Chrome, Firefox or Sublime Text on GNOME Wayland**: IBus reports those
  apps only as an anonymous mutter relay, so the engine has no name to match. Install the
  companion GNOME Shell extension (`data/gnome-shell-extension/README.md`), then log out and back
  in — editing or enabling it in place is not enough.
- **Something else**: enable _Ghi log gỡ lỗi_ (GUI _Kiểu gõ_ tab → _Chẩn đoán_, or
  `debug_log = true` in `config.toml`), reproduce the issue, and attach
  `~/.cache/ibus-mangrs/debug.log` — the engine logs every focus change,
  capability, keystroke decision and delete/inject with millisecond timestamps
  cross-referenceable with `journalctl`. Never shares your typed text.

## Testing

```sh
cargo test                      # 227 unit tests, including the 678-case golden table
cargo clippy --all-targets
```

Live test against a real daemon without installing:

```sh
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

## Architecture

- `src/server.rs` — D-Bus, message filter, property menu, hotkey, config watch, key transports
- `src/engine.rs` — state machine: display modes, App Rules, macros, restore, verify/repair
- `src/bamboo_wrapper.rs` — the only code touching `bamboo-core`; computes its own output delta
- `src/uinput.rs` — virtual keyboard: paced Backspaces, selection, Ctrl+V (US-QWERTY key table)
- `src/paste.rs` — clipboard Unicode channel for the uinput modes (debounced restore)
- `src/macros.rs` — macro table + custom dictionary
- `src/backup.rs` — one-file JSON backup/restore of settings, macros and the dictionary
- `src/golden.rs` — 678 keystroke→text golden cases
- `src/ibus_prop.rs` — IBus property-menu encoding; `src/keymap.rs` — keysym decoding
- `src/debug_log.rs` — opt-in debug log at `~/.cache/ibus-mangrs/debug.log`
- `src/config.rs` — TOML settings shared by engine + GUI; `src/gui.rs` — GTK4 window (`gui` feature)

## License

- [**MăngRS**](https://github.com/fundokukiri/ibus-mangrs) is licensed under the [GPL-3.0 License](LICENSE).
- [**IBus Bamboo:**](https://github.com/BambooEngine/ibus-bamboo) Copyright (C) 2018 Luong Thanh Lam. [GPL-3.0 License](https://github.com/bambooengine/ibus-bamboo?tab=GPL-3.0-1-ov-file)
- [**bamboo-core (Rust):**](https://github.com/bamboo-vi/bamboo-core) Vietnamese input algorithm library written in Rust. [MIT License](https://github.com/bamboo-vi/bamboo-core?tab=MIT-1-ov-file)
- [**bamboo-core (Go):**](https://github.com/BambooEngine/bamboo-core) Copyright (C) 2018 Luong Thanh Lam. [MIT License](https://github.com/BambooEngine/bamboo-core?tab=MIT-1-ov-file)
