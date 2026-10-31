# Engine diagnosis — typing stability review

Scope: the typing engine only (input methods and display modes under `src/`).
UI, settings window, installer and telemetry are out of scope.

Method: read-only code and documentation analysis. No file was modified, no
test or build was run; every claim below cites the exact code it comes from.
Anything not directly verifiable from code is labeled "inferred" or "needs
live verification". Date: 2026-10-31.

## 1. Documents read

- Repo docs: `AGENTS.md`, `ARCHITECTURE.md`, `README.md`, `TODO.md`,
  `CHANGELOG.md`, `DEVLOG.md` (newest entries), `docs/open-questions.md`,
  `docs/troubleshooting.md`, `docs/modes.md`, `docs/gap-analysis.md`,
  `docs/bamboo-audit.md`.
- Engine source: `src/engine.rs` (full engine code; test module by name),
  `src/bamboo_wrapper.rs`, `src/server.rs`, `src/uinput.rs`, `src/paste.rs`,
  `src/macros.rs`, `src/keymap.rs`, `src/spellcheck.rs`, `src/ibus_text.rs`,
  `src/debug_log.rs`, `src/golden.rs`, `src/config.rs`, `src/main.rs`,
  `src/lib.rs`.
- Cross-reference: `bamboo-core/src/input_method/definitions.rs` (VIQR and
  Microsoft layout presets, submodule pinned to v0.5.3); test counts by grep.
- Skipped per scope: `gui.rs`, `backup.rs`, `ibus_prop.rs`, install scripts.

## 2. Architecture summary

Key path: ibus-daemon sends `ProcessKeyEvent(keyval, keycode, state)` on the
private IBus bus, the message filter in `server.rs` (`src/server.rs:348`)
handles it: config mtime check, mode-cycle hotkey, then `keymap::keysym_to_char`
decodes the keysym, `Engine::process_char` (`src/engine.rs:914`, protocol-free
state machine) feeds `BambooWrapper::process_char` (`src/bamboo_wrapper.rs:250`,
the only module touching bamboo-core), which returns
`Outcome::{Edit, Finish, PassThrough}`. The engine turns outcomes into
`Action`s; `emit_with` (`src/server.rs:776`) converts them into IBus signals
(`CommitText`, `UpdatePreeditText`, `DeleteSurroundingText`) or real key
injection through `/dev/uinput`, with the defer-commit-until-echo mechanism
that keeps D-Bus commits and compositor key events ordered.

Eight display modes: Preedit (default), NonPreedit (per-key commit, tail
rewrite), Surrounding (NonPreedit plus `SetSurroundingText` verification),
Smooth / SuperSmooth / Select (uinput: raw keys forwarded while typing, latin
tail erased with real Backspaces at word end), OFF (macros still run), Default
Typing (fully inert). Mode selection order: `config.toml` (hot-reloaded by
mtime within ~1 s), property-menu radios, cycle hotkey, per-app `[[app_rules]]`
on the prgname, then the fallback chain in `effective_mode()`
(`src/engine.rs:854-893`): no preedit capability falls to NonPreedit; uinput
modes fall back to Preedit on mutter-relayed contexts, without uinput, or
without an ST report; inline suggestions and delete-dropping clients resolve to
Preedit. The `word_mode` lock freezes the resolved mode for the whole word
(`src/engine.rs:897-906`) — intentional and covered by
`word_mode_locks_across_config_changes`.

Word buffer: the wrapper keeps `raw` (keystrokes typed), `last_output` (text it
believes is on screen; deltas are computed by the wrapper against this, never
trusting bamboo-core's `EditDelta`), `output_override` (poison guard after a
mark undo), `undo_in_word`, `abbrev_fixed`. Word termination is decided by the
wrapper (`is_word_terminator`, `src/bamboo_wrapper.rs:550`) before bamboo-core
sees the key. Backspace follows the unikey convention: pop the last raw key,
reset, replay (`src/bamboo_wrapper.rs:413-443`). Delete safety: protocol
deletes only for clients that actually reported surrounding text; real
Backspace injection otherwise; repair deletes forbidden on relayed contexts
(mutter segfaults). Focus changes drop buffer state via `clear_buffer_state()`.

