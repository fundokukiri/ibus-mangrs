# Kiến trúc và luồng xử lý dữ liệu — ibus-mangrs

## Tổng quan

`ibus-mangrs` là một engine IBus (Input Method Engine) cho tiếng Việt, viết bằng Rust, chạy trên `bamboo-core`. Engine hỗ trợ 3 chế độ hiển thị chính:

| Chế độ | Mô tả |
|--------|-------|
| **Preedit** (mặc định) | Hiển thị composition có gạch chân dưới ứng dụng; commit khi gặp terminator |
| **NonPreedit** | Không gạch chân; commit từng phím và viết lại (rewrite) đuôi văn bản bằng `DeleteSurroundingText` |
| **Surrounding** | NonPreedit + kiểm tra `SetSurroundingText` để sửa chữa drift (delete bị bỏ, edit thủ công) |

---

## Cấu trúc thư mục (`src/`)

```
src/
├── lib.rs              # Module exports, layering docs
├── main.rs             # Entry point: engine daemon hoặc -gui
├── server.rs           # D-Bus server: connection, IBus calls, signals
├── engine.rs           # State machine: key → Action (protocol-free)
├── bamboo_wrapper.rs   # Chỉ module chạm vào bamboo-core
├── macros.rs           # Macro table (macros.toml) + dictionary (dictionary.txt)
├── config.rs           # TOML config tại ~/.config/ibus-mangrs/config.toml
├── ibus_prop.rs        # IBusProperty/IBusPropList encoding cho menu input-source
├── ibus_text.rs        # GVariant encode/decode IBusText/IBusAttribute
├── keymap.rs           # X11 keysym → char, modifier masks
├── uinput.rs           # Virtual keyboard (/dev/uinput) cho Backspace injection
├── debug_log.rs        # Debug log tới ~/.cache/ibus-mangrs/debug.log
├── golden.rs           # 480-case golden tests (keystroke → text)
└── gui.rs              # GTK4 settings window (feature "gui")
```

---

## Layering (từ ngoài vào trong)

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
│  ├─ Word-mode lock (không đổi mode giữa từ)                 │
│  ├─ on_edit: NonPreedit/Surrounding/Preedit paths           │
│  ├─ on_finish: macro, auto-restore, commit                  │
│  ├─ verify_surrounding: drift detection & repair            │
│  ├─ Backspace handling (protocol vs uinput injection)       │
│  ├─ Deferred commits (đợi Backspace echoes)                 │
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

## Luồng dữ liệu chính (Data Flow)

### 1. Khởi động engine

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

### 2. Xử lý một phím (ProcessKeyEvent)

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
    │  ├─ verify_surrounding()     # drift check trước khi thay đổi
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
    │  │   ├─ protocol: DeleteSurroundingText (iu)  ── khi surrounding_data_ok()
    │  │   └─ injection: Uinput::inject_backspaces() ── khi !surrounding hoặc delete_dropped
    │  ├─ CommitText(v IBusText)
    │  ├─ UpdatePreeditText(v u b u)
    │  └─ HidePreedit
    ▼
out: Vec<Message> (signals) → send(conn, msg) → IBus daemon → app
```

### 3. Luồng NonPreedit

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
         ├─ DeleteBefore(5)     # xóa "tieen"
         └─ CommitText("ếng")   # ghi "ếng"
         tail = "tiếng"
────────────────────────────────────────────────────────
Kết quả: "tiếng" (không có underline bao giờ)
```

### 4. Luồng Preedit

```text
User types: t i e e n g s [space]
────────────────────────────────────────────────────────
Mỗi key: UpdatePreeditText("tiếng", cursor=5, visible=true)
Space (terminator): Finish{word="tiếng", raw="tieengs", valid=true}
    → on_finish → CommitText("tiếng") + Forward(space)
────────────────────────────────────────────────────────
```

### 5. Luồng Surrounding (verify & repair)

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

## Các cơ chế quan trọng (Key Mechanisms)

### Word-mode lock
- Mode được resolve **lần đầu** khi bắt đầu từ (`key_mode()`)
- Khóa mode đó cho toàn bộ từ (`word_mode: Option<Mode>`)
- Giải phóng khi từ kết thúc (`Finish`) hoặc `reset()`
- Tránh flip preedit↔non-preedit giữa từ gây duplicate/invisible text

### Deferred commits (commit trì hoãn)
- Khi dùng **uinput injection**: Backspace keys đi qua compositor, CommitText đi D-Bus
- Nếu commit đồng bộ → outruns Backspace → "accent flashes and reverts"
- Giải pháp: `defer_commit(n_echoes, text)` → queue, flush khi `note_echo_forwarded()` đạt threshold
- Focus change/real key → `flush_deferred()` (force deliver)

