# Architecture — MăngRS (ibus-mangrs-core)

Vietnamese IME engine for IBus in Rust on `bamboo-core`, Wayland + X11, GTK4 settings window
(feature `gui`). Crate `ibus_mangrs`, binary `ibus-engine-ibus-mangrs`, bus name
`org.freedesktop.IBus.mangrs`, engine name `ibus-mangrs` — all must match `data/ibus-mangrs.xml`
or the daemon silently never spawns the engine.

## Display modes

| Mode | How it works |
|------|--------------|
| **Preedit** (default) | word shows inline with underline, commits at word end; sent with `IBUS_ENGINE_PREEDIT_COMMIT` so a click/focus change commits it too |
| NonPreedit | every key commits immediately, tail rewritten on marks via `DeleteSurroundingText` |
| Surrounding | NonPreedit + verifies `SetSurroundingText` against the believed `tail` and repairs drift |
| Smooth (uinput) | raw keys forwarded while typing, latin tail erased with real Backspaces at word end |
| Super Smooth (uinput) | same, without the safety check before rewriting |
| Select (uinput) | Shift+Left selects the tail, Delete erases it, word commits |
| OFF | Vietnamese off, macros still work |
| Default Typing | engine fully inert, every key forwarded |

The uinput modes need `/dev/uinput` and are intended for X11; on GNOME Wayland they resolve to
preedit wherever real-key injection cannot reach the application.

Mode resolution (`Engine::effective_mode`): hard fallback (no `IBUS_CAP_PREEDIT_TEXT` →
non-preedit) → `[[app_rules]]` substring match on client prgname → built-in heuristic for
unlisted delete-dropping clients. A **word-mode lock** freezes the resolved mode for the whole
word to prevent mid-word preedit↔non-preedit flips.

## Layout

```
src/
├── server.rs           D-Bus: connection, message filter, signals, PropertyActivate,
│                       cycle hotkey, config hot-reload (key path + idle poll + FocusIn +
│                       ReloadConfig nudge), key-injection transports
├── engine.rs           Protocol-free state machine: keys → Actions. Modes, word-mode lock,
│                       tail tracking, surrounding verify/repair, macros, auto-restore
├── bamboo_wrapper.rs   ONLY module touching bamboo-core; computes deltas by diffing
│                       its own last_output (never trusts bamboo's delta)
├── macros.rs           macros.toml + dictionary.txt in ~/.config/ibus-mangrs/
├── config.rs           TOML at ~/.config/ibus-mangrs/config.toml, shared engine+GUI;
│                       migrates legacy unikey / r-unikey config dirs
├── golden.rs           678 golden cases (102 Telex + 96 VNI + 24 VIQR rows x 3
│                       variants, 12 Microsoft rows) through the real engine into
│                       a simulated app buffer
├── ibus_prop.rs        IBusProperty/IBusPropList encoding for the input-source menu
├── ibus_text.rs        GVariant encode/decode IBusText/IBusAttrList
├── keymap.rs           X11 keysym → char, modifier masks
├── uinput.rs           Virtual single-key keyboard (/dev/uinput) for Backspace/selection keys
├── paste.rs            Clipboard commit channel for the uinput modes (Ctrl+V with restore)
├── backup.rs           Backup/restore of config + macros + dictionary as one JSON (v1)
├── debug_log.rs        Debug log at ~/.cache/ibus-mangrs/debug.log
└── gui.rs              GTK4 settings window: one window, 5 pages (Kiểu gõ / Dấu & Sửa lỗi /
│                       Gõ tắt / Dữ liệu / Giới thiệu), feature "gui"
```

One unit test (`engine::keys_workload_probe`) reads the `test/keys-test` fixture — keep it in
the repo.

## Layering

```
server.rs (D-Bus, IBus protocol)
  └─ engine.rs (state machine, no D-Bus types)  →  emits Actions
       └─ bamboo_wrapper.rs (bamboo-core touch point)  →  Outcome::{Edit, Finish, PassThrough}
```