Overall: the layering is clean (engine stays protocol-free), the test suite is
dense (181 tests for ~19k lines), and every historical crash class has a
regression test. The remaining problems cluster around: a word-terminator set
that does not know the active input method (VIQR and Microsoft layout break),
one delete-transport gate placed at a single call site, and some disk I/O on
the key path.

## 3. Issue list

Severity: Critical / Medium / Low. Confidence levels: "confirmed by code"
(both sides of the path read directly), "inferred" (logic path is closed but
not reproduced live), "needs live verification".

### BUG-01 — VIQR method cannot type any mark (feature fully broken)

- Severity: Critical.
- Location: `src/bamboo_wrapper.rs:263-278` (terminator branch),
  `src/bamboo_wrapper.rs:550-557` (`is_word_terminator`); cross-reference
  `bamboo-core/src/input_method/definitions.rs:33-45` (VIQR preset).
- Description / root cause: bamboo-core's VIQR preset uses `'` (sac), backtick
  (huyen), `?` (hoi), `~` (nga), `.` (nang), `^` (a-e-o circumflex), `+`/`*`
  (horn), `(` (breve), `d` (stroke), `0` (remove tone). `is_word_terminator`
  lists `'` backtick `?` `~` `.` `^` `+` `*` `(` as word terminators for every
  method, and the terminator branch runs before the key reaches bamboo-core.
  Only `bracket_key` (`[ ] { }`, and only with bracket mode on) and
  `digit_key` (Telex and VIQR — which also kills VIQR's `0` remove-tone key)
  are exempt. Every VIQR mark key except `dd` therefore ends the word instead
  of applying a mark. The comment at `src/bamboo_wrapper.rs:257-260` ("digits
  end the word for methods that do not use them as marks") is factually wrong
  for VIQR, which uses `0` as a mark. Additionally `is_late_mark_key`
  (`src/engine.rs:1788`) lists `'` backtick `?` `~` `!` as VIQR marks while
  bamboo-core's preset uses `.` for nang and has no `!` — a second mismatch.
- Reproduction: `method = "viqr"`, type `tooi'` — expected `tói`, actual
  `too'`. Likewise `hanh.` stays `hanh.` instead of `hạnh`.
- User-visible symptom: selecting VIQR produces Vietnamese without any marks.
- Confidence: confirmed by code (both sides read directly). No VIQR test
  exists (grep for `Viqr` in `src/` finds only `is_late_mark_key`; golden.rs
  ships Telex and VNI tables only) — which is how it survived 181 tests.

### BUG-02 — Microsoft layout cannot type ơ/ư while the bracket option is off (the default)

- Severity: Medium.
- Location: `src/bamboo_wrapper.rs:255-256` and `:263` (bracket exemption only
  when `bracket_mode != Disabled`); `src/config.rs:433` (default
  `bracket: Disabled`); cross-reference
  `bamboo-core/src/input_method/definitions.rs:63-66` (`[` = ư and `]` = ơ are
  the ONLY horn keys of this layout).
- Description / root cause: Microsoft layout places marks on digits (3 = ê,
  8 = sac) — digits are not intercepted for MsLayout so those work — but its
  only ơ/ư keys are the brackets, and brackets terminate the word unless the
  `bracket` option is enabled. The exemption was designed for Telex's bracket
  option and does not account for a layout that requires it.
- Reproduction: `method = "microsoft"`, default bracket off, type `du[` —
  expected `tư`, actual `du[`.
- User-visible symptom: MsLayout types `tiếng` but fails on any word with ơ/ư
  unless the user discovers and enables the `bracket` option (an undocumented
  config coupling).
- Confidence: confirmed by code; not reproduced live.

### BUG-03 — `DeleteBefore` actions emitted outside the `deletes_allowed()` gate: silent lost deletes, duplicated text on clients with no delete transport

- Severity: Medium.
- Location: the only gate is `src/engine.rs:1303-1318` (Unicode arm of
  `on_edit`). Paths that skip it: (a) late-tone re-entry
  `src/engine.rs:925-941`, notably `Action::DeleteBefore(n)` at line 937;
  (b) inline macro expansion `src/engine.rs:1236-1238`; (c) inline auto-restore
  `src/engine.rs:1266-1268`; (d) macro expansion in OFF mode
  `src/engine.rs:1086-1088`; (e) legacy-charset rewrite `src/engine.rs:1335-1342`
  into `rewrite()` at `src/engine.rs:1348-1359`; (f) em dash
  `src/engine.rs:993`; (g) double space `src/engine.rs:1004`. Receiver side:
  `src/server.rs:841-855` — when injection is impossible (no `/dev/uinput`)
  and there is no ST report, the delete is silently DROPPED (log only) while
  the following commit still goes out.
- Description / root cause: the current design is "the engine emits, emit
  guards" — but emit's only fallback for a lost delete is to drop the
  deletion, never the paired commit. Any delete+commit pair emitted for a
  client that has neither an ST report nor uinput becomes
  append-on-top-of-old-text. The Unicode arm of `on_edit` handles this
  correctly (raw-forward plus `resync_output_to_raw`); every other path does
  not. Note: `click_relocate_word` (`src/engine.rs:1611-1615`) gates properly
  and is NOT part of this family.
- Reproduction (most typical): app without ST and without a client id
  (Sublime / JetBrains X11 per the transport table in ARCHITECTURE.md) on a
  machine without `/dev/uinput`, preedit mode: type `tung`, pause 800 ms (idle
  commit puts `tung` into the app), type `f` — the re-entry emits
  `DeleteBefore(4)` which is dropped, preedit `tùng` appears on top, commit
  lands, screen shows `tungtùng `. Before the re-entry feature existed
  (Round 25) this situation produced only `tungf`.
- Second reproduction: same environment, type a macro (`ddq` + space) —
  result `ddqđại đội trưởng`.
- User-visible symptom: doubled words or macro expansion glued to the old
  word; only in the environment described above.
- Confidence: confirmed by code (the logic path is closed end to end); not
  measured live.

### BUG-04 — `backspace()` resolves the mode via `effective_mode()` instead of the word-mode lock

- Severity: Low.
- Location: `src/engine.rs:1480` (`let mode = self.effective_mode();`)
  versus `process_char` using `key_mode()` (`src/engine.rs:953`, lock at
  `src/engine.rs:897-906`); the Off/DefaultTyping checks at the top of
  `process_char` (`src/engine.rs:944`, `:950`) also bypass the lock.
- Description / root cause: the word-mode lock exists precisely because the
  daemon flips capabilities mid-word (comment at `src/engine.rs:172-176`).
  `process_char` honors the lock; `backspace` does not. If a flip happens
  mid-word, backspace takes the branch of a different mode than the keys
  around it — for example a NonPreedit word (per-key commits in the app)
  where the preedit capability bit appears: backspace returns `UpdatePreedit`
  and a preedit string renders on top of the already-committed text until the
  word ends.
- Reproduction: requires a mid-word capability flip by the daemon (observed
  shape `0b1001` vs `0b101001`, mostly around focus changes), then type `tun`,
  flip, Backspace, `g`.
- User-visible symptom: a phantom preedit overlapping committed text; self
  heals at word end.
- Confidence: inferred — the asymmetry is confirmed by code, the trigger
  depends on daemon behavior mid-word; needs live verification.

### BUG-05 — Keypad (KP_*) keysyms are not decoded; KP_Enter does not end the word

- Severity: Low.
- Location: `src/keymap.rs:40-49` (`keysym_to_char` covers only
  `0x20..=0x7e`, `0xa0..=0xff` and the Unicode range `0x0100_0000..`);
  `src/keymap.rs:52-69` (`is_editing_key` lacks KP_Enter 0xff8d).
- Description: KP_0..KP_9 are 0xffb0..0xffb9, so `keysym_to_char` returns
  `None` and the key is forwarded raw. VNI marks typed on the numpad do not
  apply. KP_Enter is not in `is_editing_key`, so it does not end the
  composition; a preedit stays pending one extra beat (until the idle timeout
  or the next key).
- Reproduction: NumLock on, VNI, type `tie6ng1` on the numpad — stays `tie6ng1`.
- User-visible symptom: VNI unusable on the numpad; numpad Enter leaves the
  underline up longer.
- Confidence: confirmed by code (keysym values are standard X11).

### BUG-06 — `Mutex::lock().unwrap()` on every D-Bus handler branch: a single panic kills all typing until IBus restarts

- Severity: Low (hardening; no concrete panic known).
- Location: `src/server.rs:221` (`engine_call`), `:361` (`process_key_call`),
  `:396` (`try_cycle_hotkey`), `:419` (`maybe_reload_config`); the main loop
  `run()` (`src/server.rs:95,104`) already uses `.ok()` correctly.
- Description / root cause: if any panic happens while the lock is held (a
  slice error, an overflow in a debug build, a future bug), the mutex is
  poisoned and every subsequent key event panics at the `.unwrap()` inside the
  message filter — typing dies desktop-wide, exactly what AGENTS.md forbids.
  The engine itself avoids unwrap/expect carefully, but the server turns "one
  panic" into "permanently broken" instead of degrading.
- Confidence: mechanism confirmed by code; no real-world panic observed.

### PERF-01 — `Config::load()` runs on EVERY key event, including key releases

- Severity: Medium.
- Location: `src/server.rs:370` (first line of `try_cycle_hotkey` loads the
  config BEFORE checking `cycle_mode_hotkey.is_empty()`), called from
  `process_key_call` (`src/server.rs:357`) for every event; the release mask
  is only checked later inside `process_key` (`src/server.rs:612`).
- Description / root cause: every press + release = 2 x (stat from
  `migrate_old_config`, file read, TOML parse, full struct clone including
  `app_rules`). This directly violates the invariant stated in
  ARCHITECTURE.md ("no blocking I/O in the key path") and in AGENTS.md. Real
  cost per call is small (tens to hundreds of microseconds) but it is
  doubled per keystroke and entirely avoidable.
- Confidence: confirmed by code.

### PERF-02 — `debug_log::log` opens the file and runs `create_dir_all` for EVERY line; with logging on, each keystroke produces at least 2 lines

- Severity: Medium (only when `debug_log = true` — exactly when latency is
  being diagnosed).
- Location: `src/debug_log.rs:25-43` (open/append/close plus `create_dir_all`
  per call); per-key log sites: `src/server.rs:610` (`key keyval=...`) and
  `src/engine.rs:732-735` (`st report ...` for every SetSurroundingText —
  ST-reporting apps send one per key).
- Description / root cause: no persistent file handle, no buffering. Round 17
  removed a "log wrote to disk twice per keystroke" regression (see DEVLOG
  Round 22) but the open-per-line mechanism remains, so the cost class is the
  same; the debug log therefore distorts the very latency it exists to
  diagnose.
- Confidence: confirmed by code.

### PERF-03 — The app's full buffer text is cloned 2-3 times per ST report, and the whole prefix is scanned into a `Vec<char>` per key

- Severity: Low (Medium on very long documents in ST-reporting apps:
  Firefox, gedit).
- Location: `src/engine.rs:736-737` — `pre_click = self.surrounding.clone()`
  and `old = self.surrounding.replace((text.clone(), ...))` (two clones of the
  previous text, which can be a whole paragraph; `old` is only used in the
  suggestions branch); `src/engine.rs:1368` — `verify_surrounding` collects
  the entire text before the caret into a `Vec<char>` on every key in
  NonPreedit/Surrounding.
- Description: O(n) cost over the buffer length on every keystroke; suffix
  comparison does not need full clones or a full collect.
- Confidence: confirmed by code (impact size depends on the app).

### PERF-04 — Every FocusIn reloads the full config plus macros plus dictionary plus the 38 350-word list

- Severity: Low.
- Location: `src/server.rs:268-273` into `src/engine.rs:344-347`
  (`Macros::load()`, `Dictionary::load()`, `spellcheck::WordList::load()` —
  about 400 KB read plus a HashSet build).
- Description: each alt-tab pays a few ms of I/O and allocation on the D-Bus
  thread. Functionally harmless; wasteful because the files rarely change.
  Could cache by mtime like the hot-reload path.
- Confidence: confirmed by code.

### GAP-01 — `commit_interval_ms` is dead config (already listed in `docs/gap-analysis.md`; re-confirmed)

- Severity: Low. The getter `src/engine.rs:420-422` has no consumer (grep
  over `src/`); the GUI still exposes it. Awaiting owner decision: wire it in
  or remove it.

### GAP-02 — `is_late_mark_key` misses shape keys and disagrees with the VIQR preset

- Severity: Low. `src/engine.rs:1785-1791`: Telex accepts only
  `s f j x r` (no `w` horn, no `dd`, no aa/ee/oo doubling), VIQR accepts
  `' backtick ? ~ !` but bamboo-core uses `.` for nang (see BUG-01) and has no
  `^ + * (`. So `tung` + pause + `w` gives `tungw` instead of `tưng` (in real
  ibus-bamboo the composition is still pending, so `w` applies). This may be
  the intended owner scope ("tone keys" only, Round 25) — raised for a
  decision.

### GAP-03 — `paste.rs`: one thread per paste, small clipboard-snapshot race

- Severity: Low. `src/paste.rs:108-120` (spawn a thread per paste, poll every
  50 ms up to 600 ms); race: if the previous restore has not finished writing
  when the next paste snapshots (`src/paste.rs:89-94`), the user's clipboard
  can be restored as the previously pasted word. Rare and self-limiting;
  affects only the uinput modes' `clipboard` channel (default is `commit`).

### DOC-01 — Test numbers in the documentation are stale

- `AGENTS.md` and `ARCHITECTURE.md:183` say "133 tests incl. 480-case golden
  table"; `README.md:40` says 164. Actual count: 181 `#[test]` (matches
  DEVLOG Round 25 "179 to 181"), and the golden tables hold 202 rows x 3
  variants = 606 cases (TELEX + VNI), not 480.

### DOC-02 — `TODO.md:111` "Defaults (owner spec): Non-preedit default mode" contradicts the code

- The real default is Preedit (`src/config.rs:204` enum default, `:424`
  Default impl), and `docs/open-questions.md` (Design questions #1) confirms
  preedit has been settled since Round 5. The TODO line is stale.

### DOC-03 — The `Mode::Off` doc comment still claims auto-capitalize runs in OFF (known drift, unfixed)

- `src/engine.rs:49-50` ("macros and auto-capitalize still run") versus
  `process_char_off` (`src/engine.rs:1080-1108`, macros only). Same as
  `docs/gap-analysis.md` Known gap #3 — still present.

### DOC-04 — README wording "spell check backed by a bundled 38 350-word list" (`README.md:27`) reads as if the list drives auto-correct

- The word list is lookup-only (`src/spellcheck.rs:1-6`,
  `docs/gap-analysis.md` Known gap #2); auto-correct is bamboo-core
  phonotactics and the list takes no part in it. Wording should change.

## 4. Common root causes

1. The wrapper's terminator set is not method-aware (BUG-01, BUG-02, part of
   GAP-02). `is_word_terminator` plus the interception branch at
   `src/bamboo_wrapper.rs:263` is one table for all four methods, although the
   wrapper already has a method-aware precedent (`digit_key` applies only to
   Telex/VIQR). VIQR and Microsoft layout are the two methods whose mark keys
   live inside the terminator set, so they break entirely.
2. The delete-transport gate lives at one call site instead of the point of
   emission (BUG-03). The Unicode arm of `on_edit` knows how to raw-forward
   when no transport exists; every other delete-emitting path assumes emit
   will handle it — but emit can only drop the delete, never the paired
   commit.
3. The word-mode lock is bypassed at secondary entry points (BUG-04): the lock
   was designed for exactly the mid-word daemon flip, but `backspace` and the
   Off/DefaultTyping checks re-resolve the mode independently.
4. Disk I/O on the key path (PERF-01, PERF-02): config and log are read and
   written inside key processing, violating the project's own invariant.
5. Blind spots in the test suite: BUG-01/02/03 all sit at the intersection of
   method x transport x charset that no test reaches — there is no VIQR or
   MsLayout golden table, no legacy-charset or re-entry case on a client
   without ST/uinput, no numpad test.

## 5. Fix proposals (descriptions and illustrative diffs only — NOT applied)

### BUG-01 (VIQR) — make the terminator set method-aware, following the `digit_key` pattern

Add an exemption in `BambooWrapper::process_char` next to `bracket_key` and
`digit_key`:

```rust
// VIQR marks must reach bamboo-core: they overlap the punctuation
// terminator set, so exempt them exactly like digits/brackets above.
let viqr_mark_key = self.method == Method::Viqr
    && matches!(c, '\'' | '`' | '?' | '~' | '.' | '^' | '+' | '*' | '(' | '0');
// MsLayout types ư/ơ only through brackets: exempt them for it.
let ms_bracket_key = self.method == Method::MsLayout
    && matches!(c, '[' | ']' | '{' | '}');
if (is_word_terminator(c) || digit_key) && !bracket_key && !viqr_mark_key && !ms_bracket_key {
```

Also remove Viqr from `digit_key` (VIQR uses `0` as a mark) and fix
`is_late_mark_key` to use `.` instead of `!`. Risk / design decision: VIQR has
an inherent mark-versus-punctuation ambiguity (`.` at the end of a sentence is
nang or a period?). Options: (a) exempt unconditionally, like classic
unikey/VIQR IMEs — bamboo-core then decides whether the mark applies and the
wrapper can Finish plus forward when bamboo rejects it; (b) exempt only when
the current composition can validly receive the mark. Recommendation: start
with (a), revisit after real user feedback. Scope: one function in
`bamboo_wrapper.rs`, one in `engine.rs`, plus a new VIQR golden table.
Breakage: low — only opens a path for keys that currently never reach bamboo;
Telex/VNI unaffected because the exemptions are method-gated.

### BUG-02 (MsLayout) — covered by the same diff (`ms_bracket_key`)

Alternative: make `config.rs` auto-enable the bracket exemption when
`method == microsoft` regardless of the `bracket` option. The wrapper-side
exemption is preferred (no config semantics change for Telex users). Risk: a
MsLayout user who relies on literal brackets sees a difference — but the
current state is "cannot type ư/ơ at all", so there is nothing to lose.
Breakage: low.

### BUG-03 — centralize the gate: skip the whole rewrite when the delete cannot be delivered

Either gate at the engine (a single check before every delete+commit
emission) or inside `emit_with`, which already sees the whole action batch and
can drop the paired commit of the same rewrite:

```rust
// emit_with, Action::DeleteBefore arm, DROPPED branch:
if !engine.surrounding_data_ok() {
    debug_log::log(...);
    // Dropping the delete but still committing the replacement would
    // duplicate text; skip the whole rewrite and let the engine resync.
    engine.resync_output_to_raw();   // + drop the adjacent Commit in this batch
}
```

Risk: must distinguish a standalone commit (word end, terminator forward)
from a commit that belongs to a rewrite pair — group by position in the
batch. Scope: `server.rs::emit_with` plus new tests for 3 modes x (no uinput,
no ST). Breakage: medium-low; behavior only changes in environments that are
broken today.

### BUG-04 — make `backspace()` honor the word-mode lock

At `src/engine.rs:1480`, use `key_mode()` while a composition is in flight
(or store the word's locked mode when `key_mode()` locks and read it back in
`backspace`). Risk: low; add a test that a mid-word capability flip does not
change which branch backspace takes. Breakage: low.

### BUG-05 (numpad) — `keymap.rs`

In `keysym_to_char` add `0xffb0..=0xffb9 => char::from_u32('0' as u32 +
keyval - 0xffb0)`; add KP_Enter (0xff8d) and KP_Delete / KP_Tab to
`is_editing_key`. Risk: near zero — these keys are forwarded raw today, so
every current case can only improve. Note KP_Digit keysyms only appear with
NumLock on (desired); with NumLock off the daemon sends arrow keysyms, so no
conflict.

### BUG-06 (mutex poisoning) — survive a poisoned lock

Replace the `.lock().unwrap()` calls in the handlers with
`.lock().unwrap_or_else(|p| p.into_inner())` (accept the post-panic state and
keep typing) — or at minimum let `run()` detect poisoning and exit cleanly so
ibus-daemon respawns the engine instead of a zombie answering every key with
an error. Risk: into_inner after a panic may carry half-updated state — still
better than a dead engine; log to stderr. Breakage: very low.

### PERF-01 — cache the cycle hotkey in the handler

Read `cycle_mode_hotkey` once at startup and refresh it inside
`maybe_reload_config` (which already loads the config under the mtime gate);
`try_cycle_hotkey` then only compares keyval/mods against the cache, touching
no disk. Risk: near zero. Breakage: low.

### PERF-02 — keep the debug-log file handle open

A `static LOG: Mutex<Option<File>>`; `set_enabled` opens/closes; `log()` only
writes; run `create_dir_all` once at open. Risk: if the user deletes the file
mid-session, the old handle writes invisibly — acceptable, or re-stat
sparsely. Breakage: low.

### PERF-03 / PERF-04 — reduce clones

PERF-03: replace the two clones in `set_surrounding` with a
`std::mem::take`-based swap and compare suffixes directly on `&str`; avoid the
full `Vec<char>` collect in `verify_surrounding` by comparing iterators.
PERF-04: cache `Config` / `Macros` / `Dictionary` / word list by mtime like
the hot-reload path. Risk: low; `set_surrounding` is sensitive, so the
click/park tests must be rerun in full.

### GAP-02 — owner decision

Extend re-entry to shape keys (`w`, `dd`, aa/ee/oo for Telex; `^ + * (` for
VIQR) or keep the Round 25 "tone keys only" scope. If extended, do it after
BUG-01 is fixed and add the keys per method in `is_late_mark_key`.

## 6. Recommended priority order

1. BUG-01 + BUG-02 (VIQR + MsLayout) — an advertised feature is fully broken
   on VIQR; one diff in `bamboo_wrapper.rs` covers both. Tests after: a new
   VIQR golden table (all marks `' backtick ? ~ . ^ + * ( dd 0`) through all 3
   main display modes; MsLayout `ti3ng8`, `du[`, `doj]`; the 606 existing
   Telex/VNI golden cases must not regress; `ddc` still gives `đc`; English
   punctuation (`don't`, `(a)`, `3.14`) must stay literal in Telex/VNI (the
   exemptions are method-gated).
2. PERF-01 — small, independent, immediate key-path win.
3. BUG-03 — prevents doubled text; needs new tests across 3 modes x 2
   no-transport environments and verification with `MANGRS_REQUIRE_UINPUT=1`.
4. BUG-05 (numpad) — a few lines in `keymap.rs`.
5. BUG-04, BUG-06, PERF-02/03/04 — stability and hygiene.
6. GAP-01 (`commit_interval_ms`) — owner decision: wire or remove.
7. DOC-01..04 — documentation catch-up.

Not verified experimentally within this read-only analysis: real desktop
behavior of VIQR/MsLayout (requires real typing or a permitted `cargo test`
run), and the real-world frequency of mid-word capability flips for BUG-04.

## 7. Open questions for the owner

1. BUG-01: is this a genuine bug (not a deliberate "VIQR partial support")?
   If fixing, which ambiguity policy for VIQR mark-vs-punctuation: (a)
   unconditional exemption like classic unikey, or (b) exempt only when the
   composition can validly receive the mark, otherwise Finish and forward the
   character?
2. BUG-02: wrapper-side automatic bracket exemption for MsLayout (no config
   change), or "bracket mode on is a documented requirement for MsLayout"?
3. BUG-03: skip-the-whole-rewrite at emit (recommended), or per-site gates
   mirroring the Unicode arm?
4. May `cargo test` (with `env -u RUSTC_WRAPPER`) be run to confirm the
   181/181 baseline before any fix is proposed? Read-only; touches only
   `target/`.
5. GAP-02: extend the late-tone re-entry to shape keys (`tung` + pause + `w`
   giving `tưng`), or keep the Round 25 "tone keys only" scope?
