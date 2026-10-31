# 07 — Thư viện bên thứ ba

> Chương này trả lời: repo phụ thuộc gì, dùng để làm gì, ở đâu, và vì sao. Phiên bản
> lấy từ `Cargo.lock`, license lấy từ `Cargo.toml` của từng crate trong cargo registry
> (đã đối chiếu ngày viết).

## Bảng tổng hợp

| Crate | Phiên bản (Cargo.lock) | License | Dùng ở | Vì sao chọn |
|---|---|---|---|---|
| `bamboo-core` | 0.5.3 | MIT | `src/bamboo_wrapper.rs` | Engine tiếng Việt thuần (Rust port logic của ibus-bamboo); cùng tác giả hệ sinh thái, đúng một mối quan tâm của wrapper |
| `dbus` (dbus-rs, crate `dbus` feature `ffidisp`) | 0.9.12 | Apache-2.0 OR MIT | `src/server.rs` | Binding D-Bus đồng bộ cấp thấp đủ để trả method từ message filter và gửi signal thủ công — dự án cần điều khiển chính xác thời điểm send |
| `serde` + `serde_derive` | 1.0.229 / 1.0.154 | MIT OR Apache-2.0 | `src/config.rs` | De/serialize `Config` TOML với `#[serde(default)]` — config thiếu key không chết |
| `toml` | 0.8.23 | MIT OR Apache-2.0 | `src/config.rs` (`merge`, `Macros::load`) | Parse/ghi config.toml + macros.toml, giữ key lạ khi ghi đè |
| `serde_json` | 1.0.154 | MIT OR Apache-2.0 | `src/backup.rs` | Backup/restore settings một file JSON |
| `chrono` | 0.4.45 (feature `clock`, không default) | MIT OR Apache-2.0 | `src/macros.rs` | `$TIME`/`$DATE` strftime |
| `dirs` | 6.0.0 | MIT OR Apache-2.0 | config/cache/data paths (`Config::path`, `debug_log.rs`, `paste.rs`...) | Chuẩn XDG, không tự dựng path |
| `libc` | 0.2.191 | MIT OR Apache-2.0 | `src/uinput.rs` | ioctl/write raw cho `/dev/uinput` (`UI_DEV_CREATE`, legacy `uinput_user_dev`) |
| `signal-hook` | 0.3.18 | Apache-2.0 OR MIT | `src/main.rs::install_signal_handlers` | SIGTERM/SIGINT exit sạch để ibus-daemon nhả bus name (tránh kẹt `InQueue`) |
| `gtk4` (optional, feature `gui`) | 0.9.7 (features v4_6, v4_8) | MIT | `src/gui.rs`, `src/backup.rs` | Settings window; v4_6 cần `Texture::from_bytes` (logo About), v4_8 cần `CheckButton::set_child` (comment trong `Cargo.toml`) |

## Ghi chú lựa chọn

- **Không async runtime.** Toàn bộ là đồng bộ: một message filter + một vòng lặp
  `conn.iter(timeout)` với deadline từ `Engine::ms_until_tick`. Phím phải có thứ tự;
  async chỉ thêm tail-latency vàrace — không có lợi. (Quyết định thiết kế — chương 11.)
- **Không logger crate.** `debug_log.rs` tự viết (~70 dòng): append một file, mở một
  lần, tắt/bật runtime — tránh phụ thuộc và giữ chi phí gần bằng 0 khi tắt.
- **dbus-rs phiên bản `ffidisp`**: API cũ nhưng là cách duy nhất trong crate này để trả
  lời method *ngay trong filter* (yêu cầu của IBus protocol — trả muộn qua MsgHandler sẽ
  bị libdbus đáp UnknownMethod trước; `ARCHITECTURE.md` "Method replies").
- **bamboo-core qua crates.io**, submodule chỉ để đọc — tách bạch build và audit
  (comment trong `Cargo.toml`).

## Phụ thuộc hệ thống (không phải crate)

- `ibus-daemon` ≥ 1.5 (đã kiểm trên 1.5.34, `ARCHITECTURE.md`), `gtk4` runtime cho GUI.
- `/dev/uinput` + udev rule `data/80-uinput.rules` + group `input` — cho transport
  Backspace thật; kernel cụ thể từ chối ioctl `UI_DEV_SETUP` nên code dùng đường legacy
  (`src/uinput.rs::open`, comment trong file).
- Wayland: `wl-copy`/`wl-paste`; X11: `xclip` — chỉ khi dùng clipboard channel
  (`src/paste.rs::session_tool`).

## Đọc tiếp

- [06-ma-nguon.md](06-ma-nguon.md) — vị trí từng crate trong cây.
- [08-du-lieu-va-cau-hinh.md](08-du-lieu-va-cau-hinh.md) — dữ liệu mà các thư viện serde/toml thao tác.
