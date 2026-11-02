# End-to-end functional tests (local)

Black-box functional tests for the MăngRS engine. They run the **real release
binary** on a **private D-Bus session bus** and drive it exactly the way
ibus-daemon and a focused client would: calling `org.freedesktop.IBus.Engine`
methods and asserting on the engine's signal stream — the same contract real
applications see.

They complement `cargo test` (209 unit/golden tests, no D-Bus) with the
protocol level: wire encodings, method dispatch, signal ordering, and the
config/property plumbing that only exists across D-Bus.

## Run

```sh
test/e2e/run.sh                            # every feature file
test/e2e/run.sh preedit                    # one feature (tests/e2e_preedit.rs)
MANGRS_E2E_FILTER=word test/e2e/run.sh     # substring filter inside the cases
```

Each feature lives in its own `tests/e2e_<feature>.rs`, so a single feature is
a single cargo target: `run.sh <feature>` compiles and runs just that one.
Feature files: `preedit`, `nonpreedit`, `surrounding`, `input_methods`,
`macros`, `word_end`, `tone_style`, `special_keys`, `typing_features`,
`charsets`, `client_lifecycle`, `config`, `safety`, `modes`. The shared
harness (engine session, client simulation, signal parsing) lives in
`tests/common/mod.rs`.

The suites are `#[ignore]`d, so plain `cargo test` never runs them (CI/sandbox
safe). `run.sh` builds `target/release/ibus-engine-ibus-mangrs` when missing,
wraps everything in `dbus-run-session`, sets the `MANGRS_E2E`/`MANGRS_E2E_BIN`
gates, and runs the selected targets with `--test-threads=1`. Exit code is 0
only when every selected case passes.

Requirements: the `dbus` crate (already a project dependency) and
`dbus-run-session` (standard on a GNOME system). No Python, no extra packages.
If the cargo build fails with `unknown proxy name: zcode`, run
`env -u RUSTC_WRAPPER cargo build --release` once by hand — `run.sh` retries
without the wrapper automatically.

## What is covered

- Typing correctness for all four input methods over the wire: Telex, VNI,
  VIQR (including literal mark keys, unikey convention), Microsoft layout
  (digit inserts, bracket vowels), plus uppercase keystrokes.
- All protocol display modes: Preedit (underline stream with the
  `IBusAttribute`, `PREEDIT_COMMIT` flag, commit on space, unikey Backspace),
  NonPreedit (per-key commits + `DeleteSurroundingText` rewrites, protocol
  backspace, reported client), Surrounding (verified typing; a stale report
  must never tear down the word).
- The two non-protocol modes that are safe to drive over the wire: OFF (raw
  keys, optional macro expansion) and Default Typing (fully inert, no
  signals).
- Word-end behaviors: macro expansion (visible in the preedit, committed on
  space, `$TIME`/`$DATE` with the configured formats, capitalization that
  follows the key case), abbreviations (`ddc` keeps the horn; off restores
  the raw reversion), auto-restore of non-Vietnamese words, dictionary words
  never restored (`windowf` -> `windờ` with the bent form listed), digits and
  underscore as word terminators that always reach the app.
- Typing features: URL/email/path bypass, auto-capitalize, `--` em-dash,
  double-space period, modern vs traditional tone placement, free tone
  marking off, `w2u` (nonstart / everywhere), bracket keys (nonstart /
  everywhere).
- Charsets: `charset = "tcvn3"` encodes the commit while the preedit stays
  Unicode (expected value pinned from a verified run).
- Client lifecycle: `Reset` commits nothing (the client owns the preedit
  commit), `FocusOut` is a wire no-op, `Disable` forwards keys.
- Config: mid-session hot-reload via the config file mtime; the property menu
  (`RegisterProperties` on focus, `PropertyActivate` persists and emits
  `UpdateProperty`); the mode-cycle hotkey; per-app rules overriding the
  default mode; the debug log writing decisions when enabled.
- Capability fallback: a client without `CAP_PREEDIT_TEXT` types via commits.
- Safety invariants: a mutter-relayed client (`gnome-shell`) without
  surrounding-text reports must never receive a protocol delete and must
  never trigger key injection — raw keys pass through untouched; a uinput
  mode requested on a relay must resolve to preedit (nothing pressed).

## What is deliberately NOT covered

- **No live uinput injection.** Smooth / SuperSmooth / Select rewrites and
  `RewriteByBackspace` press real keys through `/dev/uinput`, which land in
  whatever window has focus on the real desktop. Those paths are covered by
  the unit tests with fake transports (`server.rs` tests, `golden.rs` fault
  matrix). The e2e cases therefore always either report surrounding text
  (protocol deletes only) or use modes/clients where no delete transport is
  engaged.
- No GUI end-to-end (the settings window is covered by the widget round-trip
  unit test; driving GTK interactively is out of scope here).

## Isolation guarantees

- Everything runs inside `dbus-run-session` — the system IBus session is never
  touched (no `ibus restart`, no component files, no `/usr`).
- Each case gets its own temp `XDG_CONFIG_HOME` / `XDG_CACHE_HOME`, removed
  when the case ends; the user's real config is never read or written.
- No typed text is written anywhere persistent; assertions live only in the
  process output.
