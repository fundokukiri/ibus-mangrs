# MăngRS

Bộ gõ tiếng Việt cho IBus, viết bằng Rust trên [`bamboo-core`](https://crates.io/crates/bamboo-core).

---

## Yêu cầu

- Arch Linux (đã kiểm tra) hoặc distro có IBus ≥ 1.5
- `ibus`, `gtk4` (cho cửa sổ cài đặt)
- Rust ≥ 1.75 để build
- `/dev/uinput` đọc/ghi được để dùng đường xoá bằng phím thật (udev rule có sẵn trong lần
  cài: `/etc/udev/rules.d/80-uinput.rules` + nhóm `input`); thiếu nó engine tự quay về
  protocol delete cho client có surrounding text, còn client khác sẽ không gõ được dấu.

## Build

```sh
cargo build --release --features gui
```

## Cài đặt

Build + cài + restart IBus một bước:

```sh
./scripts/autoinstall.sh
```

Hoặc làm thủ công:

```sh
sudo ./scripts/install.sh
```

Script chép:

| File | Đích |
|---|---|
| `target/release/ibus-engine-ibus-mangrs` | `/usr/lib/ibus-mangrs/ibus-engine-ibus-mangrs` |
| `data/ibus-mangrs.xml` | `/usr/share/ibus/component/ibus-mangrs.xml` |

Rồi:

1. `ibus restart`
2. **Settings → Keyboard → Input Sources → +** → *Vietnamese* → **MăngRS**
   (hoặc `ibus engine ibus-mangrs`)
3. `Super+Space` để chuyển bộ gõ

Gỡ: `sudo ./scripts/uninstall.sh` (giữ cấu hình trong `~/.config/ibus-mangrs/`).

## Sử dụng

| Việc | Cách làm |
|---|---|
| Bật/tắt tiếng Việt | Menu input source → *Bật tiếng Việt*, hoặc `Super+Space` |
| Mở cửa sổ cài đặt | Menu input source → *Phím tắt…*, hoặc `ibus-engine-ibus-mangrs -gui` |
| Đổi kiểu gõ | Menu → *Kiểu gõ* (Telex / VNI / VIQR / Microsoft layout), hoặc GUI |
| Đổi bảng mã | Menu → *Bảng mã* (11 bảng mã), hoặc GUI |
| Đổi chế độ hiển thị | Menu → *Hiển thị* (Không gạch chân / Gạch chân / Surrounding text), hoặc GUI, hoặc hotkey |
| Phím tắt đổi chế độ | GUI → *Phím tắt đổi chế độ* (vd `Control+Alt+v`) |
| Cấu hình nhanh | Menu → *Cấu hình nhanh* (gõ tắt, viết hoa, bỏ dấu tự do, dấu thanh chuẩn, ẩn gạch chân, …) |
| Chính tả & từ điển | Menu → *Chính tả & từ điển* |
| Gõ tiếng Việt | Telex: `tieengs` → `tiếng`; VNI: `tie6ng1` → `tiếng` |
| Gõ tiếng Anh | Tắt bộ gõ, hoặc gõ bình thường — từ không hợp lệ được trả về phím gốc |

Cấu hình lưu ở `~/.config/ibus-mangrs/config.toml` — engine tự nạp lại trong ~1 giây, không cần
`ibus restart`. Chi tiết tuỳ chọn: `docs/settings.md`; chế độ hiển thị & App Rules:
`docs/modes.md`; xử lý sự cố: `docs/troubleshooting.md`.

### App Rules mặc định

| Ứng dụng | Chế độ | Vì sao |
|---|---|---|
| Chrome/Chromium/Brave/Edge/Vivaldi, Electron apps | preedit | nhóm Chromium nuốt `DeleteSurroundingText`, preedit không cần xoá |
| Sublime Text | surrounding | preedit bị ẩn; delete qua phím thật khi bị nuốt |
| kgx / GNOME Console | preedit | nuốt protocol delete |
| Khác (Firefox, gedit, …) | preedit (mặc định) | |

Chỉnh trong GUI tab *Ứng dụng* — có dropdown app đã gặp.

## Gỡ lỗi

Bật **"Ghi log gỡ lỗi"** trong GUI (tab *Chung*, hay `debug_log = true` trong
`~/.config/ibus-mangrs/config.toml`) rồi gõ lại câu lỗi. Engine ghi mỗi focus change, capability,
SetSurroundingText, phím và quyết định delete/inject vào `~/.cache/ibus-mangrs/debug.log` (ms từ
epoch — đối chiếu được với `journalctl`). Gửi file log kèm báo cáo lỗi là cách nhanh nhất để
tái hiện các bug kiểu "thỉnh thoảng mới bị".

## Kiểm thử

```sh
cargo test                      # 133 unit test, gồm 480 golden ca keystroke→text
cargo clippy --all-targets
```

Kiểm trên daemon thật không cần cài:

```sh
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

Công thức lái engine qua `gdbus` (cú pháp: string phải `'kiểu-này'`) và đọc tín hiệu:
`CLAUDE.md` — "Testing without a keyboard injector".

## Kiến trúc

- `src/server.rs` — D-Bus, message filter, property menu, hotkey, config watch, injection transport
- `src/engine.rs` — state machine: 3 chế độ hiển thị, App Rules, macro, restore, verify/repair
- `src/bamboo_wrapper.rs` — nơi duy nhất chạm `bamboo-core`; tự tính delta (fix Backspace)
- `src/uinput.rs` — bàn phím ảo 1 phím (Backspace) cho client nuốt protocol delete
- `src/macros.rs` — macro table + từ điển
- `src/golden.rs` — 480 ca keystroke → text
- `src/ibus_prop.rs` — mã hoá menu thuộc tính IBus
- `src/config.rs` — TOML chung engine + GUI
- `src/gui.rs` — GTK4 6 tab (feature `gui`)
- `CLAUDE.md` — kiến trúc chi tiết + "IBus protocol facts" (không public)

## Giấy phép

MIT. Phụ thuộc `bamboo-core` (MIT) — xem `THIRD_PARTY_LICENSES.md`.
