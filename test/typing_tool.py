#!/usr/bin/env python3
"""Type the contents of a text file into the focused input field.

Opens a temporary virtual keyboard on /dev/uinput and presses the keys after
a delay, so the focused application (and MăngRS) receives them like real
typing. The workload is typed once per display mode: before each round the
tool rewrites the `mode` key of config.toml (the engine hot-reloads within
~1 s) and restores the original mode when finished.

Usage:
    python3 typing_tool.py                      # cycles all modes, 30 s countdown
    python3 typing_tool.py --delay 10           # shorter countdown
    python3 typing_tool.py --modes preedit      # single mode
    python3 typing_tool.py --file my-cases.txt --interval 120

No third-party packages needed; /dev/uinput must be writable (see
docs/install.md for the udev rule and the input group).
"""

import argparse
import fcntl
import os
import re
import struct
import sys
import time
from datetime import datetime

DEBUG_LOG = os.path.expanduser("~/.cache/ibus-mangrs/debug.log")
LOGS_DIR = os.path.join(os.path.dirname(os.path.abspath(__file__)), "logs")

# Display modes cycled through after each full workload, in order. To support
# a future display mode, add its config.toml `mode` value here — nothing else
# needs to change.
MODES = ["preedit", "nonpreedit", "surrounding"]
CONFIG_PATH = os.path.join(
    os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")),
    "ibus-mangrs", "config.toml")


def read_engine_mode():
    """Return the current `mode` value from config.toml, or None."""
    try:
        with open(CONFIG_PATH, encoding="utf-8") as handle:
            text = handle.read()
    except OSError:
        return None
    match = re.search(r'(?m)^\s*mode\s*=\s*"([^"]*)"', text)
    return match.group(1) if match else None


def set_engine_mode(mode):
    """Rewrite only the top-level `mode` key of config.toml — never the `mode`
    entries inside `[[app_rules]]` tables (those are per-app rules; clobbering
    them destroyed the sublime_text rule once). The engine hot-reloads the
    file within ~1 second, so a short wait after this is enough."""
    try:
        with open(CONFIG_PATH, encoding="utf-8") as handle:
            text = handle.read()
    except OSError as err:
        sys.exit(f"cannot read {CONFIG_PATH}: {err}")
    # top-level keys live before the first TOML table header
    head, sep, tail = text.partition("\n[[")
    new, count = re.subn(r'(?m)^(mode\s*=\s*)"[^"]*"', rf'\g<1>"{mode}"', head)
    if count == 0:
        new = new.rstrip("\n") + f'\nmode = "{mode}"'
    with open(CONFIG_PATH, "w", encoding="utf-8") as handle:
        handle.write(new + sep + tail)

UI_SET_EVBIT = 0x40045564
UI_SET_KEYBIT = 0x40045565
UI_DEV_CREATE = 0x5501
UI_DEV_DESTROY = 0x5502
EV_SYN = 0x00
EV_KEY = 0x01
SHIFT = 42

