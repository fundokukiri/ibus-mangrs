# Architecture and Data Flow — ibus-mangrs

## Overview

`ibus-mangrs` is a Vietnamese Input Method Engine (IME) for IBus, written in Rust on top of `bamboo-core`. The engine supports three display modes:

| Mode | Description |
|------|-------------|
| **Preedit** (default) | Shows inline composition with underline; commits when terminator is encountered |
| **NonPreedit** | No underline; commits each keystroke and rewrites the tail via `DeleteSurroundingText` |
| **Surrounding** | NonPreedit + verifies `SetSurroundingText` to repair drift (dropped deletes, manual edits) |

---

## Directory Structure (`src/`)

```
src/
├── lib.rs              # Module exports, layering docs
├── main.rs             # Entry point: engine daemon or -gui
├── server.rs           # D-Bus server: connection, IBus calls, signals
├── engine.rs           # State machine: key → Action (protocol-free)
├── bamboo_wrapper.rs   # Only module touching bamboo-core
├── macros.rs           # Macro table (macros.toml) + dictionary (dictionary.txt)
├── config.rs           # TOML config at ~/.config/ibus-mangrs/config.toml
├── ibus_prop.rs        # IBusProperty/IBusPropList encoding for input-source menu
├── ibus_text.rs        # GVariant encode/decode IBusText/IBusAttribute
├── keymap.rs           # X11 keysym → char, modifier masks
├── uinput.rs           # Virtual keyboard (/dev/uinput) for Backspace injection
├── debug_log.rs        # Debug log to ~/.cache/ibus-mangrs/debug.log
├── golden.rs           # 480-case golden tests (keystroke → text)
└── gui.rs              # GTK4 settings window (feature "gui")
```

---

## Layering (outside-in)

```
┌─────────────────────────────────────────────────────────────┐
│  server.rs                  ← D-Bus, IBus protocol          │
│  ├─ main.rs: Server::connect() → Server::run()              │
│  ├─ EngineHandler: message filter (reply from filter)       │
│  ├─ ProcessKeyEvent → engine.process_char()                 │
│  ├─ PropertyActivate → config mutation + UpdateProperty     │
│  ├─ FocusIn/Enable → clear state, register properties       │
│  ├─ SetSurroundingText → engine.set_surrounding()           │
│  └─ SetCapabilities → engine.set_capabilities()             │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  engine.rs                  ← State machine, no D-Bus types │
│  ├─ Mode resolution (AppRule, heuristic, caps)              │
│  ├─ Word-mode lock (mode doesn't change mid-word)           │
│  ├─ on_edit: NonPreedit/Surrounding/Preedit paths           │
│  ├─ on_finish: macro, auto-restore, commit                  │
│  ├─ verify_surrounding: drift detection & repair            │
│  ├─ Backspace handling (protocol vs uinput injection)       │
│  ├─ Deferred commits (wait for Backspace echoes)            │
│  └─ Action emission → server.emit()                         │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  bamboo_wrapper.rs          ← Only bamboo-core touch point  │
│  ├─ process_char → Outcome::Edit/Finish/PassThrough         │
│  ├─ backspace() → Edit (delta from last_output)             │
│  ├─ rebuild(text) → sync wrapper from surrounding text      │
│  ├─ delta tracking against last_output (not bamboo's prev)  │
│  ├─ undo_in_word detection (mark removed mid-word)          │
│  └─ Output override (poisoned state workaround)             │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
┌─────────────────────────────────────────────────────────────┐
│  bamboo-core (external crate)                               │
│  ├─ Telex/VNI/VIQR/Microsoft layout                         │
│  ├─ 11 charsets (Unicode, NFD, TCVN3, VNI, VIQR, ...)       │
│  ├─ auto_correct, free_tone, w2u, bracket modes             │
│  └─ is_valid_word()                                         │
└─────────────────────────────────────────────────────────────┘
```

---

## Main Data Flows

### 1. Engine Startup

```text
main()
  └─ Config::load()
  └─ Engine::new(method, mode)
  └─ Server::connect(SharedEngine)
       └─ Connection::open_private(IBUS_ADDRESS)
       └─ conn.register()           # Hello handshake
       └─ conn.register_name(BUS_NAME)
       └─ conn.replace_message_callback(EngineHandler)
  └─ server.run()  (main loop: conn.iter(1000))
```

