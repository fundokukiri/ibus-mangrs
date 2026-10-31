#!/usr/bin/env python3
"""Automated test for the click-mid-preedit bug (#14).

Types a sentence plus one word left pending in preedit, then clicks the
mouse into the focused field (through a virtual absolute pointer on
/dev/uinput, at the screen position of a character obtained from AT-SPI),
and finally reads the field back: the pending word must have landed at the
position where it was typed — not at the click position, not lost, not
corrupting the sentence.

Usage:
    python3 click_tool.py --delay 10              # click at start/middle/end
    python3 click_tool.py --at middle             # one click position only
    python3 click_tool.py --modes preedit,nonpreedit

The workload: `xin chaof cac ban choaf` — the engine renders
`xin chào các bạn ` with `chào` still pending in preedit. Expected final
text (after the settling space): `xin chào các bạn chào `.

Requires a writable /dev/uinput (keyboard + pointer), GNOME accessibility
for the read-back, and GTK3 for the screen size (falls back to --screen).
"""

import argparse
import fcntl
import os
import struct
import sys
import time

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import typing_tool as tt  # reuse the uinput keyboard, config and verify code

UI_SET_ABSBIT = 0x40045567
EV_ABS = 0x03
EV_KEY = 0x01
ABS_X, ABS_Y = 0, 1
ABS_MAX = 32767
BTN_LEFT = 0x110

# Raw keystrokes (Telex): the engine turns them into
# `xin chào các bạn ` with the last `chào` still pending in preedit.
PREFIX_KEYS = "xin chaof cac ban "
WORD_KEYS = "choaf"
WORD = "chào"
SETTLING = " "  # terminator typed after the click, commits the pending word


def screen_size():
    try:
        import gi
        gi.require_version("Gdk", "3.0")
        from gi.repository import Gdk
        screen = Gdk.Screen.get_default()
        return screen.get_width(), screen.get_height()
    except Exception:
        return None


def open_pointer():
    """Virtual absolute pointer (tablet-style): mutter maps ABS 0..ABS_MAX
    onto the full screen, so a coordinate click is deterministic."""
    fd = os.open("/dev/uinput", os.O_WRONLY | os.O_NONBLOCK)
    fcntl.ioctl(fd, tt.UI_SET_EVBIT, EV_KEY)
    fcntl.ioctl(fd, tt.UI_SET_KEYBIT, BTN_LEFT)
    fcntl.ioctl(fd, tt.UI_SET_EVBIT, EV_ABS)
    fcntl.ioctl(fd, UI_SET_ABSBIT, ABS_X)
    fcntl.ioctl(fd, UI_SET_ABSBIT, ABS_Y)
    payload = struct.pack("80s5H", b"mangrs-click-tool", 0x03, 0x1234, 2, 1, 0)
    payload += b"\0\0"  # ff padding
    payload += struct.pack("64i", *([ABS_MAX, ABS_MAX] + [0] * 62))  # absmax
    payload += b"\0" * (64 * 4 * 3)  # absmin, absfuzz, absflat
    assert len(payload) == tt.UINPUT_USER_DEV_SIZE, len(payload)
    os.write(fd, payload)
    fcntl.ioctl(fd, tt.UI_DEV_CREATE)
    return fd


def move_to(fd, x, y, screen_w, screen_h):
    ax = int(x * ABS_MAX / screen_w)
    ay = int(y * ABS_MAX / screen_h)
    tt.emit(fd, EV_ABS, ABS_X, ax)
    tt.emit(fd, EV_ABS, ABS_Y, ay)
    tt.emit(fd, tt.EV_SYN, 0, 0)


def click_at(fd, x, y, screen_w, screen_h):
    move_to(fd, x, y, screen_w, screen_h)
    time.sleep(0.15)
    tt.emit(fd, EV_KEY, BTN_LEFT, 1)
    tt.emit(fd, tt.EV_SYN, 0, 0)
    time.sleep(0.06)
    tt.emit(fd, EV_KEY, BTN_LEFT, 0)
    tt.emit(fd, tt.EV_SYN, 0, 0)


def read_focused_object():
    """The focused text widget itself (not just its text), for extents."""
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi

    desktop = Atspi.get_desktop(0)
    text_roles = {Atspi.Role.ENTRY, Atspi.Role.TEXT, Atspi.Role.PARAGRAPH,
                  Atspi.Role.DOCUMENT_TEXT, Atspi.Role.TERMINAL}

    def walk(obj, depth):
        if depth > 14:
            return None
        try:
            if (obj.get_role() in text_roles
                    and obj.get_state_set().contains(Atspi.StateType.FOCUSED)):
                return obj
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
    return None


