# 06 — Xử lý sự cố

Bảng chẩn đoán nhanh. Bật **Debug Logging** trước khi tái hiện (GUI General tab hoặc
`debug_log = true`) — log tại `~/.cache/ibus-mangrs/debug.log`, mốc thời gian ms khớp
`journalctl`, không ghi nội dung bạn gõ.

## Không thấy MăngRS trong danh sách input sources

| | |
|---|---|
| Nguyên nhân thường gặp | Component XML chưa ở đúng chỗ; registry cache của daemon cũ; binary chết ngay khi chạy |
| Cách kiểm tra | `ls /usr/share/ibus/component/ibus-mangrs.xml`; `ibus write-cache --system` rồi `ibus-daemon -drx` |
| Khắc phục | cài lại (`sudo ./scripts/install.sh`), `ibus restart`. Chạy dev: `IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh` — không có `-t refresh` thì daemon dùng cache cũ |

## Engine kẹt trên bus name (`InQueue`)

| | |
|---|---|
| Nguyên nhân | tiến trình cũ chết giữ tên bus |
| Cách kiểm tra | `pgrep -a ibus-engine-ibus` |
| Khắc phục | `pkill -x ibus-engine-ibu` (comm bị cắt 15 ký tự; `-f` sẽ dính cả shell của bạn), rồi `ibus restart` |

## Gõ không ra dấu / ra chữ thường ở một app

| | |
|---|---|
| Nguyên nhân thường gặp | client không có ST và `/dev/uinput` không ghi được → engine giữ raw thay vì sửa (đây là an toàn, không phải crash) |
| Cách kiểm tra | log có dòng `raw-forward (no delete transport)`; `ls -l /dev/uinput` phải là `crw-rw---- root input` và bạn thuộc group `input` (`id -nG`) |
| Khắc phục | cài udev rule (`data/80-uinput.rules`), `sudo usermod -aG input $USER`, đăng nhập lại |

## Chữ bị nhân đôi (`tungùng`)

| | |
|---|---|
| Nguyên nhân thường gặp | app nuốt `DeleteSurroundingText` ở chế độ NonPreedit; từ phiên mới engine tự phát hiện và chuyển app sang Preedit cho các từ sau |
| Cách kiểm tra | log: `rewrite delete swallowed (report proof)` |
| Khắc phục | thêm App Rule Preedit cho app đó (GUI Applications) hoặc chuyển chế độ toàn cục sang Preedit |

## Dấu nháy lên rồi biến mất

| | |
|---|---|
| Nguyên nhân thường gặp | commit chạy trước phép xóa inject (transport khác nhau) — đã có cơ chế defer-cho-echo và flush 150ms |
| Cách kiểm tra | log: `defer commit`, `flush deferred`, `injection echoes stale` |
| Khắc phục | nếu còn gặp: gửi kèm debug.log; thử chế độ Preedit cho app đó |

## Con trỏ di chuyển mà preedit treo cũ / từ rơi sai chỗ

| | |
|---|---|
| Nguyên nhân thường gặp | click/arrow khi preedit đang treo — cơ chế #14 (idle commit 800ms + Reset deferral) xử lý nhưng có giới hạn protocol |
| Cách kiểm tra | log: `reset from client`, `st report cursor=...` |
| Khắc phục | gõ nốt từ (space) trước khi click; hoặc bật `preedit_commit_timeout_ms` dài hơn/ngắn hơn; hoặc đổi `click_action` sang `keep`/`relocate` |

## Gõ tiếng Việt không ăn ở JetBrains/ứng dụng X11

| | |
|---|---|
| Nguyên nhân thường gặp | input source đang là bàn phím Anh thuần (xkb us) — IBus không nhận phím |
| Cách kiểm tra | `gsettings get org.gnome.desktop.input-sources current`; log trống key events |
| Khắc phục | `Super+Space` chuyển về MăngRS; xoá/bớt nguồn xkb us để không bị chuyển lặng |

## Cửa sổ cài đặt không mở

| | |
|---|---|
| Nguyên nhân thường gặp | binary build không có feature gui |
| Cách kiểm tra | `ibus-engine-ibus-mangrs -gui` — báo "this build has no GUI" nếu thiếu |
| Khắc phục | `cargo build --release --features gui` rồi cài lại |

## Đọc tiếp

- [05-tuong-thich-ung-dung.md](05-tuong-thich-ung-dung.md) — hành vi riêng từng app.
- [07-build-tu-source.md](07-build-tu-source.md) — build binary mới khi sửa lỗi.
