#!/usr/bin/env python3
"""Measure per-keystroke latency of the typing pipeline.

Types a workload through the same /dev/uinput virtual keyboard as
typing_tool.py, recording the exact send time of every key (milliseconds
since the epoch). Afterwards it matches each sent key against the engine's
debug log (`~/.cache/ibus-mangrs/debug.log`, enable with `debug_log = true`)
which logs every ProcessKeyEvent with the same clock. The difference is the
per-keystroke engine latency: kernel -> compositor -> ibus -> engine.

Every run appends one JSON line to a shared results file (default
test/perf-results.jsonl) so runs from different days/apps accumulate.

Usage:
    python3 perf_tool.py --label "chrome"          # cycles all display modes
    python3 perf_tool.py --file short.txt --delay 10
    python3 perf_tool.py --modes preedit           # single mode
    python3 perf_tool.py --no-engine               # skip log matching
"""

import argparse
import json
import os
import re
import statistics
import struct
import sys
import time
from datetime import datetime, timezone

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)
import typing_tool  # noqa: E402  (reuses the keymap and uinput setup)

DEBUG_LOG = os.path.expanduser("~/.cache/ibus-mangrs/debug.log")
KEYVAL_RE = re.compile(r"^(\d+) key keyval=0x([0-9a-fA-F]+)")

# US-QWERTY keysym for the characters we send: letters, digits, space and the
# mapped punctuation share their ASCII code; Enter/Tab are special keysyms.
SPECIAL_KEYSYM = {"\n": 0xFF0D, "\t": 0xFF09}


def keysym_for(ch):
    if ch in SPECIAL_KEYSYM:
        return SPECIAL_KEYSYM[ch]
    return ord(ch.lower())


def match_engine_log(sends, log_path=DEBUG_LOG):
    """Match sent keys (index, keysym, t_send) to `key keyval=` log lines.

    Returns (latencies, matched) where latencies is {index: ms}. Matching is
    in order: each sent key consumes the next log line with the same keysym
    whose timestamp is not before the send time minus a small clock slack.
    """
    if not os.path.exists(log_path):
        return {}, 0
    events = []
    with open(log_path, encoding="utf-8") as handle:
        for line in handle:
            m = KEYVAL_RE.match(line)
            if m:
                events.append((int(m.group(1)), int(m.group(2), 16)))
    latencies = {}
    pos = 0
    for index, keysym, t_send in sends:
        limit = t_send - 50  # clock slack, ms
        while pos < len(events):
            ts, kv = events[pos]
            if kv == keysym and ts >= limit:
                latencies[index] = ts - t_send
                pos += 1
                break
            if ts < limit:
                pos += 1
                continue
            pos += 1
    return latencies, len(latencies)


def stats(values):
    if not values:
        return None
    ordered = sorted(values)
    p95 = ordered[min(len(ordered) - 1, int(round(0.95 * len(ordered))) - 1)]
    return {
        "min": round(min(ordered), 1),
        "p50": round(statistics.median(ordered), 1),
        "p95": round(p95, 1),
        "max": round(max(ordered), 1),
    }


def expected_path_for(keys_file):
    """Expected-output file for a keys file: accept both naming styles."""
    candidates = [keys_file + ".expected"]
    if keys_file.endswith("-test"):
        candidates.append(keys_file[:-5] + "-expected")
    for path in candidates:
        if os.path.exists(path):
            return path
    return candidates[0]