US_QWERTY = {    " ": (57, False), "\n": (28, False), "\t": (15, False),
    "a": (30, False), "b": (48, False), "c": (46, False), "d": (32, False),
    "e": (18, False), "f": (33, False), "g": (34, False), "h": (35, False),
    "i": (23, False), "j": (36, False), "k": (37, False), "l": (38, False),
    "m": (50, False), "n": (49, False), "o": (24, False), "p": (25, False),
    "q": (16, False), "r": (19, False), "s": (31, False), "t": (20, False),
    "u": (22, False), "v": (47, False), "w": (17, False), "x": (45, False),
    "y": (21, False), "z": (44, False),
    "1": (2, False), "2": (3, False), "3": (4, False), "4": (5, False),
    "5": (6, False), "6": (7, False), "7": (8, False), "8": (9, False),
    "9": (10, False), "0": (11, False),
    ".": (52, False), ",": (51, False), "/": (53, False), ";": (39, False),
    "'": (40, False), "-": (12, False), "=": (13, False), "[": (26, False),
    "]": (27, False), "\\": (43, False), "`": (41, False),
    "A": (30, True), "B": (48, True), "C": (46, True), "D": (32, True),
    "E": (18, True), "F": (33, True), "G": (34, True), "H": (35, True),
    "I": (23, True), "J": (36, True), "K": (37, True), "L": (38, True),
    "M": (50, True), "N": (49, True), "O": (24, True), "P": (25, True),
    "Q": (16, True), "R": (19, True), "S": (31, True), "T": (20, True),
    "U": (22, True), "V": (47, True), "W": (17, True), "X": (45, True),
    "Y": (21, True), "Z": (44, True),
    "!": (2, True), "@": (3, True), "#": (4, True), "$": (5, True),
    "%": (6, True), "^": (7, True), "&": (8, True), "*": (9, True),
    "(": (10, True), ")": (11, True), "?": (53, True), ":": (39, True),
    '"': (40, True), "_": (12, True), "+": (13, True), "{": (26, True),
    "}": (27, True), "|": (43, True), "<": (51, True), ">": (52, True),
    "~": (41, True),
}

# Special-key tokens usable in a keys file. Deliberately NO Enter: it submits
# or drops fields in many apps. Values are LINUX KEYCODES (input-event-codes.h)
# — uinput consumes these directly; the AT set-1 numbers are wrong here and
# once mapped {end} to KEY_VOLUMEUP (115) and {home} to KEY_INSERT (110).
SPECIAL_KEYS = {
    "{backspace}": 14, "{delete}": 111, "{left}": 105, "{right}": 106,
    "{up}": 103, "{down}": 108, "{home}": 102, "{end}": 107,
    "{tab}": 15, "{esc}": 1, "{space}": 57,
}
TOKEN_RE = re.compile(r"\{[a-z]+\}?", re.IGNORECASE)


def tokenize(text):
    """Split a keys file into ('char', ch) and ('key', keycode) tokens."""
    tokens = []
    pos = 0
    for match in TOKEN_RE.finditer(text):
        for ch in text[pos:match.start()]:
            tokens.append(("char", ch))
        token = match.group(0)
        key = token.lower()
        if key in SPECIAL_KEYS:
            tokens.append(("key", SPECIAL_KEYS[key]))
        elif token != "{}":
            print(f"warning: unknown token {token!r} skipped")
        pos = match.end()
    for ch in text[pos:]:
        tokens.append(("char", ch))
    return tokens

# struct uinput_user_dev on 64-bit: name[80] + input_id (4*u16) + ff_effects_max
# (u16) + 2 bytes padding + four 64-entry s32 abs arrays = 1116 bytes. This
# kernel rejects UI_DEV_SETUP, the legacy write is what works.
UINPUT_USER_DEV_SIZE = 1116


def archive_engine_log(label, t_start_ms=None, out_dir=LOGS_DIR):
    """Archive the engine's debug log for a test run.

    Copies entries from `t_start_ms` on (or the whole file when None) into
    `out_dir/<label>_<stamp>.log` and returns the path, or None when the log
    does not exist (debug logging off). Logs may contain short surrounding-
    text snippets typed by the user, so archives stay out of git.
    """
    if not os.path.exists(DEBUG_LOG):
        return None
    os.makedirs(out_dir, exist_ok=True)
    stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
    safe = re.sub(r"[^A-Za-z0-9._-]+", "_", label)[:40].strip("_") or "run"
    path = os.path.join(out_dir, f"{safe}_{stamp}.log")
    with open(DEBUG_LOG, encoding="utf-8") as src, open(path, "w", encoding="utf-8") as dst:
        for line in src:
            if t_start_ms is None:
                dst.write(line)
                continue
            head = line.split(" ", 1)[0]
            if head.isdigit() and int(head) >= t_start_ms - 100:
                dst.write(line)
    return path


