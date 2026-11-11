<p align="center">
  <img src="data/logo.png" alt="MăngRS logo" width="360">
</p>

# MăngRS

[Tiếng Việt](vi/README-vi.md) | **English**

A Vietnamese input method engine for **IBus**, written in **Rust** on top of
[`bamboo-core`](https://crates.io/crates/bamboo-core). 9 input methods, 11 output charsets,
9 display modes, a GTK4 settings window, macros, an emoji picker at the caret and per-app
rules.

> Developed and tested on **Arch Linux** (IBus 1.5.34, GNOME 51, Wayland; X11 smoke-tested).
> The only hard requirements are IBus ≥ 1.5 and GTK4 + libadwaita — it should work wherever
> IBus works. Reports from other distros are welcome.

## Features

- **9 input methods** — Telex (`tieengs` → `tiếng`), Telex 2, Telex W, Telex + VNI,
  Telex + VNI + VIQR, VNI (`tie6ng1`), VNI (French keyboard), VIQR and the Microsoft layout
  (`ti3ng8` → `tiếng`) — with **11 output charsets** (Unicode, VNI, VIQR, TCVN3, VISCII, ...).
- **9 display modes** (table below), settable per application through **App Rules**.
- **Flexible typing options**: modern tone placement (`hòa`) or traditional (`hoà`), free tone
  marking (`hoangf` → `hoàng`), auto-correction of invalid syllables, auto-restore of foreign
  words, raw-key bypass for URLs/emails/paths, `w` → `ư`, `--` → `—`, auto-capitalize,
  double space → period.
- **Three Backspace behaviors on tone-marked words**: tone first (`hạ` → `ha`), whole screen
  character (`việt` → `việ`), or the unikey walk (`hạ` → `hã` → `hà` → ...).
- **Per-key marks in the Flow/Turbo modes** (on by default): `ee` shows `ê` the moment it
  lands instead of holding raw keys until the word ends. A click or a focus change never
  loses the half-typed word — the preedit commits what it shows, including a shorthand fix
  like `ddc` → `đc`.
- **Macros** with `$TIME` / `$DATE` expansion and smart capitalization; a **custom
  dictionary** of words that are never auto-restored.
- **Emoji**: typing `:name:` rewrites to the emoji (off by default), or an assignable hotkey
  opens a picker at the caret — 1,940 built-in emoji with Vietnamese and English keywords;
  your own `name = "glyph"` pairs in `emoji.toml` extend the table.
- **Editable panel menu** (folders, functions, radio groups, separators) and 14 assignable
  shortcut functions, alongside the 4 built-in shortcut slots (all unassigned by default).
- **GTK4 + libadwaita settings window** (12 pages, Vietnamese or English — a row on
  _Other settings_ switches the language): every change hot-reloads within ~1 second, and
  all settings back up into one JSON file.

## Display modes

| Display mode (label) | Config value    | How it works                                                     
| -------------------- | --------------- | ---------------------------------------------------------------- | 
| Underline (preedit)  | `preedit`       | word shows inline with underline, commits at word end            | 
| No underline         | `nonpreedit`    | every key commits, the tail is rewritten on marks                | 
| Surrounding text     | `surrounding`   | non-preedit, verified against the app's own text report          | 
| Flow (uinput)        | `smooth`        | raw keys forwarded; real Backspaces erase the tail at word end   | 
| Turbo (uinput)       | `supersmooth`   | Flow without the safety check before rewriting                   | 
| Select (uinput)      | `select`        | Shift+← selects the tail, Delete erases it, word commits         | 
| Forward Backspace    | `forwardkeyevent` | the tail is deleted with Backspace events forwarded via IBus   | 
| Forward Selection    | `forwardselect` | the tail is selected with Shift+← and replaced by the commit     | 
| OFF                  | `off`           | Vietnamese off, macros and auto-capitalize still work            | 

The labels are the ones the engine's menu and settings window show; the config value is what
`config.toml` and App Rules accept.

Switch modes from the input-source menu, the settings window's _Display mode_ row, a cycle
hotkey, or per app with App Rules. The three uinput modes press real keys through
`/dev/uinput`: they need write access to that device and fall back to preedit without it
(Select also needs a surrounding-text report from the app). They run on both X11 and GNOME
Wayland. The two Forward modes need neither `/dev/uinput` nor a surrounding-text report —
their delete or selection travels as forwarded key events. A config or rule still carrying
the removed `auto` or `defaulttyping` value migrates automatically (to `preedit` and `off`).

## Requirements

| Distro          | Packages (the one-line installer installs them)               |
| --------------- | ------------------------------------------------------------- |
| Arch            | `ibus` `gtk4` `libadwaita` `dbus` + `gcc` `make` `pkgconf`    |
| Debian / Ubuntu | `ibus` `libgtk-4-dev` `libadwaita-1-dev` `libdbus-1-dev` + build-essential |
| Fedora          | `ibus` `gtk4-devel` `libadwaita-devel` `dbus-devel` + `gcc` `make` |

The installer detects the distro (apt / pacman / dnf / zypper), installs only what is missing
and sets up Rust through rustup. GTK 4.10+ and libadwaita 1.5+ are required (Ubuntu 22.04
ships 4.6). Only Arch is tested. IBus must be the system's input-method framework (on GNOME
it is).

