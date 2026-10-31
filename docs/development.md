# Development

## Layout

```
src/server.rs          D-Bus, message filter, property menu, hotkey, config watch, key transports
src/engine.rs          state machine: display modes, App Rules, macros, restore, verify/repair
src/bamboo_wrapper.rs  the only code touching bamboo-core; computes its own output delta
src/uinput.rs          virtual keyboard: paced Backspaces, selection, Ctrl+V (US-QWERTY table)
src/paste.rs           clipboard Unicode channel for the uinput modes (debounced restore)
src/spellcheck.rs      bundled Vietnamese word list (advisory lookups)
src/macros.rs          macro table + custom dictionary
src/golden.rs          480 keystroke→text golden cases
src/ibus_prop.rs       IBus property-menu encoding
src/keymap.rs          keysym decoding
src/config.rs          TOML settings shared by engine + GUI
src/gui.rs             GTK4 settings window (gui feature)
```

The engine core comes from the `bamboo-core` crate on crates.io. The `bamboo-core/` submodule is
pinned to the same version for source browsing; it is not used by the build.

## Build, test, lint

```sh
cargo build --release --features gui
cargo test                      # 164 unit tests, including the 480-case golden table
cargo clippy --all-targets      # must stay clean
```

The `gui` feature pulls in GTK4 ≥ 4.8; everything builds without it for engine-only work.

## Test tooling

`test/README.md` documents the automation used to verify real-app behavior:

- `typing_tool.py` types a hard-word workload through the kernel (`/dev/uinput`) into any focused
  field, cycles all display modes and verifies the result code-point-for-code-point.
- `keys-test` exercises the backspace/delete/caret paths.
- `perf_tool.py` measures per-keystroke latency end to end.
- `expected-gen` replays keystrokes through the real engine pipeline to generate the expected
  output files.

## Live engine without installing

```sh
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

Debug logging (`debug_log = true`) writes engine decisions to `~/.cache/ibus-mangrs/debug.log`
with millisecond timestamps cross-referenceable with `journalctl`.

## Packaging

- `scripts/install.sh` / `uninstall.sh` — system install (engine, component XML, udev rule,
  `input` group).
- `PKGBUILD` — Arch package; the udev rule goes to `/usr/lib/udev/rules.d/`, the word list to
  `/usr/share/ibus-mangrs/`.
- Releases are tagged `ibus-mangrs-<version>`; `CHANGELOG.md` follows the release history.

## Conventions

- Conventional Commits; one logical change per commit; every commit builds and passes tests.
- The engine must never panic while typing — a crash breaks typing desktop-wide.
- Debug logs never contain typed text.