def read_focused_text():
    """Best-effort read of the focused text entry via AT-SPI (GNOME
    accessibility). Returns the field text, or None when nothing focused is
    readable (accessibility off, non-GUI target, bindings missing). Never
    raises — verification must not kill a typing run."""
    try:
        import gi
        gi.require_version("Atspi", "2.0")
        from gi.repository import Atspi

        desktop = Atspi.get_desktop(0)
        # this gi version names the text-entry role ENTRY, not TEXT_ENTRY
        text_roles = {Atspi.Role.ENTRY, Atspi.Role.TEXT, Atspi.Role.PARAGRAPH,
                      Atspi.Role.DOCUMENT_TEXT, Atspi.Role.TERMINAL}

        def walk(obj, depth):
            if depth > 14:
                return None
            try:
                if (obj.get_role() in text_roles
                        and obj.get_state_set().contains(Atspi.StateType.FOCUSED)):
                    iface = obj.query_text()
                    return iface.get_text(0, iface.get_character_count())
            except Exception:
                return None
            try:
                count = obj.get_child_count()
            except Exception:
                return None
            for i in range(count):
                found = walk(obj.get_child_at_index(i), depth + 1)
                if found is not None:
                    return found
            return None

        for i in range(desktop.get_child_count()):
            found = walk(desktop.get_child_at_index(i), 0)
            if found is not None:
                return found
    except Exception:
        return None
    return None


def verify_typed(expected):
    """Compare the focused field's tail against the expected on-screen text.
    Returns True/False, or None when verification is impossible."""
    actual = read_focused_text()
    if actual is None:
        print("  verify: cannot read the focused field via AT-SPI - compare manually")
        print(f"  expected tail: ...{expected[-60:]}")
        return None
    if actual.endswith(expected):
        print(f"  verify: PASS ({len(expected)} chars match the focused field)")
        return True
    depth = 0
    limit = min(len(expected), len(actual))
    while depth < limit and actual[-1 - depth] == expected[-1 - depth]:
        depth += 1
    print(f"  verify: FAIL - first mismatch {depth} chars from the end")
    print(f"    expected ...{expected[len(expected) - depth - 25:len(expected) - depth]}"
          f"[{expected[len(expected) - depth - 1:len(expected) - depth]!r}]"
          f"{expected[len(expected) - depth:depth and len(expected) or 60]}")
    print(f"    actual   ...{actual[len(actual) - depth - 25:len(actual) - depth]}"
          f"[{actual[len(actual) - depth - 1:len(actual) - depth]!r}]"
          f"{actual[len(actual) - depth:depth and len(actual) or 60]}")
    return False


def open_device():
    try:
        fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    except OSError as err:
        sys.exit(
            f"cannot open /dev/uinput: {err}\n"
            "Set up permissions first — see docs/install.md (udev rule + input group)."
        )
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_SYN)
    fcntl.ioctl(fd, UI_SET_EVBIT, EV_KEY)
    # every keycode the tool may send — the kernel silently drops events for
    # keycodes that were not registered here (special keys once vanished this
    # way: Backspace presses disappeared without any error)
    codes = {key for key, _ in US_QWERTY.values()} | set(SPECIAL_KEYS.values()) | {SHIFT}
    for code in codes:
        fcntl.ioctl(fd, UI_SET_KEYBIT, code)
    setup = struct.pack("80s5H", b"mangrs-typing-tool", 0x03, 0x1234, 1, 1, 0)
    os.write(fd, setup + b"\0" * (UINPUT_USER_DEV_SIZE - len(setup)))
    fcntl.ioctl(fd, UI_DEV_CREATE)
    return fd


def emit(fd, etype, code, value):
    os.write(fd, struct.pack("=QQHHi", 0, 0, etype, code, value))