### 2. Processing a Key (ProcessKeyEvent)

```text
IBus daemon
    │
    ▼
server.rs: EngineHandler::handle()  (message filter)
    │  ├─ maybe_reload_config()    # hot-reload config.toml mtime
    │  ├─ try_cycle_hotkey()       # Control+Alt+v → cycle mode
    │  └─ process_key()
    ▼
engine.rs: Engine::process_char(c)
    │  ├─ key_mode()               # resolve + lock word_mode
    │  ├─ verify_surrounding()     # drift check before any change
    │  ├─ auto-capitalize
    │  └─ wrapper.process_char(c)  → Outcome
    ▼
Outcome::
  ├─ PassThrough        → Action::Forward
  ├─ Finish {word,raw}  → on_finish() → macro/auto-restore/commit
  └─ Edit {del,ins}     → on_edit() → DeleteBefore/Commit/UpdatePreedit
    ▼
server.rs: emit(engine, actions, out)
    │  ├─ DeleteBefore/DeleteSurrounding
    │  │   ├─ protocol: DeleteSurroundingText (iu)  ── when surrounding_data_ok()
    │  │   └─ injection: Uinput::inject_backspaces() ── when !surrounding or delete_dropped
    │  ├─ CommitText(v IBusText)
    │  ├─ UpdatePreeditText(v u b u)
    │  └─ HidePreedit
    ▼
out: Vec<Message> (signals) → send(conn, msg) → IBus daemon → app
```

### 3. NonPreedit Flow

```text
User types: t i e e n g s
────────────────────────────────────────────────────────
Key 't': wrapper → Edit{0,"t"} → emit → CommitText("t")
         tail = "t"

Key 'i': wrapper → Edit{0,"i"} → emit → CommitText("i")
         tail = "ti"

Key 'e': wrapper → Edit{0,"e"} → emit → CommitText("e")
         tail = "tie"

Key 'e': wrapper → Edit{0,"e"} → emit → CommitText("e")
         tail = "tiee"

Key 'n': wrapper → Edit{0,"n"} → emit → CommitText("n")
         tail = "tieen"

Key 'g': wrapper → Edit{0,"g"} → emit → CommitText("g")
         tail = "tieeng"

Key 's' (tone): wrapper → Edit{5,"ếng"} → emit
         ├─ DeleteBefore(5)     # delete "tieen"
         └─ CommitText("ếng")   # write "ếng"
         tail = "tiếng"
────────────────────────────────────────────────────────
Result: "tiếng" (never any underline)
```

### 4. Preedit Flow

```text
User types: t i e e n g s [space]
────────────────────────────────────────────────────────
Each key: UpdatePreeditText("tiếng", cursor=5, visible=true)
Space (terminator): Finish{word="tiếng", raw="tieengs", valid=true}
    → on_finish → CommitText("tiếng") + Forward(space)
────────────────────────────────────────────────────────
```

### 5. Surrounding Flow (verify & repair)

```text
Client: SetSurroundingText("xin chào", cursor=8, anchor=8)
Engine.tail = "xin ch"
────────────────────────────────────────────────────────
verify_surrounding():
  actual = "xin chào".chars().take(8) = "xin chào"
  tail   = "xin ch"
  ends_with(actual, tail)?  YES → healthy, no repair

Client: SetSurroundingText("xin chào  ", cursor=10)  # app dropped delete
Engine.tail = "xin chào"
────────────────────────────────────────────────────────
verify_surrounding():
  actual = "xin chào  "
  tail   = "xin chào"
  ends_with? NO
  common_suffix = "xin chào" (k=8)
  extra = 2 chars ("  ") before suffix
  → DeleteSurrounding(offset=-10, n=2)  # protocol delete
  → repair_delete = Some((-10, 2))
  → delete_dropped = false (first attempt)
  
Client drops delete again → same drift
→ delete_dropped = true
→ RewriteByBackspace(backspaces=10, commit="xin chào")
   # 10 Backspace keys injected via uinput
```

---

## Key Mechanisms

### Word-mode lock
- Mode is resolved **once** when a word starts (`key_mode()`)
- Locks that mode for the entire word (`word_mode: Option<Mode>`)
- Released when word ends (`Finish`) or `reset()`
- Prevents preedit↔non-preedit flips mid-word causing duplicate/invisible text

