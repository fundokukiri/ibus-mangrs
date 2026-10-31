# Test tooling

## typing_tool.py — automatic typing into the focused input field

Reads a keystroke file, waits, then presses the keys through a temporary
virtual keyboard on `/dev/uinput` — the kernel route, identical to real
typing, so MăngRS processes every keystroke. Use it to reproduce mode bugs
and to fill `docs/compat-matrix.md` without typing by hand.

```sh
# types test/text-test once per display mode (preedit -> nonpreedit ->
# surrounding), switching config.toml `mode` automatically, 30 s countdown
python3 test/typing_tool.py

python3 test/typing_tool.py --delay 10            # shorter countdown
python3 test/typing_tool.py --modes preedit       # single mode
python3 test/typing_tool.py --file case.txt --interval 120
```

Mode cycling rewrites only the `mode` key of `~/.config/ibus-mangrs/
config.toml` (the engine hot-reloads it within ~1 s) and restores the
original mode when done. Per-app rules override the global mode, so run the
cycle in an app without a matching rule. Adding a future display mode is a
one-line change: extend `MODES` at the top of `typing_tool.py` with the new
config `mode` value.

Requirements: `/dev/uinput` writable (udev rule `80-uinput.rules` + `input`
group — see `docs/install.md`; the install script does not set this up).
MăngRS must be the active input source; the file should contain raw
keystrokes (Telex/VNI sequences), which the engine converts.

## text-test / text-expected — the keystroke workload and its expected output

`text-test` is plain keystrokes on a **single line** — Enter is never pressed
on purpose: in search boxes and chat inputs it submits or drops the field, so
newlines in the file are typed as plain spaces. **29 words**, chosen hard on
purpose:

- the undo-mark family that caused the round 10–12 bugs: `toool`→tool,
  `teeen`→teen, `neeed`→need, `tooolbox`→toolbox;
- `ươ/ưu` rhymes and free-tone jumps: `muowif`→mười, `tuowi`→tươi,
  `suownf`→sườn, `tuowngr`→tưởng, `ruwouj`→rượu, `nghieeng`→nghiêng,
  `hieeur`→hiểu, `hoangf`→hoàng;
- mark placement traps: `ngooij`→ngồi (nặng), `muoois`→muối (sắc),
  `mieenx`→miễn (ngã), `quyeens`→quyến (sắc);
- terminators next to words: `tieengs2`→tiếng2, `x1`→x1, `ab_bcd2`→ab_bcd2;
- capitals: `Vieetj`→Việt, `DDuwowngf`→Đường.

`text-expected` holds the on-screen text the workload must produce, generated
by replaying the keys through the project's own `BambooWrapper` (the exact
engine pipeline, undo-mark and auto-restore included). Both tools type a
round, then read the focused field back over AT-SPI and print PASS/FAIL;
without AT-SPI access they print the expected tail for manual comparison.

After editing `text-test`, regenerate and (for new words) verify per word:

```sh
cd test/expected-gen && cargo run --offline -- ../text-test > ../text-expected
cargo run --offline -q -- --check pairs.tsv     # keys<TAB>expected-word lines
```

Trust the checker over your eyes: several of these words differ by one
combining mark and render deceptively (`dưỡng` vs `dượng`, `ngồi` vs `ngội`)
— the checker compares code points.

## perf_tool.py — per-keystroke latency measurement

Types the same way as `typing_tool.py` but records the send time of every
key, then matches each key against the engine's debug log (which logs every
`ProcessKeyEvent` with the same millisecond clock). The difference is the
real per-keystroke pipeline latency: uinput -> kernel -> compositor -> IBus
-> engine. Every run appends one JSON line to the shared results file
`test/perf-results.jsonl`.

```sh
# enable debug logging first: GUI General tab, or debug_log = true in config
python3 test/perf_tool.py --label "chrome"        # one workload per display mode
python3 test/perf_tool.py --file short.txt --delay 10 --interval 120
python3 test/perf_tool.py --modes preedit         # single mode
python3 test/perf_tool.py --no-engine             # injection overhead only
```