def press(fd, code, shift):
    if shift:
        emit(fd, EV_KEY, SHIFT, 1)
        emit(fd, EV_SYN, 0, 0)
    emit(fd, EV_KEY, code, 1)
    emit(fd, EV_SYN, 0, 0)
    emit(fd, EV_KEY, code, 0)
    emit(fd, EV_SYN, 0, 0)
    if shift:
        emit(fd, EV_KEY, SHIFT, 0)
        emit(fd, EV_SYN, 0, 0)


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
    parser.add_argument("--file", default=os.path.join(os.path.dirname(__file__), "text-test"))
    parser.add_argument("--delay", type=int, default=30, help="seconds before typing starts")
    parser.add_argument("--interval", type=int, default=80, help="ms between key presses")
    parser.add_argument("--modes", default=",".join(MODES),
                        help="comma-separated display modes cycled after each full workload")
    parser.add_argument("--pause", type=int, default=2500,
                        help="ms to wait after each mode switch (engine reloads in ~1 s)")
    args = parser.parse_args()

    try:
        with open(args.file, encoding="utf-8") as handle:
            text = handle.read()
    except OSError as err:
        sys.exit(f"cannot read {args.file}: {err}")
    # Enter is dangerous: in search boxes and chat inputs it submits or drops
    # the field. Newlines in the keys file become plain spaces instead.
    text = text.replace("\n", " ").replace("\r", " ")

    expected_path = expected_path_for(args.file)
    expected = None
    try:
        with open(expected_path, encoding="utf-8") as handle:
            expected = handle.read()
    except OSError:
        print(f"note: no expected-output file at {expected_path} - typed text "
              "cannot be verified automatically")

    tokens = tokenize(text)
    unknown = sorted({ch for kind, ch in tokens if kind == "char" and ch not in US_QWERTY})
    if unknown:
        print(f"warning: characters without a key mapping will be skipped: {unknown!r}")

    modes = [m.strip() for m in args.modes.split(",") if m.strip()]
    original_mode = read_engine_mode()
    t_start = time.time() * 1000
    print(
        f"Typing {len(text)} characters from {args.file}, once per display mode:\n"
        f"  {' -> '.join(modes)}\n"
        "Click into the target input field NOW and keep it focused (Ctrl+C cancels).\n"
        "note: per-app rules override the global mode - test in an app without a "
        "matching rule, or the mode switch will not take effect."
    )
    for remaining in range(args.delay, 0, -1):
        print(f"\r{remaining:3d}s ", end="", flush=True)
        time.sleep(1)
    print()

    try:
        fd = open_device()
        try:
            for round_index, mode in enumerate(modes):
                set_engine_mode(mode)
                time.sleep(1.5)  # engine hot-reloads config within ~1 s
                print(f"--- display mode {round_index + 1}/{len(modes)}: {mode}")
                for kind, value in tokens:
                    if kind == "char":
                        if value not in US_QWERTY:
                            continue
                        press(fd, *US_QWERTY[value])
                    else:
                        press(fd, value, False)
                    time.sleep(args.interval / 1000)
                time.sleep(0.5)  # let the last commit land before verifying
                if expected is not None:
                    verify_typed(expected)
                if round_index < len(modes) - 1:
                    time.sleep(args.pause / 1000)
            time.sleep(0.3)  # let the engine flush its last log entries
        finally:
            fcntl.ioctl(fd, UI_DEV_DESTROY)
            os.close(fd)
    finally:
        if original_mode:
            try:
                set_engine_mode(original_mode)
                print(f"original display mode restored: {original_mode}")
            except SystemExit:
                print(f"could not restore original mode ({original_mode}) - set it manually")

    archive = archive_engine_log("typing", t_start)
    if archive:
        print(f"engine debug log archived to {archive}")
    else:
        print("no engine debug log found (debug_log = false?) — nothing archived")
    print("done")


if __name__ == "__main__":
    main()