### Deferred commits
- When using **uinput injection**: Backspace keys go through compositor, CommitText goes D-Bus
- If synchronous commit → outruns Backspace → "accent flashes and reverts"
- Solution: `defer_commit(n_echoes, text)` → queue, flush when `note_echo_forwarded()` reaches threshold
- Focus change/real key → `flush_deferred()` (force deliver)

### Delete transport selection
```rust
fn deletes_allowed(&self) -> bool {
    self.surrounding_data_ok()           // client has reported ST
        || (self.uinput_available        // has /dev/uinput
            && !self.injection_forbidden()) // not relay (gnome-shell/fake)
}

fn may_inject(&self) -> bool {
    !self.injection_forbidden() 
        && (self.delete_dropped || self.surrounding.is_none())
}
```

| Client type | surrounding_data_ok | injection_forbidden | Transport |
|-------------|---------------------|---------------------|-----------|
| GTK4/GTK3 (Wayland, relayed) | Yes | Yes | Protocol delete |
| XWayland/Sublime/JetBrains | No | No | uinput Backspace |
| Chrome/Electron (drop delete) | Yes | No | Protocol (first) → uinput (after drop detected) |
| GNOME Shell search (`fake`) | No | Yes | N/A (no typing) |

### Stale SetSurroundingText classification
`verify_surrounding` classifies client reports:

1. **Healthy**: `actual` ends with `tail` → OK
2. **Repair**: extra chars before common suffix → `DeleteSurrounding` / `RewriteByBackspace`
3. **Rebuild**: `tail` is substring (Chrome suggestions) → `sync_from_surrounding()` (rebuild wrapper)
4. **Stale/Ignore**: `tail` not in `actual` → ignore silently (used to tear down composition)

### Macro & Auto-restore
- Macro: `macros.toml` (key → expansion), `$TIME`/`$DATE`, capitalize-first/all
- Auto-restore: non-Vietnamese word (invalid syllable) → restore raw keys
- Exception: word with `undo_in_word` (user removed mark mid-word) → NO restore

---

## IBus Protocol Facts (verified on wire)

| Fact | Detail |
|------|--------|
| Bus | Private bus of ibus-daemon, NOT session bus. Connect with `open_private()` + `register()` |
| CreateEngine | Return type **`o` (object path)**, not `s` |
| Method reply | Must reply **from message filter** (`replace_message_callback`), return `true` to claim |
| FocusId/ActiveSurroundingText | On wire are **booleans** (`g_variant_get_boolean`). Wrong type → daemon warn + fallback |
| FocusId = true | Makes daemon send `FocusInId(path, client)` instead of `FocusIn` — only way to know which app is typing |
| DeleteSurroundingText | Signature **`(iu)`**: offset (signed) + count (uint), **no variant wrapper** |
| IBusText wire form | `('IBusText', @a{sv} {}, 'str', <('IBusAttrList', @a{sv} {}, [attrs])>)` |
| PropertyActivate | GNOME Shell sends **1 call/item** of radio group: picked=CHECKED, siblings=UNCHECKED. Must ignore UNCHECKED |
| RegisterProperties/UpdateProperty | Are **signals** emitted from engine object, daemon relays to panel |

---

## Configuration (Config)

**File**: `$XDG_CONFIG_HOME/ibus-mangrs/config.toml`

```toml
enabled = true
method = "telex"        # telex | vni | viqr | microsoft
mode = "preedit"        # nonpreedit | preedit | surrounding (default: preedit)
charset = "unicode"     # 11 charsets
modern_tone_style = true
free_tone_marking = true
auto_correct = true
auto_restore = true
w2u = "disabled"        # disabled | nonstart | everywhere
bracket = "disabled"    # disabled | nonstart | everywhere
em_dash = false
auto_capitalize = false
double_space_period = false
macro_enabled = true
capitalize_macro = true
macro_in_off_mode = false
time_format = "%H:%M"
date_format = "%d/%m/%Y"
dictionary_enabled = false
cycle_mode_hotkey = ""  # e.g. "Control+Alt+v"
hide_underline = false
debug_log = false
app_rules = [
  { pattern = "chrome", mode = "preedit" },
  { pattern = "sublime_text", mode = "surrounding" },
  ...
]
```

**Hot-reload**: server checks config mtime max once per second on key events.

---

## App Rules (Per-application mode override)

