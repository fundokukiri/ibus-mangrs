<p align="center">
  <img src="data/logo-no-bg.png" alt="MăngRS logo" width="360">
</p>

# MăngRS

A Vietnamese input method engine for **IBus**, written in **Rust** on top of
[`bamboo-core`](https://crates.io/crates/bamboo-core). Telex, VNI, VIQR and Microsoft layouts,
three classic display modes plus three real-key (uinput) modes, a GTK4 settings window, macros,
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
- **Typing options**: modern tone placement (`hòa` vs traditional `hoà`), free tone marking
  (`hoangf` → `hoàng`), spell check backed by a bundled 38 350-word Vietnamese word list,
  auto-restore of non-Vietnamese words, raw-key bypass after URL/email/path characters
  (`/ : @ . _ ~ = # & % ?`), `w` → `ư`, `[`/`]` → `ơ`/`ư`, `--` → `—`, auto-capitalize,
  double space → period.
- **Macros** with `$TIME` / `$DATE` expansion and smart capitalization; shorthand keys that are
  valid Vietnamese words never expand while you are typing that word normally.
- **Custom dictionary**: words that must never be auto-restored.
- **GTK4 settings window** (6 tabs, Vietnamese); every change hot-reloads within ~1 second.
- **Backup/restore** of all settings as one JSON file.
- 164 unit tests including a 480-case keystroke→text golden table.

## Display modes

| Mode | How it works | Best for |
|---|---|---|
| Preedit (default) | word shows inline with underline, commits at word end | widest compatibility |
| Non-preedit | every key commits immediately, tail rewritten on marks | Firefox, GTK apps |
| Surrounding text | non-preedit + verifies against the app's own text report | Sublime Text |
| Smooth (uinput) | raw keys forwarded while typing, latin tail erased with real Backspaces at word end | terminals, quirky apps (X11) |
| Super Smooth (uinput) | same, without the safety check before rewriting | fast typing (X11) |
| Select (uinput) | Shift+← selects the tail, Delete erases it, word commits | apps where Backspaces misbehave (X11) |
| OFF | Vietnamese off, macros still work | mixed Vietnamese/English sessions |
| Default Typing | engine fully inert, every key forwarded | apps that resent any processing |

Switch modes from the input-source menu, the settings window, an assignable cycle hotkey, or per
app with **App Rules**. The uinput modes need `/dev/uinput` and fall back to preedit wherever
real-key injection cannot reach the application (on GNOME Wayland they resolve to preedit; they
are intended for X11 sessions).

## Requirements

| Distro | Packages |
|---|---|
| Arch | `ibus` `gtk4` (build: `rust`) |
| Debian / Ubuntu | `ibus` `libgtk-4-dev` (build: `cargo rustc`) |
| Fedora | `ibus` `gtk4-devel` (build: `rust cargo`) |
| openSUSE | `ibus` `gtk4-devel` (build: `rust cargo`) |

The exact package names above are the standard ones but are **unverified on those distros** —
only the Arch line is tested. IBus must be your active input-method framework (on GNOME it is).

## Build

```sh
cargo build --release --features gui
```

The engine core comes from the `bamboo-core` crate on crates.io. The `bamboo-core/` directory in
this repository is a **git submodule** pinned to the same `v0.5.3` tag — kept for source browsing
and audits, not used by the build; pull it in with `git submodule update --init` if you want it.

## Install

### Binary packages (from the GitHub release)

Download the package for your distro and the `SHA256SUMS` file from
[releases](https://github.com/fundokukiri/ibus-mangrs/releases), verify, then install with your
package manager:

```sh
sha256sum -c SHA256SUMS
sudo pacman -U ibus-ibus-mangrs-*.pkg.tar.zst        # Arch
sudo apt install ./ibus-mangrs_*_amd64.deb           # Debian / Ubuntu
sudo dnf install ./ibus-mangrs-*.x86_64.rpm          # Fedora / openSUSE
```

After installing, run `ibus restart` once.

### One-line install (online)

Downloads the source, builds it and installs — needs `curl`, Rust (`cargo`), `ibus`, `gtk4` and
`sudo`; the `/dev/uinput` udev rule and `input` group are set up for you:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/dev/scripts/install-online.sh | sh
```

(Piping a script to `sh` runs code from the network — review it first if that matters to you.)

### From a local checkout

One step (build + install + restart IBus):

```sh
./scripts/autoinstall.sh
```

Or manually:

```sh
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
2. **Settings → Keyboard → Input Sources → +** → *Vietnamese* → **MăngRS**
   (or `ibus engine ibus-mangrs`).
3. `Super+Space` switches input sources; the gear next to the engine opens the settings window.

## Usage

| Action | How |
|---|---|
| Toggle Vietnamese | input-source menu → *Enable Vietnamese*, or `Super+Space` |
| Open settings window | input-source menu → *Cài đặt…*, or `ibus-engine-ibus-mangrs -gui` |
| Change input method | menu → *Input Method* (Telex / VNI / VIQR / Microsoft), or GUI |
| Change charset | menu → *Charset* (11 charsets), or GUI |
| Change display mode | menu → *Display* (8 modes), or GUI, or the cycle hotkey |
| Display mode cycle hotkey | GUI General tab → *Cycle Mode Hotkey* (e.g. `Control+Alt+v`) |
| Quick settings | menu → *Quick Settings* (macros, auto-capitalize, free tone marks, hide underline, …) |
| Spell check & dictionary | menu → *Spell Check & Dictionary* |
| Type English / code | just type — invalid words revert to raw keystrokes; URLs/emails/paths stay raw |
| Per-app display mode | GUI Applications tab → `process-name=mode`, or the dropdown of seen apps |

## Configuration files

```
~/.config/ibus-mangrs/config.toml     # all settings, hot-reloaded within ~1 s
~/.config/ibus-mangrs/macros.toml     # one key=expansion per line, $TIME / $DATE
~/.config/ibus-mangrs/dictionary.txt  # words never auto-restored
~/.config/ibus-mangrs/vn-words.txt    # optional override of the bundled spell-check word list
```

## Troubleshooting

- **Engine not listed in Input Sources**: run `ibus write-cache --system` then `ibus-daemon -drx`;
  make sure `/usr/share/ibus/component/ibus-mangrs.xml` exists.
- **Marks do not appear in some apps** (Sublime, XWayland clients): `/dev/uinput` is missing or
  not writable — see the permissions section above.
- **Mode changes seem ignored**: an App Rule for the app overrides the global mode.
- **Something else**: enable **Debug Logging** (GUI General tab, or `debug_log = true` in
  `config.toml`), reproduce the issue, and attach `~/.cache/ibus-mangrs/debug.log` — the engine
  logs every focus change, capability, keystroke decision and delete/inject with millisecond
  timestamps cross-referenceable with `journalctl`. Never shares your typed text.

## Testing

```sh
cargo test                      # 164 unit tests, including the 480-case golden table
cargo clippy --all-targets
```

Live test against a real daemon without installing:

```sh
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

Automated typing/latency tooling that types through the kernel into any focused field and
verifies the result: see `test/README.md`.

## Architecture

- `src/server.rs` — D-Bus, message filter, property menu, hotkey, config watch, key transports
- `src/engine.rs` — state machine: display modes, App Rules, macros, restore, verify/repair
- `src/bamboo_wrapper.rs` — the only code touching `bamboo-core`; computes its own output delta
- `src/uinput.rs` — virtual keyboard: paced Backspaces, selection, Ctrl+V (US-QWERTY key table)
- `src/paste.rs` — clipboard Unicode channel for the uinput modes (debounced restore)
- `src/spellcheck.rs` — bundled Vietnamese word list (advisory lookups)
- `src/macros.rs` — macro table + custom dictionary
- `src/golden.rs` — 480 keystroke→text golden cases
- `src/ibus_prop.rs` — IBus property-menu encoding; `src/keymap.rs` — keysym decoding
- `src/config.rs` — TOML settings shared by engine + GUI; `src/gui.rs` — GTK4 window (`gui` feature)

More detail: `ARCHITECTURE.md` (protocol facts, engine invariants, delete-transport safety rules).

## Documentation

| Guide | Content |
|---|---|
| [docs/installation.md](docs/installation.md) | prerequisites per distro, one-line and manual install, `/dev/uinput` permissions, upgrade |
| [docs/usage.md](docs/usage.md) | all eight display modes, input methods, typing behavior, macros, dictionary, App Rules, config files |
| [docs/faq.md](docs/faq.md) | common problems and how to report a bug with a debug log |
| [docs/development.md](docs/development.md) | code layout, build/test/lint, test tooling, packaging |

## License

MăngRS is **MIT** — see `LICENSE`. Third-party attributions: `NOTICE.md` and `data/README.md`.

- [`bamboo-core`](https://crates.io/crates/bamboo-core) (MIT) — the Vietnamese input algorithm,
  a Rust extraction of ibus-bamboo's logic.
- `data/vn-words.txt` — the spell-check word list comes from Ho Ngoc Duc's *Free Vietnamese
  Dictionary Project* and is distributed as a separate data file under the **GPL**; the source
  and the rationale are documented in `data/README.md`. Deleting the file only disables the
  dictionary check.