## Install

### One line (online)

Downloads the source, builds and installs; missing dependencies are installed too, so only
`curl` (or `wget`) and `sudo` are needed up front. The `/dev/uinput` udev rule and the `input`
group are set up, build-only packages installed by the run are removed afterwards, and
re-running installs nothing twice. `--demo` previews the whole run without touching anything:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
# or, without curl:
wget -qO- https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
```

(Piping a script from the network to `sh` runs code from the network — review it first.)

To remove everything again:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/uninstall.sh | sh
```

Add `PURGE_CONFIG=1` before the command to also delete `~/.config/ibus-mangrs`; the plain
run keeps your config. `--demo` previews the removal without deleting anything.

### From a checkout

```sh
# Arch / Manjaro / EndeavourOS
sudo pacman -S --needed gcc pkgconf ibus dbus gtk4 libadwaita desktop-file-utils

# Debian / Ubuntu / Mint / Pop!_OS
sudo apt install gcc libc6-dev pkg-config ibus libdbus-1-dev libgtk-4-dev \
    libadwaita-1-dev desktop-file-utils

# Fedora / RHEL / CentOS
sudo dnf install gcc pkgconf-pkg-config ibus dbus-devel gtk4-devel \
    libadwaita-devel desktop-file-utils

# openSUSE
sudo zypper install gcc pkg-config ibus dbus-1-devel gtk4-devel \
    libadwaita-devel desktop-file-utils

# Rust (all distros)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo build --release --features gui
sudo ./scripts/install.sh
ibus restart
```

The install script copies the engine to `/usr/lib/ibus-mangrs/`, registers the IBus component,
installs the udev rule and adds your user to the `input` group (next login). Uninstall:
`sudo ./scripts/uninstall.sh` (keeps your config). On Arch: `makepkg -si`.

### /dev/uinput permissions (manual fallback)

The engine presses **real Backspace keys** through `/dev/uinput` for apps that cannot delete
text over IBus; without write permission those apps cannot type tone marks. Create
`/etc/udev/rules.d/80-uinput.rules` (shipped as `data/80-uinput.rules`):

```
KERNEL=="uinput", MODE="0660", GROUP="input"
```

```sh
sudo modprobe uinput
sudo usermod -aG input $USER                    # next login
sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput
echo uinput | sudo tee /etc/modules-load.d/uinput.conf   # load at every boot
```

Verify: `ls -l /dev/uinput` must show `crw-rw---- root input`.

## Enable the input source

1. `ibus restart` (after installing or upgrading the binary).
2. **Settings → Keyboard → Input Sources → +** → _Vietnamese_ → **MăngRS**
   (or `ibus engine ibus-mangrs`).
