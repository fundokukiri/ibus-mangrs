# 06 — Bản đồ mã nguồn

> Chương này là bản đồ: cây thư mục có chú thích, quan hệ crate, và thứ tự đọc đề xuất
> cho người mới mở repo.

## Cây thư mục repo

```
├── Cargo.toml              crate ibus-mangrs (lib + bin), features: gui, profile.release LTO
├── Cargo.lock              khoá phiên bản dependency
├── src/                    MÃ NGUỒN CHÍNH (chi tiết dưới)
├── bamboo-core/            submodule pin v0.5.3 — chỉ để đọc/audit, build dùng crates.io
├── data/
│   ├── ibus-mangrs.xml     component XML đăng ký engine với ibus-daemon (bắt buộc khớp bus name)
│   ├── 80-uinput.rules     udev rule cho /dev/uinput (0660, group input)
│   ├── vn-words.txt        38 350 từ cho spell check (nguồn GPL — data/README.md)
│   └── logo*.png           logo (embedded trong GUI qua gtk4 Texture::from_bytes)
├── scripts/                install.sh, uninstall.sh, autoinstall.sh, install-online.sh, dist.sh, release.sh
├── test/                   công cụ gõ thật qua kernel + expected files (test/README.md)
├── audit/                  báo cáo audit chế độ gõ (typing-modes-report.md)
├── docs/                   tài liệu này + các doc nội bộ
├── analysis/               vendor tham chiếu: ibus-bamboo (Go), ClakIME, vietc... (không build)
└── PKGBUILD                gói Arch (pkgname=ibus-ibus-mangrs)
```

## `src/` — vai trò từng file

```
src/
├── main.rs            entry: -gui → GTK window; else nạp config, tạo Engine, chạy Server
├── lib.rs             khai báo module + layering
├── server.rs          D-Bus: connection, message filter, signals, emit_with (transports),
│                      property menu, cycle hotkey, config mtime watch, main loop (tick)
├── engine.rs          state machine protocol-free: phím → Vec<Action>; modes, word lock,
│                      verify/repair ST, #14 click, macro/auto-restore hooks, ECHO_STALE
├── bamboo_wrapper.rs  NƠI DUY NHẤT chạm bamboo-core; Outcome, delta tự tính, backspace
│                      unikey (replay), poison guard, terminator theo method
├── config.rs          Config TOML serde + mặc định + merge giữ key lạ + migrate unikey cũ
├── macros.rs          Macros (macros.toml) + Dictionary (dictionary.txt)
├── spellcheck.rs      nạp vn-words.txt (lookup-only)
├── golden.rs          678 ca golden + FakeClient/fault matrix + fuzz có seed
├── keymap.rs          keysym → char, editing/navigation keys, KP
├── uinput.rs          /dev/uinput: backspace paced, Shift+Left, Ctrl+V, char_key table US-QWERTY
├── paste.rs           clipboard channel: wl-copy/xclip + Ctrl+V + restore debounce 600ms
├── ibus_text.rs       encode/decode IBusText/IBusAttribute trên dây
├── ibus_prop.rs       IBusProperty/PropList cho menu input-source
├── debug_log.rs       debug.log mở-một-lần, brief() truncate
├── backup.rs          backup/restore settings JSON (GUI)
└── gui.rs             GTK4 settings window (feature "gui") — 6 tab
```

## Quan hệ crate

```mermaid
flowchart TD
    BIN["bin: ibus-engine-ibus-mangrs\n(main.rs)"] --> LIB["lib: ibus_mangrs"]
    LIB --> SRV[server.rs] --> ENG[engine.rs] --> WRAP[bamboo_wrapper.rs] --> BC["bamboo-core 0.5.3"]
    LIB --> CFG[config.rs] --> BC
    LIB --> MAC[macros.rs] --> CHRONO[chrono]
    LIB --> SP[spellcheck.rs]
    LIB --> UI[uinput.rs] --> LIBC[libc]
    LIB --> PASTE[paste.rs]
    SRV --> DBUS[dbus 0.9] --> LIBC
    LIB --> GTK4["gtk4 0.9 (feature gui)"] -.-> BIN
    ENG --> GOLD[golden.rs (test)] --> ENG
```

Cùng một `Engine` instance được chia sẻ giữa message filter (mutation) và main loop
(tick) qua `Arc<Mutex<Engine>>` (`src/server.rs::SharedEngine`, `lock_engine` poison-tolerant).

## Thứ tự đọc đề xuất

1. **`src/golden.rs`** — nhìn hành vi trước: chuỗi phím → text cuối, không bị chi tiết
   làm loạn. Mọi ca là một "spec thực thi".
2. **`src/engine.rs`** — đầu đọc `Action` enum + `Mode` rồi `process_char` theo flow
   chương 02; sau đó `on_edit`/`on_finish`/`verify_surrounding`.
3. **`src/bamboo_wrapper.rs`** — toàn bộ, ngắn (~800 dòng), giải thích mọi quirk.
4. **`src/server.rs`** — chỉ khi cần hiểu D-Bus/transport: `handle`, `process_key`,
   `emit_with`, `run`.
5. Còn lại theo nhu cầu: `config.rs` (schema), `uinput.rs`/`paste.rs` (transports),
   `ibus_text.rs` (wire format).

Tài liệu kèm theo: `ARCHITECTURE.md` (protocol facts + invariants — nguồn tham chiếu
cho nhiều chương này), `docs/gap-analysis.md` (mọi tùy chọn × code × trạng thái),
`docs/bamboo-audit.md` (dùng/bỏ những gì của core).

## Đọc tiếp

- [09-kiem-thu.md](09-kiem-thu.md) — cách đọc và thêm test vào golden/matrix.
- [10-mo-rong.md](10-mo-rong.md) — điểm chạm khi thêm tính năng.
