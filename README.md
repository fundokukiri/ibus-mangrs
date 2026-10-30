# MăngRS

Vietnamese Input Method for IBus, written in Rust on [`bamboo-core`](https://crates.io/crates/bamboo-core).

---

## Requirements

- Arch Linux (tested) or distro with IBus ≥ 1.5
- `ibus`, `gtk4` (for settings window)
- Rust ≥ 1.75 to build
- `/dev/uinput` readable/writable for real Backspace key injection (manual one-time setup: udev rule `/etc/udev/rules.d/80-uinput.rules` + `input` group — see `docs/install.md`); without it engine falls back to protocol delete for clients with surrounding text, while other clients won't be able to type marks.

## Build

```sh
cargo build --release --features gui
```

## Install

Build + install + restart IBus in one step:

```sh
./scripts/autoinstall.sh
```

Or manually:

```sh
sudo ./scripts/install.sh
```

Script copies:

| File | Destination |
|------|-------------|
| `target/release/ibus-engine-ibus-mangrs` | `/usr/lib/ibus-mangrs/ibus-engine-ibus-mangrs` |
| `data/ibus-mangrs.xml` | `/usr/share/ibus/component/ibus-mangrs.xml` |

Then:

1. `ibus restart`
2. **Settings → Keyboard → Input Sources → +** → *Vietnamese* → **MăngRS**
   (or `ibus engine ibus-mangrs`)
3. `Super+Space` to switch IME

Uninstall: `sudo ./scripts/uninstall.sh` (keeps config in `~/.config/ibus-mangrs/`).

## Usage

| Action | How |
|--------|-----|
| Toggle Vietnamese | Input source menu → *Enable Vietnamese*, or `Super+Space` |
| Open settings window | Input source menu → *Shortcuts…*, or `ibus-engine-ibus-mangrs -gui` |
| Change input method | Menu → *Input Method* (Telex / VNI / VIQR / Microsoft layout), or GUI |
| Change charset | Menu → *Charset* (11 charsets), or GUI |
| Change display mode | Menu → *Display* (NonPreedit / Preedit / Surrounding text), or GUI, or hotkey |
| Display mode hotkey | GUI → *Cycle Mode Hotkey* (e.g. `Control+Alt+v`) |
| Quick settings | Menu → *Quick Settings* (macros, auto-capitalize, free tone, standard tones, hide underline, ...) |
| Spell check & dictionary | Menu → *Spell Check & Dictionary* |
| Type Vietnamese | Telex: `tieengs` → `tiếng`; VNI: `tie6ng1` → `tiếng` |
| Type English | Disable IME, or type normally — invalid words revert to raw keystrokes |

Config lives at `~/.config/ibus-mangrs/config.toml` — engine hot-reloads in ~1 second, no `ibus restart` needed. Full option details: `docs/settings.md`; display modes & App Rules: `docs/modes.md`; troubleshooting: `docs/troubleshooting.md`.

## Debugging

Enable **Debug Logging** in GUI (General tab, or `debug_log = true` in `~/.config/ibus-mangrs/config.toml`), then reproduce the issue. Engine logs every focus change, capability, SetSurroundingText, keystroke and delete/inject decision to `~/.cache/ibus-mangrs/debug.log` (ms since epoch — cross-referenceable with `journalctl`). Attaching the log file with a bug report is the fastest way to reproduce "intermittent" bugs.

## Testing

```sh
cargo test                      # 133 unit tests, including 480 golden cases keystroke→text
cargo clippy --all-targets
```

Live test on real daemon without install:

```sh
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

Driving the engine via `gdbus` (string arguments must be `'like-this'`) and reading signals:

## Architecture

- `src/server.rs` — D-Bus, message filter, property menu, hotkey, config watch, injection transport
- `src/engine.rs` — state machine: 3 display modes, App Rules, macro, restore, verify/repair
- `src/bamboo_wrapper.rs` — only place touching `bamboo-core`; self-computes delta (fixes Backspace)
- `src/uinput.rs` — single-key virtual keyboard (Backspace) for clients swallowing protocol delete
- `src/macros.rs` — macro table + dictionary
- `src/golden.rs` — 480 cases keystroke → text
- `src/ibus_prop.rs` — IBus property menu encoding
- `src/config.rs` — TOML shared by engine + GUI
- `src/gui.rs` — GTK4 6 tabs (feature `gui`)
- `CLAUDE.md` — pointer to `AGENTS.md` (agent working rules, not public)

## License

MIT. Dependency `bamboo-core` (MIT) — see `NOTICE.md`.