3. `Super+Space` switches between input sources (GNOME's own shortcut); the gear next to the
   engine opens the settings window.

## Usage

| Action                     | How                                                                                       |
| -------------------------- | ------------------------------------------------------------------------------------------ |
| Toggle Vietnamese          | input-source menu → _Vietnamese typing_, or an assignable hotkey (unassigned by default)   |
| Open settings              | menu → _Settings_, or `ibus-engine-mangrs -gui`                                            |
| Change input method        | menu → _Typing method_ (9 presets), or GUI                                                 |
| Change charset             | menu → _Charset_ (11 charsets), or GUI                                                     |
| Change display mode        | menu → _Display mode_, GUI (all 9), or the cycle hotkey                                    |
| Type an emoji              | type `:name:` (enable in the GUI _Emoji_ page), or the emoji hotkey opens the picker       |
| Pick a per-app rule        | the app-rule hotkey opens a picker at the caret: the app and its display mode              |
| Assign a shortcut          | GUI _Shortcuts_ page (click a row, press the combo)                                        |
| Type English / code        | just type — invalid words revert to raw keystrokes, URLs/emails/paths stay raw             |
| Per-app display mode       | GUI _Per-app rules_ page (below)                                                           |

All four built-in hotkey slots (toggle Vietnamese, cycle display mode, app-rule picker,
emoji picker) ship unassigned — set them on the _Shortcuts_ page. `Super+Space` is GNOME's
input-source switcher, not a MăngRS shortcut.

## App Rules (per-app display mode)

A rule forces one display mode for one application and overrides the global mode; the first
match wins, so put specific rules above general ones. Matching is a case-insensitive
substring of the app's name (`term` also matches `xterm`); the special pattern `none`
targets apps that never report a name. Set rules on the GUI _Per-app rules_ page (pick an
app, pick a mode), through the app-rule picker hotkey, or by hand in
`~/.config/ibus-mangrs/config.toml` (hot-reloaded within ~1 s):

```toml
[[app_rules]]
pattern = "chrome"
mode = "preedit"

[[app_rules]]
pattern = "sublime_text"
mode = "surrounding"
```

Mode values: `preedit` (default), `nonpreedit`, `surrounding`, `smooth`, `supersmooth`,
`select` (these three press real keys through `/dev/uinput`), `forwardkeyevent`,
`forwardselect`, `off`.

No rules ship with the engine. It does carry a built-in safeguard for known
delete-dropping clients (Chrome, Chromium, Edge, Brave, Vivaldi and Electron apps such as
VS Code, Slack, Discord): if a rewriting mode is active there, the engine types them in
preedit instead of duplicating text. Give such an app a different mode by adding your own
rule.

Chrome and Firefox on GNOME Wayland are reported to IBus only as an anonymous relay:
matching them by name needs the companion GNOME Shell extension
(`data/gnome-shell-extension/README.md`), then a logout/login.

## Configuration files

```
~/.config/ibus-mangrs/config.toml     # all settings, hot-reloaded within ~1 s
~/.config/ibus-mangrs/macros.toml     # one key=expansion per line, $TIME / $DATE
~/.config/ibus-mangrs/emoji.toml      # name = "glyph" pairs
~/.config/ibus-mangrs/dictionary.txt  # words never auto-restored
```

## Troubleshooting

- **Engine not listed in Input Sources**: run `ibus write-cache --system` then `ibus-daemon -drx`;
  make sure `/usr/share/ibus/component/ibus-mangrs.xml` exists.
- **Marks do not appear in some apps** (Sublime, XWayland clients): `/dev/uinput` is missing or
  not writable — see the permissions section above.
- **Mode changes seem ignored**: an App Rule for the app overrides the global mode.
- **X11/XIM apps (JetBrains) misbehave**: try the two _X11/XIM compatibility_ switches in the
  GUI _Tuning_ page.

## License

- [**MăngRS**](https://github.com/fundokukiri/ibus-mangrs) — [GPL-3.0 License](LICENSE).
- [**IBus Bamboo:**](https://github.com/BambooEngine/ibus-bamboo) Copyright (C) 2018 Luong Thanh Lam, [GPL-3.0 License](https://github.com/bambooengine/ibus-bamboo?tab=GPL-3.0-1-ov-file).
- [**bamboo-core (Rust):**](https://github.com/bamboo-vi/bamboo-core) Vietnamese input algorithm library in Rust, [MIT License](https://github.com/bamboo-vi/bamboo-core?tab=MIT-1-ov-file).
- [**bamboo-core (Go):**](https://github.com/BambooEngine/bamboo-core) Copyright (C) 2018 Luong Thanh Lam, [MIT License](https://github.com/BambooEngine/bamboo-core?tab=MIT-1-ov-file).