def char_center(obj, index):
    """Screen coordinates of the center of character `index`."""
    import gi
    gi.require_version("Atspi", "2.0")
    from gi.repository import Atspi
    rect = obj.query_text().get_character_extents(index, Atspi.CoordType.SCREEN)
    try:
        x, y, w, h = rect  # GI maps the out-params to a plain tuple
    except TypeError:
        x, y, w, h = rect.x, rect.y, rect.width, rect.height
    return x + w // 2, y + h // 2


def click_index(text, where):
    """Character index to click on, chosen within the committed text."""
    if where == "start":
        return 0
    if where == "end":
        return len(text) - 1
    return len(text) // 2


def type_keys(fd, keys, interval_ms):
    for ch in keys:
        if ch not in tt.US_QWERTY:
            print(f"warning: no key mapping for {ch!r}, skipped")
            continue
        tt.press(fd, *tt.US_QWERTY[ch])
        time.sleep(interval_ms / 1000)


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--delay", type=int, default=15, help="seconds before typing starts")
    parser.add_argument("--interval", type=int, default=80, help="ms between key presses")
    parser.add_argument("--at", default="all", choices=["start", "middle", "end", "all"],
                        help="click position within the field text")
    parser.add_argument("--modes", default="preedit",
                        help="display modes to cycle (the bug is preedit-only; "
                             "default just preedit)")
    parser.add_argument("--screen", help="WxH fallback when GTK3 is unavailable")
    args = parser.parse_args()

    size = screen_size()
    if size is None and args.screen:
        w, h = args.screen.lower().split("x")
        size = (int(w), int(h))
    if size is None or not size[0]:
        sys.exit("cannot determine the screen size: install GTK3 or pass --screen 1920x1080")

    positions = ["start", "middle", "end"] if args.at == "all" else [args.at]
    modes = [m.strip() for m in args.modes.split(",") if m.strip()]
    original_mode = tt.read_engine_mode()
    t_start = time.time() * 1000
    print(
        "Click-mid-preedit test (#14): type a pending word, click into the field,\n"
        "verify the word lands where it was typed.\n"
        f"  keys:    {tt.PREFIX_KEYS!r} + {tt.WORD_KEYS!r} (pending {tt.WORD!r})\n"
        "  click:   " + " -> ".join(positions) + "\n"
        "  expect:  word at its typed position after the click, sentence intact\n"
        "Focus the target input field NOW (Ctrl+C cancels)."
    )
    for remaining in range(args.delay, 0, -1):
        print(f"\r{remaining:3d}s ", end="", flush=True)
        time.sleep(1)
    print()

    failures = 0
    try:
        kfd = tt.open_device()
        pfd = open_pointer()
        try:
            for mode in modes:
                tt.set_engine_mode(mode)
                time.sleep(1.5)
                for where in positions:
                    print(f"--- mode {mode}, click at {where}")
                    type_keys(kfd, tt.PREFIX_KEYS + tt.WORD_KEYS, args.interval)
                    time.sleep(0.4)
                    obj = read_focused_object()
                    if obj is None:
                        print("  FAIL: no focused text field found via AT-SPI")
                        failures += 1
                        continue
                    iface = obj.query_text()
                    text = iface.get_text(0, iface.get_character_count())
                    idx = click_index(text, where)
                    x, y = char_center(obj, idx)
                    print(f"  clicking char {idx} of {len(text)} at ({x}, {y})")
                    click_at(pfd, x, y, *size)
                    time.sleep(1.2)  # engine resolves the click (~8 ms) + settle
                    after_click = iface.get_text(0, iface.get_character_count())
                    word_there = after_click.endswith(tt.PREFIX_KEYS.replace("chaof", WORD) + tt.WORD)
                    print(f"  after click: word at typed position: {'YES' if word_there else 'NO'}")
                    type_keys(kfd, tt.SETTLING, args.interval)
                    time.sleep(0.5)
                    final = iface.get_text(0, iface.get_character_count())
                    wanted = tt.PREFIX_KEYS.replace("chaof", WORD) + tt.WORD + tt.SETTLING
                    if final.endswith(wanted):
                        print("  verify: PASS (word delivered to its typed position)")
                    else:
                        failures += 1
                        print(f"  verify: FAIL\n    expected tail: ...{wanted!r}\n"
                              f"    actual tail:   ...{final[-60:]!r}")
                    time.sleep(1.0)
        finally:
            fcntl.ioctl(kfd, tt.UI_DEV_DESTROY)
            os.close(kfd)
            fcntl.ioctl(pfd, tt.UI_DEV_DESTROY)
            os.close(pfd)
    finally:
        if original_mode:
            try:
                tt.set_engine_mode(original_mode)
                print(f"original display mode restored: {original_mode}")
            except SystemExit:
                print("could not restore the original mode - set it manually")

    archive = tt.archive_engine_log("click", t_start)
    if archive:
        print(f"engine debug log archived to {archive}")
    print(f"done ({'all passed' if failures == 0 else f'{failures} FAILURES'})")


if __name__ == "__main__":
    main()