### Delete transport selection
```rust
fn deletes_allowed(&self) -> bool {
    self.surrounding_data_ok()           // client đã report ST
        || (self.uinput_available        // có /dev/uinput
            && !self.injection_forbidden()) // không phải relay (gnome-shell/fake)
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
`verify_surrounding` phân loại report từ client:

1. **Healthy**: `actual` ends with `tail` → OK
2. **Repair**: extra chars before common suffix → `DeleteSurrounding` / `RewriteByBackspace`
3. **Rebuild**: `tail` là substring (Chrome suggestions) → `sync_from_surrounding()` (rebuild wrapper)
4. **Stale/Ignore**: `tail` không có trong `actual` → ignore silently (trước đây tear down composition)

### Macro & Auto-restore
- Macro: `macros.toml` (key → expansion), `$TIME`/`$DATE`, capitalize-first/all
- Auto-restore: từ không phải tiếng Việt (invalid syllable) → restore raw keys
- Exception: từ có `undo_in_word` (user bỏ dấu giữa từ) → KHÔNG restore

---

## IBus Protocol Facts (đã verify trên wire)

| Fact | Detail |
|------|--------|
| Bus | Private bus của ibus-daemon, KHÔNG phải session bus. Connect bằng `open_private()` + `register()` |
| CreateEngine | Return type **`o` (object path)**, không phải `s` |
| Method reply | Phải trả lời **từ message filter** (`replace_message_callback`), return `true` để claim |
| FocusId/ActiveSurroundingText | Trên wire là **boolean** (`g_variant_get_boolean`). Sai type → daemon warn + fallback |
| FocusId = true | Làm daemon gửi `FocusInId(path, client)` thay vì `FocusIn` — chỉ cách biết app nào đang gõ |
| DeleteSurroundingText | Signature **`(iu)`**: offset (signed) + count (uint), **không wrap variant** |
| IBusText wire form | `('IBusText', @a{sv} {}, 'str', <('IBusAttrList', @a{sv} {}, [attrs])>)` |
| PropertyActivate | GNOME Shell gửi **1 call/item** của radio group: picked=CHECKED, siblings=UNCHECKED. Phải ignore UNCHECKED |
| RegisterProperties/UpdateProperty | Là **signals** emit từ engine object, daemon relay tới panel |

---

## Cấu hình (Config)

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
cycle_mode_hotkey = ""  # ví dụ: "Control+Alt+v"
hide_underline = false
debug_log = false
app_rules = [
  { pattern = "chrome", mode = "preedit" },
  { pattern = "sublime_text", mode = "surrounding" },
  ...
]
```

**Hot-reload**: server check mtime config tối đa 1 lần/giây trên key events.

---

## App Rules (Per-application mode override)

- Match: substring case-insensitive trên `prgname` từ `FocusInId` (`gtk4-im:chrome` → `chrome`)
- Built-in defaults: Chrome/Electron → Preedit, Sublime → Surrounding
- GUI cho phép quick-add từ `~/.cache/ibus-mangrs/clients` (server ghi 50 client gần nhất)

---

## uinput Virtual Keyboard

- Device: `/dev/uinput` (yêu cầu udev rule, group `input`)
- Chỉ inject **Backspace** (keycode 14)
- Legacy path: write `struct uinput_user_dev` → `UI_DEV_CREATE` (kernel này từ chối `UI_DEV_SETUP`)
- Timing: 10ms interval, 10ms post-delay (giống ibus-bamboo)
- Echoes quay lại qua `ProcessKeyEvent` → `pending_injected` counter → forward untouched

---

## Testing

```bash
cargo test                    # 87 unit tests (bao gồm golden.rs 480 cases)
cargo clippy --all-targets
cargo build --features gui    # GTK4 settings window
```

**Live test (no keyboard injector)**:
```bash
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs

# Gọi D-Bus trực tiếp:
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
- [ ] Per-application regex rules (chỉ substring prgname)
- [ ] Mode Menu Hotkey (engine không thể mở panel popup)
- [ ] Spell check dictionary riêng (hiện dùng `auto_correct` của bamboo-core)
- [ ] Surrounding mode không sửa được drift trên client không gửi ST và không render preedit

---

## Debug & Logging

```bash
# Bật debug log
# config.toml: debug_log = true
# Log file: ~/.cache/ibus-mangrs/debug.log

# dbus-monitor để xem signals
dbus-monitor --address "$ADDR"
# Xem: UpdatePreeditText, CommitText, DeleteSurroundingText, RegisterProperties
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

## Tóm tắt luồng sự kiện chính

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