One results file collects every run; each line carries the timestamp, the
`--label` (name the app and mode), the display `mode` of that workload, the
workload, the key interval, the injection overhead (min/p50/p95/max in
microseconds) and the engine latency (min/p50/p95/max in milliseconds) plus
how many keys matched. One JSON line per display mode, so mode comparisons
come out of a single run. Compare across apps, modes, and engine versions
with `jq` or any JSONL reader. This feeds the TODO Phase 12 goal
(`docs/performance.md`).

Notes:
- `--label` is what makes runs comparable later; always set it.
- Matched counts below `keys_sent` mean the engine saw fewer keys than sent
  (raw-forward still logs; unmatched keys usually mean the engine was not
  the active input source, or the log was rotated mid-run).
- 95th percentile matters more than the average: users feel the slow keys.

## keys-test — backspace, delete and caret-movement workload

`typing_tool.py` also accepts special-key tokens (deliberately **no Enter** —
it submits or drops fields): `{backspace}`, `{delete}`, `{left}`, `{right}`,
`{up}`, `{down}`, `{home}`, `{end}`, `{tab}`, `{esc}`, `{space}`.

```sh
python3 test/typing_tool.py --file test/keys-test
```

`test/keys-test` exercises the backspace/caret paths: deleting a whole word
and retyping it, backspacing into a composed word, backspacing through marks,
digits then backspace, `{home}`-commit then insert, caret-left then
`{delete}`. `test/keys-test.expected` is the engine's current behavior
(generated by `expected-gen`, which replays through the real `BambooWrapper`
with a caret buffer model).

Two known divergences from naive editing are baked into that expected file —
they are engine bugs of the bamboo poison class, see `docs/open-questions.md`:
`toool{backspace}` keeps "tool" (the Backspace deletes nothing), and retyping
after backspacing into a marked word corrupts it (`muowif`+BS+BS+`if` →
"mừi" instead of "mười"). Regenerate the expected file after those get fixed
and the check should then match naive editing.

## click_tool.py — automated click-mid-preedit test (#14)

Types a sentence plus one word left pending in preedit, then **clicks the
mouse into the field by itself** — through a virtual absolute pointer on
`/dev/uinput` (mutter maps ABS 0..32767 onto the full screen), aimed at the
screen position of a character obtained from AT-SPI. After the click it
reads the field back: the pending word must have landed at the position
where it was typed — not at the click position, not lost, not corrupting
the sentence. A settling space is typed last so the pending word commits
and the final text is deterministic.

```sh
python3 test/click_tool.py --delay 10     # clicks start -> middle -> end
python3 test/click_tool.py --at middle    # one position only
```

Workload: `xin chaof cac ban choaf` renders `xin chào các bạn ` with the
last `chào` pending; expected tail after the settling space is
`xin chào các bạn chào `. PASS means the relocation machinery (click
surgery, or park-when-unreachable) delivered the word to its typed
position; FAIL prints the actual tail — usually the word vanished (park
fallback) or landed at the click position. Run per display mode with
`--modes`; the bug is preedit-only, so the default is `preedit`.

Requirements: writable `/dev/uinput` (keyboard + pointer), GNOME
accessibility for the read-back and coordinates, GTK3 for the screen size
(`--screen 1920x1080` when unavailable).

## Notes

- **Engine log archiving.** Both tools archive the engine's debug log after
  every run into `test/logs/<label>_<timestamp>.log` — for `typing_tool.py`
  the whole window since the countdown started; for `perf_tool.py` the path
  is also recorded in the run's JSON line (`debug_log` field), so every
  measurement can be cross-referenced with its raw keystroke decisions.
  Requires `debug_log = true`; the tools print a warning when the log is off.
- `test/logs/` and `test/perf-results.jsonl` are gitignored: debug-log
  entries can contain short surrounding-text snippets typed on the machine,
  so keep them local when sharing bug reports (strip or trim as needed).
- 30 s default countdown exists so you can switch to the target app after
  launching; `--delay 0` starts immediately.
- Keys press every 80 ms by default; raise `--interval` for slow apps.
- Characters with no US-QWERTY mapping (Vietnamese letters, most symbols) are
  skipped with a warning — the file holds raw keystrokes, not final text.