`server.rs` turns `Action`s into signals: `CommitText`, `UpdatePreeditText`, `HidePreedit`,
`DeleteSurroundingText` (protocol) or `Uinput::inject_backspaces` (real Backspace keys),
`RewriteByBackspace` (press k+extra Backspaces, re-commit the common suffix), `Paste` (clipboard
channel). Key path: message filter → config hot-reload check → cycle hotkey →
`Engine::process_char` → `key_mode()` (resolve + lock) → `verify_surrounding()` → wrapper →
`Outcome` → actions → emit.

### Config hot-reload (four trigger paths, one watch)

`ConfigWatch` (in `server.rs`) stats the config mtime at most once a second and re-baselines
after the engine's own saves so it never reloads itself. Reload happens from:

1. **Key events** — `ProcessKeyEvent` checks the watch first (the original path).
2. **The idle loop** — `Server::run` polls the watch every iteration (`idle_tick`), so
   mouse-only GUI edits apply with no key events.
3. **FocusIn** — every focus change re-reads the config.
4. **`ReloadConfig`** — a private method (`org.freedesktop.IBus.mangrs` interface, engine object
   path) the GUI calls after every save (including JSON import) for an instant refresh; the GUI
   also re-emits `RegisterProperties` from the engine side via this reload.

The nudge is best-effort: see "Known quirks" — the idle poll is the guarantee (~1s), never build
anything that depends on the nudge alone.

**Preedit carries `IBUS_ENGINE_PREEDIT_COMMIT`, so a click or focus change never loses the
word.** `Action::UpdatePreedit` is sent as `UpdatePreeditText(text, cursor, true, mode)` with
`mode = 1` (`IBUS_ENGINE_PREEDIT_COMMIT`; 0 is `CLEAR`). When the client resets its input
context — GTK does this on a mouse click, a focus change and a button press — it **commits the
preedit string it is currently showing** instead of clearing it: ibus's IM module
(`client/gtk2/ibusimcontext.c`, the gtk3/gtk4 files are symlinks to it) has
`ibus_im_context_clear_preedit_text()`, which emits the `commit` signal when
`preedit_mode == IBUS_ENGINE_PREEDIT_COMMIT`, and `ibus_im_context_reset()` /
`focus_out()` / the GTK button-press handler all call it. ibus-bamboo sends the same mode; this
is what makes its preedit survive a click.

Consequences the engine must honour:

- The engine's own `Reset` and `FocusOut` handlers **drop** the composition without committing
  (`drop_pending_word`): the client already committed the word, and committing it again would
  duplicate it.
- The client commits whatever the preedit **displays**, so the preedit shows the **macro
  expansion** when the raw keys are a macro key (`Engine::preedit_text`); otherwise a click would
  commit the raw `gs` instead of `giờ sáng` (bamboo shows the expansion for the same reason).
- For the same reason the preedit also shows the **auto-restore to the raw keys** of a
  non-Vietnamese word (`windowf`). That restore uses the prefix verdict (see the wrapper
  paragraph), so a still-valid prefix like `đuo` stays composed instead of flashing `dduo`.
- Hiding the preedit (`Action::HidePreedit`) keeps `mode = 0` (`CLEAR`), so hiding at a word end
  never commits the old word.

## Engine Invariants (each earned by a real crash or bug — keep them)