- Match: case-insensitive substring on `prgname` from `FocusInId` (`gtk4-im:chrome` → `chrome`)
- Built-in defaults: Chrome/Electron → Preedit, Sublime → Surrounding
- GUI allows quick-add from `~/.cache/ibus-mangrs/clients` (server records 50 most recent clients)

---

## uinput Virtual Keyboard

- Device: `/dev/uinput` (requires udev rule, group `input`)
- Injects only **Backspace** (keycode 14)
- Legacy path: write `struct uinput_user_dev` → `UI_DEV_CREATE` (this kernel rejects `UI_DEV_SETUP`)
- Timing: 10ms interval, 10ms post-delay (same as ibus-bamboo)
- Echoes return via `ProcessKeyEvent` → `pending_injected` counter → forwarded untouched

---

## Testing

```bash
cargo test                    # 133 unit tests (including golden.rs 480 cases)
cargo clippy --all-targets
cargo build --features gui    # GTK4 settings window
```

**Live test (no keyboard injector)**:
```bash
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs

# Direct D-Bus calls:
ADDR=$(grep -o 'unix:path=[^,]*' ~/.config/ibus/bus/*)
M() { gdbus call --address "$ADDR" --dest org.freedesktop.IBus.mangrs \
      --object-path /org/freedesktop/IBus/Engine/1 \
      --method "org.freedesktop.IBus.Engine.$1" "${@:2}"; }
M SetCapabilities "uint32 41"                       # PREEDIT|FOCUS|SURROUNDING
M FocusInId "/org/freedesktop/IBus/InputContext_9" "gtk4-im:chrome"
M ProcessKeyEvent "uint32 116" "uint32 0" "uint32 0"  # 't'
```

---

## Known Gaps / TODO

- [ ] Uinput modes, Emoji Picker, Minecraft mode
- [ ] Candidate window / lookup table
- [ ] Per-application regex rules (substring prgname only for now)
- [ ] Mode Menu Hotkey (engine cannot open panel popup)
- [ ] Separate spell-check dictionary (currently uses bamboo-core's `auto_correct`)
- [ ] Surrounding mode cannot repair drift on clients that neither send ST nor render preedit

---

## Debug & Logging

```bash
# Enable debug log
# config.toml: debug_log = true
# Log file: ~/.cache/ibus-mangrs/debug.log

# dbus-monitor to watch signals
dbus-monitor --address "$ADDR"
# Watch: UpdatePreeditText, CommitText, DeleteSurroundingText, RegisterProperties
```

---

## Build & Run

```bash
# Build
cargo build --release

# Dev run (no install)
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs

# Install (root)
sudo ./scripts/install.sh

# GUI settings
cargo build --features gui
./target/debug/ibus-engine-ibus-mangrs -gui
```

---

## Event Flow Summary

```
┌──────────────┐     ┌─────────────┐     ┌──────────────┐     ┌─────────────┐
│  IBus daemon │────▶│  server.rs  │────▶│  engine.rs   │────▶│bamboo_wrap. │
│  (private    │     │  (D-Bus,    │     │  (state      │     │  (bamboo-   │
│   bus)       │     │   signals)  │     │   machine)   │     │   core)     │
└──────────────┘     └─────────────┘     └──────────────┘     └─────────────┘
                           │                   │                   │
                           │ ProcessKeyEvent   │ process_char    │ process_char
                           │ PropertyActivate  │ verify_surround │ backspace
                           │ FocusIn/Out       │ on_edit/finish  │ rebuild
                           │ SetSurrounding    │ reset           │
                           │ SetCapabilities   │                 │
                           ▼                   ▼                   ▼
                    ┌─────────────┐     ┌──────────────┐     ┌─────────────┐
                    │  Signals    │◀────│  Actions     │◀────│  Outcome    │
                    │  CommitText │     │  Delete/     │     │  Edit/      │
                    │  UpdatePre..│     │  Commit/     │     │  Finish/    │
                    │  DeleteSur..│     │  Rewrite/    │     │  PassThrough│
                    │  Register.. │     │  HidePreedit │     │             │
                    └─────────────┘     └──────────────┘     └─────────────┘
                           │
                           ▼
                    ┌─────────────┐
                    │ Application │
                    │ (GTK/Qt/    │
                    │  XWayland)  │
                    └─────────────┘
```

---

*Generated from codebase analysis. For implementation details, see individual `src/*.rs` files.*