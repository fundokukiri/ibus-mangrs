# Performance

Goal (TODO Phase 12): per-keystroke latency must be imperceptible under fast
typing — the user should never feel the engine behind the keyboard.

## What is measured

`test/perf_tool.py` types a workload through the kernel and, for every key,
records the time between the uinput injection and the engine's `ProcessKeyEvent`
log line (both clocks are millisecond-precision and identical, so the
difference is the true pipeline latency: uinput -> kernel -> compositor -> IBus
-> engine). It also records the injection overhead alone (microseconds — that
is our code), and appends one JSON line per display mode to the shared
`test/perf-results.jsonl`.

```sh
# enable debug logging first (GUI General tab, or debug_log = true in config)
python3 test/perf_tool.py --label "text-editor"        # one workload per mode
python3 test/perf_tool.py --no-engine                  # injection overhead only
```

## Reading the results

- **p95 is the number that matters**: users feel the slow keys, not the average.
- `matched` below `keys_sent` means the engine saw fewer keys than were sent —
  the run is suspect (engine not the active input source, or log rotated).
- One JSON line per mode; compare across apps, modes and engine versions with
  any JSONL reader, e.g.
  `jq -r 'select(.label=="text-editor") | [.mode, .engine_latency_ms.p95] | @tsv' test/perf-results.jsonl`.

## Current numbers

All runs on the same machine (GNOME 51, Wayland). Empty cells: not yet
measured — enable `debug_log` and run the tool in the named field.

| Date | App / field | Mode | Workload | Engine p50 | Engine p95 | Inject p50 | Notes |
|---|---|---|---|---|---|---|---|
| 30/10 | Chrome omnibox | preedit | text-test | — | — | 72 us | Injection overhead only (`matched=0`: debug log was off); rerun for engine latency. |
| | | | | | | | |

Acceptance: engine p95 well under one frame (~16 ms) in every mode and every
app tested, with `keys_sent == matched`. When a row fails, compare the JSON
lines against the archived engine log (`test/logs/`) before blaming the app.