**Delete transport.** Protocol `DeleteSurroundingText` to a client whose buffer was never received
segfaults mutter (`g_utf8_offset_to_pointer`, GNOME logs out; Shell logs "input-method engines
should not call the delete-surrounding-text API..."). Hard rules:

- Never protocol-delete without **both** the `IBUS_CAP_SURROUNDING_TEXT` capability **and** an
  actual ST report; clamp every delete count to what the last report says exists.
- Never protocol-delete or repair-delete on relay clients (`gnome-shell`, `mutter`, `fake`).
- Clients without ST data, or proven to swallow the protocol delete, get **real Backspace presses
  via `/dev/uinput`** (10ms interval, 10ms post-delay — ibus-bamboo's params).
- When injecting, **defer the commit** until the injected-key echoes arrive (`pending_injected`
  counts them; those echoes are forwarded untouched — they ARE the deletion) or a 150ms stale
  timeout flushes. A synchronous commit outruns the compositor path ("accent flashes then
  reverts"). `RewriteByBackspace {backspaces, commit}` presses k+extra keys then re-commits the
  common suffix so the deletion lands correctly mid-word.

| Client type | Transport |
|-------------|-----------|
| GTK3/GTK4 Wayland (relayed, report ST) | Protocol delete |
| Chrome/Electron (report ST, drop deletes) | Protocol first, uinput after a drop is detected |
| XWayland / Sublime / JetBrains (no client id, no ST) | uinput Backspace |
| `gnome-shell` / `mutter` / `fake` (relay, no typing) | none |

**Stale ST reports are classified by content** (fast typing makes reports predate our commit):

1. `actual` ends with `tail` → healthy, no repair.
2. Extra chars right before the common suffix → repair delete / inject.
3. `actual` contains `tail` as a substring (app inserted around the word — Chrome suggestions) →
   rebuild the composition from the real text (`rebuild_from_text`; safe because the wrapper
   diffs its own `last_output`).
4. `tail` not in `actual` → **ignore silently** (stale or user-deleted). Treating this as an app
   edit used to tear down the composition and eat characters. Reports where the word is not
   adjacent to the caret (>2 chars away — Chrome search bar) fall here too.

**bamboo-core quirks (probed, not documented upstream).** Its `EditDelta` is relative to internal
`prev_preedit`, which `remove_last_output_char` does not refresh — the key after Backspace deletes
an already-gone char (`tie` → BS → `n` produced `tn`); the wrapper diffs `last_output` instead.
After a tone mark is undone the internal state stays **poisoned** — the next key resurfaces a
hidden char (`too`+`l` → `toool`, output longer than raw keys); the wrapper uses the displayed
`output_override` and falls back to raw keys for that word. A digit under Telex/VIQR **clears the
entire composition** — digits and `_` are word terminators intercepted in `bamboo_wrapper`
**before** the engine sees them (all terminators must be: bamboo's own handling would delete the
already-committed word; bracket keys stop being terminators while bracket mode is on). Telex `w`
transforms the adjacent vowel (u→ư, o→ơ, a→ă, `aa/ee/oo` → â/ê/ô, `aw` → ă), no auto `uo`→`ươ`.
VNI digits go right after the vowel they transform (6=â/ê/ô, 7=ơ/ư, 8=ă, 9=đ, tones 1..5).
Microsoft layout is append-style (`ti3ng8` → `tiếng`) and cannot type `đ` from `d`.
`auto_correct` (on by default) reverts invalid intermediate syllables. Two bamboo-core verdicts
drive the wrapper: `is_valid_word()` (a complete syllable) gates macro expansion, and
`is_valid_prefix()` (also accepts a still-extendable state like `đuo`) drives auto-restore of
non-Vietnamese words — the prefix check is what ibus-bamboo uses while typing, while the
complete-word one restored every half-typed word to its raw keys (`đuo` → `dduo`).

**Charsets.** 11 charsets from `bamboo-core::encode_charset` (Unicode default, NFD, TCVN3, VNI,
VIQR, VISCII, VPS, CP1258, BKHCM, Vietware X/Full). Encoding happens only on text sent to the app
(`Action::Commit`), never on preedit. Legacy encodings change the character count (VNI `â`→`aâ`),
so the whole current word is rewritten with a correct delete count instead of replaying the
Unicode delta; Unicode keeps the delta path.

## IBus Protocol Facts (verified on the wire — do not re-derive)

| Fact | Detail |
|------|--------|
| Bus | Private bus of ibus-daemon, NOT the session bus. `Connection::open_private(addr)` then `conn.register()` — open_private skips Hello; every call fails AccessDenied without it. Address: `IBUS_ADDRESS` env, else the newest `~/.config/ibus/bus/*` registry file |
| Minimal bus | The bus is served by ibus-daemon itself, not a full dbus-daemon: bus-management methods like `GetConnectionUnixProcessID` do not exist, and generic method calls from an arbitrary peer to the engine name may go unanswered (see Known quirks) |
| Method replies | Must be answered **from the message filter** (`replace_message_callback`, return `true` to claim). A later `MsgHandler` reply is too late (libdbus already answered UnknownMethod); registering an object path is worse — its vtable swallows the message |
| CreateEngine | Returns `o` (object path). Sending `s` makes the daemon fail to build its proxy with only a GLib critical — engine never starts |
| FocusId / ActiveSurroundingText | **Booleans on the wire** (`g_variant_get_boolean` in engineproxy.c); any other type → daemon warns and falls back. `FocusId = true` makes the daemon send `FocusInId(s path, s client)` — the only way to learn which app is typing |
| client ids | `gtk4-im:<prgname>` / `gtk3-im:<prgname>` / `fake` for the daemon's hotkey context. `fake` may interleave between real apps; `Engine::set_client` ignores it and resets only on a real client change. Some apps (Sublime, X11 XIM bridge) never send a client id — App Rules cannot match them |
| DeleteSurroundingText | `(iu)` — signed offset + count, no variant wrapping (other signals wrap args in `v`) |
| IBusText | `('IBusText', @a{sv} {}, 'str', <('IBusAttrList', @a{sv} {}, [attrs])>)`; incoming string is tuple field 2 (`ibus_text::text_from_variant`) |
| SetSurroundingText | `(v text, u cursor, u anchor)` on the engine object; only data source for Surrounding mode. The engine must emit `RequireSurroundingText` on Enable/FocusIn or GTK IM modules never send it (found via `strings` on ibus-bamboo's binary) |

### Property menu (input-source popup)

- `RegisterProperties(v)` / `UpdateProperty(v)` are **signals** the engine emits on its own object
  path; the daemon relays them to the panel. The click comes back as the **method**
  `PropertyActivate(s name, u state)`.
- `RegisterProperties` is (re-)emitted on Enable/FocusIn, after every radio/toggle click, and on
  every config reload path — a mid-session re-register updates the panel's menu content.
- GNOME Shell sends **one call per item of a clicked radio group**: picked item `CHECKED`, every
  sibling `UNCHECKED` (`keyboard.js::_buildPropSubMenu`). Ignore UNCHECKED — honoring them made
  the last sibling always win (the "always VIQR / always preedit" bugs). After a click, re-emit
  `UpdateProperty` for **every** child of the group or the previous checkmark persists.
- Clicking any item closes the whole popup (`popupMenu.js itemActivated`) — Shell behavior, not
  influenceable from the engine. An engine cannot open the panel popup itself, so no Mode Menu
  Hotkey.
- `IBusProperty` is a 12-field tuple: `('IBusProperty', a{sv} {}, key, type, label:v, icon:s,
  tooltip:v, sensitive:b, visible:b, state:u, sub_props:v, symbol:v)`; types NORMAL 0 / TOGGLE 1 /
  RADIO 2 / MENU 3 / SEPARATOR 4; states UNCHECKED 0 / CHECKED 1. `IBusPropList` is
  `('IBusPropList', a{sv} {}, av [props])`.
- Signatures verified against ibus 1.5.34 by capturing ibus-bamboo with `dbus-monitor`.

## Configuration

`~/.config/ibus-mangrs/config.toml` (TOML, shared engine + GUI), hot-reloaded (see the four
reload paths above) — no `ibus restart` needed for config changes. Keys: `enabled`, `method`
(telex/vni/viqr/microsoft), `mode` (8 values, keys as in the mode table), `charset`,
`modern_tone_style`, `free_tone_marking`, `auto_correct`, `auto_restore`, `bypass_url`, `w2u`,
`bracket`, `em_dash`, `auto_capitalize`, `double_space_period`, `macro_enabled`,
`capitalize_macro`, `macro_in_off_mode`, `time_format`, `date_format`, `dictionary_enabled`,
`abbreviations` (`ddc` → `đc` — bamboo's auto_correct reverts non-syllable compositions and the
wrapper re-applies the dd→đ transform), `cycle_mode_hotkey`, `hide_underline`,
`commit_interval_ms`, `backspace_interval_ms`, `post_delay_ms` (uinput pacing),
`unicode_channel` (commit/clipboard), `debug_log`, `[[app_rules]]` (`pattern` + `mode`).

**App Rules**: case-insensitive substring on the prgname from `FocusInId`; built-in defaults
Chrome/Electron → preedit, Sublime → surrounding. The server records seen clients (newest 50) in
`~/.cache/ibus-mangrs/clients` for the GUI's quick-add dropdown.

**uinput**: `/dev/uinput` needs udev rule `80-uinput.rules` (installed to `/etc/udev/rules.d/`
by `scripts/install.sh`, to `/usr/lib/udev/rules.d/` by the PKGBUILD) + group `input`. This
kernel rejects `UI_DEV_SETUP` (EINVAL) — the legacy path (write a `struct uinput_user_dev`
straight to the fd, then `UI_DEV_CREATE`) is what `Uinput::open` uses.

## Testing

```sh
cargo test                    # 209 tests incl. 678 golden cases — no D-Bus or daemon needed
cargo clippy --all-targets    # must stay clean; cargo fmt too
cargo build --features gui    # the -gui settings window needs this feature
```

Live test without installing (-t refresh is required or the daemon serves a stale registry):

```sh
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

Install (root): `sudo ./scripts/install.sh` → `/usr/lib/ibus-mangrs/` + `/usr/share/ibus/component/`,
then `ibus restart`. `~/.local/share/ibus/component/` is **not** scanned on this machine — only
`IBUS_COMPONENT_PATH` and `/usr/share/ibus/component` work. A `PKGBUILD` exists for Arch
packaging. **Restart IBus after reinstalling the binary**: `cp` over `/usr/lib` replaces the file
in place, but the running engine keeps its old code until the process is restarted — a stale
engine explains "the fix works in tests but not on the desktop".

**Offline end-to-end smoke test** (no system IBus touched): run the engine under
`dbus-run-session` with `IBUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"` and a temp `XDG_CONFIG_HOME`,
edit the config file, drive the engine with `dbus-send` (e.g. the private `ReloadConfig` call),
and watch `RegisterProperties` appear with `dbus-monitor --address "$ADDR"`. This catches
wiring bugs without a live GNOME session.

Live checks are driven over D-Bus directly (no keyboard injector on this machine):

```
ADDR=$(grep -o 'unix:path=[^,]*' ~/.config/ibus/bus/*)
M() { gdbus call --address "$ADDR" --dest org.freedesktop.IBus.mangrs \
      --object-path /org/freedesktop/IBus/Engine/1 \
      --method "org.freedesktop.IBus.Engine.$1" "${@:2}"; }
M SetCapabilities "uint32 41"                        # PREEDIT|FOCUS|SURROUNDING
M FocusInId "/org/freedesktop/IBus/InputContext_9" "gtk4-im:chrome"
M ProcessKeyEvent "uint32 116" "uint32 0" "uint32 0" # 't'
M SetSurroundingText "<('IBusText', {}, 'hello', <('IBusAttrList', {}, [])>)>" "uint32 5" "uint32 5"
```

Watch signals with `dbus-monitor --address "$ADDR"`. **gdbus string trap:** string arguments must
be `'like-this'` — writing `"string macro"` sends the literal `string macro`; numbers take
`"uint32 0"`. Always confirm what landed by reading it back in dbus-monitor.

Debug intermittent typing bugs: enable `debug_log = true` (GUI or config), reproduce, read
`~/.cache/ibus-mangrs/debug.log` (ms-since-epoch, cross-referenceable with `journalctl`). The log
records engine decisions, never the typed text.

Kill the engine with `pkill -x ibus-engine-ibu` (comm truncates at 15 chars; `-f` also matches
your own shell). SIGTERM/SIGINT exit cleanly.

## Known quirks

- **Defunct engine children in `ps`.** The engine spawns the GUI (`open_settings`) as a child and
  never reaps it, so exited GUI windows linger as `[ibus-engine-ibu] <defunct>` zombies. Cosmetic;
  do not confuse them with a crashed engine. Fix = reap `SIGCHLD`, only if it bothers someone.
- **NoReply from non-daemon peers (unresolved).** A `dbus-send` from a fresh connection to the
  engine name on the live bus got no reply at all — not even libdbus's automatic UnknownMethod —
  for both a private method and `Introspect`, while the engine polled normally and key events
  flowed. Observed against a stale engine process; retest after a clean engine restart before
  relying on (or ruling out) third-party → engine method calls.
- **Stale registry files.** `~/.config/ibus/bus/` accumulates files from past daemons; the one
  matching the running daemon (`IBUS_DAEMON_PID` inside) is used as the bus address.