def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--file", default=os.path.join(HERE, "text-test"))
    parser.add_argument("--delay", type=int, default=30)
    parser.add_argument("--interval", type=int, default=80)
    parser.add_argument("--label", default="unlabeled", help="app/mode note stored with the run")
    parser.add_argument("--results", default=os.path.join(HERE, "perf-results.jsonl"))
    parser.add_argument("--modes", default=",".join(typing_tool.DEFAULT_CYCLE),
                        help="comma-separated display modes, one workload each "
                             "(all: " + ",".join(typing_tool.MODES) + ")")
    parser.add_argument("--no-engine", action="store_true", help="skip debug-log matching")
    args = parser.parse_args()
    modes = [m.strip() for m in args.modes.split(",") if m.strip()]

    try:
        with open(args.file, encoding="utf-8") as handle:
            text = handle.read()
    except OSError as err:
        sys.exit(f"cannot read {args.file}: {err}")
    # never press Enter (submits/drops fields); newlines become spaces
    text = text.replace("\n", " ").replace("\r", " ")
    text = "".join(ch for ch in text if ch in typing_tool.US_QWERTY)
    if not text:
        sys.exit("nothing to type")

    expected_path = expected_path_for(args.file)
    expected = None
    try:
        with open(expected_path, encoding="utf-8") as handle:
            expected = handle.read()
    except OSError:
        print(f"note: no expected-output file at {expected_path} - typed text "
              "cannot be verified automatically")

    original_mode = typing_tool.read_engine_mode()
    t_start = time.time() * 1000
    print(f"Typing {len(text)} keys per display mode: {' -> '.join(modes)}\n"
          f"Label: {args.label}. Click into the target input field NOW and keep it "
          "focused (Ctrl+C cancels).\n"
          "note: per-app rules override the global mode - test in an app without a "
          "matching rule, or the mode switch will not take effect.")
    for remaining in range(args.delay, 0, -1):
        print(f"\r{remaining:3d}s ", end="", flush=True)
        time.sleep(1)
    print()

    fd = typing_tool.open_device()
    results = []
    try:
        for round_index, mode in enumerate(modes):
            typing_tool.set_engine_mode(mode)
            time.sleep(1.5)  # engine hot-reloads config within ~1 s
            print(f"--- display mode {round_index + 1}/{len(modes)}: {mode}")

            sends = []          # (index, keysym, t_send_ms)
            inject_us = []      # how long our own uinput writes took
            for index, ch in enumerate(text):
                code, shift = typing_tool.US_QWERTY[ch]
                t0 = time.perf_counter_ns()
                t_send = time.time() * 1000
                typing_tool.press(fd, code, shift)
                inject_us.append((time.perf_counter_ns() - t0) / 1000)
                sends.append((index, keysym_for(ch), t_send))
                time.sleep(args.interval / 1000)
            time.sleep(0.5)  # let the last commit land before verifying/measuring
            if expected is not None:
                typing_tool.verify_typed(expected)
            if round_index < len(modes) - 1:
                time.sleep(1.0)

            result = {
                "ts": datetime.now(timezone.utc).astimezone().isoformat(timespec="seconds"),
                "label": args.label,
                "mode": mode,
                "file": os.path.basename(args.file),
                "interval_ms": args.interval,
                "keys_sent": len(sends),
                "inject_us": stats(inject_us),
            }
            if args.no_engine:
                result["engine_latency_ms"] = None
                result["matched"] = 0
            else:
                latencies, matched = match_engine_log(sends)
                result["matched"] = matched
                result["engine_latency_ms"] = stats(list(latencies.values()))
                if matched == 0:
                    print("no keys matched in the debug log - is debug_log = true and "
                          "is MăngRS the active input source?")
            results.append(result)
            print(f"injection overhead us: {result['inject_us']}")
            if result["engine_latency_ms"]:
                print(f"engine latency ms   : {result['engine_latency_ms']} "
                      f"(matched {matched}/{len(sends)})")
        time.sleep(0.3)  # let the engine flush its last log entries
    finally:
        import fcntl
        fcntl.ioctl(fd, typing_tool.UI_DEV_DESTROY)
        os.close(fd)
        if original_mode:
            try:
                typing_tool.set_engine_mode(original_mode)
                print(f"original display mode restored: {original_mode}")
            except SystemExit:
                print(f"could not restore original mode ({original_mode}) - set it manually")

    # archive the engine log for this run next to the results so every
    # measurement can be cross-referenced with its raw keystroke decisions
    archive = typing_tool.archive_engine_log(args.label, t_start)
    archive_rel = os.path.relpath(archive, HERE) if archive else None
    for result in results:
        result["debug_log"] = archive_rel
    if archive:
        print(f"engine debug log archived to {archive}")
    else:
        print("no engine debug log found (debug_log = false?) — nothing archived")

    with open(args.results, "a", encoding="utf-8") as handle:
        for result in results:
            handle.write(json.dumps(result, ensure_ascii=False) + "\n")
    print(f"saved to {args.results}")


if __name__ == "__main__":
    main()
