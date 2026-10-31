# Bamboo-core audit

What the `bamboo-core` crate (0.5.3) ships vs. what ibus-mangrs implements on
top of it, from the actual code. The build uses the crates.io crate; the
`bamboo-core/` submodule is pinned to the same version for source browsing.
The only module that talks to bamboo-core is `src/bamboo_wrapper.rs`; the
engine, server and GUI go through `BambooWrapper`.

## What bamboo-core ships

- **Committing engine** (`Engine`): `new`/`with_config`/`from_preset`,
  `process_key` and `process_key_edit` (per-keystroke composition, the edit
  variant returns an `EditDelta` suffix edit), `process`/`process_str` for
  whole strings, `commit`, `output_str`, `get_processed_str` with
  `OutputOptions` (tone-less, mark-less, lowercase, raw), `reset`.
- **Deletion**: `undo_last_key(_with)` (keystroke-level undo via a snapshot
  stack) and `remove_last_output_char` (grapheme-level backspace).
- **Word restore**: `restore_last_word_as`, `rebuild_from_text` (load existing
  text as if typed).
- **Validity**: `is_valid_word` / `is_valid_prefix` (CVC phonotactic check).
- **Input methods**: `telex`, `vni`, `viqr`, `microsoft_layout` plus
  `telex_2`, `telex_vni`, `telex_vni_viqr`, `vni_french_layout`, `telex_w`,
  and custom rule-driven methods via `from_definition`.
- **Config**: `Config`/`ConfigBuilder` — `free_tone_marking`, `tone_style`,
  `auto_correct`, `w2u_mode`, `bracket_mode`.
- **Charsets**: `encode_charset`/`decode_charset` over 17 `Charset` variants
  (Unicode, TCVN3, VNI, VIQR, VISCII, VPS, CP1258, BKHCM x2, Vietware x2,
  NFD/NCR and more).
- **Extras**: `warm_up` (deprecated since 0.3.4), `advanced` module (DFA and
  rule internals), `parallel` feature, `ffi` and `wasm` modules.

## What ibus-mangrs uses

`Engine::with_config`, `process_key_edit`, `output_str`, `is_valid_word`,
`reset`, `commit`, `rebuild_from_text`, `encode_charset`, the `telex`/`vni`/
`viqr`/`microsoft_layout` presets, `Mode::Vietnamese`, and the five `Config`
fields listed above (forwarded from `src/config.rs` `bamboo_config`).

## What ibus-mangrs implements itself, and why

- **Suffix edits**: bamboo's `EditDelta` is discarded (`let _ =
  process_key_edit(...)`). Deltas are relative to an internal buffer that is
  not refreshed after a backspace, so a keystroke after backspace reports a
  deletion for a character that is already gone (the "backspace then type eats
  a letter" bug). The wrapper computes its own suffix delta against its own
  `last_output` tracker.
- **Backspace**: bamboo's `undo_last_key` / `remove_last_output_char` are
  never called — the grapheme variant left the transform stack stale, so
  hidden transforms resurfaced nondeterministically on later keys
  (`mừi` instead of `mười`). The wrapper implements the unikey convention:
  pop the last raw key, `reset`, replay the remaining raw keys, diff outputs.
- **Poisoned-state guards**: an undone mark leaves state inside bamboo that
  resurfaces later (`too` + `l` → `toool`). The wrapper detects undo-in-word,
  sanity-checks post-undo deltas and falls back to rebuild-from-raw or plain
  append, and can override the output outright.
- **Word termination**: bamboo buffers punctuation and digits into the
  composition (and clears it on digits in Telex/VIQR), which would delete app
  text. The wrapper decides terminators itself and finishes the word.
- **Auto-restore**: bamboo's `restore_last_word_as` is unused; the wrapper's
  engine decides at word end (invalid syllable that differs from raw keys →
  commit the raw keys).
- **Legacy charsets**: bamboo only encodes; ibus-mangrs handles the fact that
  legacy encodings change character counts (VNI `â` → `aâ`) by rewriting the
  whole word instead of applying per-keystroke deltas.
- **Everything above the word engine** — IBus D-Bus transport, preedit,
  surrounding-text verification/repair, client rules, echo/stale-injection
  tracking, uinput injection, clipboard channel, macros, em dash, double
  space, auto-capitalize — is ibus-mangrs-only; bamboo-core is a pure text
  engine by design.

## Unused bamboo-core capabilities

- `warm_up` (deprecated; skipping is deliberate).
- The entire deletion subsystem (`undo_last_key(_with)`,
  `remove_last_output_char`, `remove_last_char`) — replaced by replay.
- `restore_last_word_as` — replaced by the wrapper's auto-restore.
- `EditDelta` consumption — produced, never read.
- `process`/`process_str` (one debug helper only),
  `get_processed_str`/`OutputOptions`, `Mode::English` (off-mode is the
  wrapper's own flag), `can_process_key`.
- Presets `telex_2`, `telex_vni`, `telex_vni_viqr`, `vni_french_layout`,
  `telex_w`, and custom methods via `from_definition`.
- Charsets `Bkhcm2`, `Utf8`, NCR variants and the C-string hex variants;
  `decode_charset` (encode-only use).
- `advanced` module, `parallel` feature, `ffi`/`wasm` modules, DFA
  introspection accessors, `Config::to_flags`/`from_flags`.

The unused surface is either redundant with a wrapper workaround (deletion,
restore, deltas) or irrelevant to an IBus daemon (wasm, ffi, parallel batch).
The unused presets are candidates if more input methods are ever requested.
