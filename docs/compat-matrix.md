# Compatibility matrix

Real-typing results per application, display mode, and workload. This file
decides what Phase 6 (input modes) still needs: an app that already passes in
every mode needs nothing; an app that loses or duplicates characters gets an
App Rule or a new mode.

Each row is produced by the test tooling (`test/README.md`), never by memory:

```sh
# 29 hard words, one line, no Enter; cycles preedit -> nonpreedit -> surrounding
python3 test/typing_tool.py
# backspace / delete / caret workload (11 scenarios, no Enter)
python3 test/typing_tool.py --file test/keys-test
# per-keystroke latency, appends one JSON line per mode to test/perf-results.jsonl
python3 test/perf_tool.py --label "<app> <context>"
```

The tool types through the kernel (`/dev/uinput`), so what the engine receives
is identical to a real keyboard. PASS means the AT-SPI read-back matched
`text-expected` / `keys-test.expected` code-point-for-code-point.

Status legend: **PASS** verified workload matched · **OK** manual check, no
read-back · **FAIL** lost/duplicated/corrupted chars · **N/A** mode cannot work
in this field · **—** not tested yet.

## Measured (GNOME 51, Wayland)

| App / field | Date | Workload | Preedit | NonPreedit | Surrounding | Notes |
|---|---|---|---|---|---|---|
| GNOME Text Editor | 31/10 | text-test (29 words) | PASS | PASS | PASS | All 3 modes with the unikey-backspace engine (fix of 31/10). |
| GNOME Text Editor | 31/10 | keys-test (11 scenarios) | PASS | PASS | PASS | Lone `x` rendered twice after a forwarded nav key (preedit) is an app-side artifact, see open-questions #12; engine commits exactly once per mode (unit-tested). |
| Chrome omnibox | 31/10 | text-test + backspace cases | PASS | OK | OK | Verified after the backspace fix. First word next to an autocomplete suggestion may drift (TODO "Awaiting real tests"). |
| Chrome omnibox | 30/10 | perf (inject overhead only) | p50 72 us, p95 106 us | — | — | `matched=0`: engine debug log was off during that run; rerun with `debug_log = true` for real latency. |
| Firefox address bar | 31/10 | keys-test | PASS | OK | OK | gdbus in-process client. Surrounding-text deletes behave naively-correct (no swallow needed). |
| Firefox address bar | 29/10 | lone-x probe | x3 | x0 | x0 | Rendering artifact counts from open-questions #12 measurement. |
| GNOME Text Editor + Firefox | 31/10 | text-test in smooth/supersmooth/select | PASS (preedit fallback) | PASS (preedit fallback) | PASS (preedit fallback) | Uinput rewrite products never reach mutter-relayed apps (open-questions #13): the modes resolve to preedit on GNOME Wayland. Injection itself works; X11 sessions are the real target. |

## To be measured

Run the two typing commands in each field, one mode cycle per app (remove the
app's rule from `config.toml` first so the global cycle applies, or the cycle
is silently overridden). Wayland rows first; X11 (GNOME on Xorg) rows are a
separate column because the IBus route differs (XIM bridge apps send no
`FocusInId`, see open-questions #0).

| App / field | Toolkit | Workload | Preedit | NonPreedit | Surrounding | Notes |
|---|---|---|---|---|---|---|
| GNOME Terminal | VTE | text + keys | — | — | — | No preedit by default?; candidate weak spot for lost chars (Phase 6 quality goal). |
| kitty or Alacritty | GPU terminal | text + keys | — | — | — | Same as above. |
| Chromium | Chromium/GTK | text + keys | — | — | — | Chrome's omnibox result does not transfer; test Chromium's own field. |
| VS Code | Electron | text + keys | — | — | — | Phase 6 weak spot: lost/duplicated chars. |
| gedit | GTK3 | text + keys | — | — | — | GTK3-vs-GTK4 comparison for open-questions #12. |
| a Qt app (e.g. qlcpe / kate) | Qt | text + keys | — | — | — | Qt5/Qt6 ibus platform input context. |
| LibreOffice Writer | VCL | text + keys | — | — | — | Own composition window handling. |
| Sublime Text | custom | text + keys | — | — | — | Preedit invisible (app never renders it) — expect OK only in nonpreedit/surrounding. |
| JetBrains (XToolkit) | X11 bridge | text + keys | — | — | — | No FocusInId expected; listed under X11 too. |
| Same set, X11 session | | text + keys | — | — | — | One representative GTK app, one Electron, one terminal enough to start. |

After a row is filled: update this file, add the app to `docs/troubleshooting.md`
if it needs a workaround, and add an App Rule to the default config if the right
mode differs from the global default. Reprioritize Phase 6 from the FAIL rows.
