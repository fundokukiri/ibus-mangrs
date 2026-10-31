# 05 — Tương thích ứng dụng

Mỗi ứng dụng/toolkit xử lý xóa text và preedit khác nhau — đó là nguồn gốc của mọi khác
biệt. Engine tự chọn chế độ cho app qua heuristic + App Rules
(`src/config.rs::default_app_rules`, `src/engine.rs::effective_mode`). Bảng mặc định:

| Ứng dụng | Chế độ mặc định | Lý do |
|---|---|---|
| Chrome, Chromium, Brave, Edge, Vivaldi, Electron/VS Code, Slack, Discord | Preedit | nuốt protocol delete → NonPreedit sẽ nhân đôi |
| Sublime Text | Surrounding | preedit không hiển thị; repair bằng phím thật khi bị nuốt |
| kgx, GNOME Console | Preedit | nuốt protocol delete |
| Firefox | Preedit (mặc định chung) | NonPreedit cũng đo tốt |
| Không có rule | Preedit | an toàn nhất |

## Đã biết và cách xử lý

| Tình huống | Giải thích | Cách làm |
|---|---|---|
| Preedit không hiển thị ở Sublime | Sublime không render composition (app-side) | chuyển Sublime sang Surrounding (rule mặc định đã có) |
| Preedit nền đảo màu ở GNOME Console (kgx) | vte tự render preedit như vậy | không sửa được từ engine |
| Web framework nhân đôi preedit (`tttutuntung…`) | trang web tự copy composition vào input — lỗi site, mọi IME gặp | dùng mode không-preedit cho app đó; không có rule theo website (chỉ thấy prgname) |
| Chrome gợi ý chèn text quanh từ | engine phát hiện (ST) và rebuild composition; gõ nhanh quanh suggestion hiếm khi lệch | giữ Preedit (mặc định) |
| JetBrains "không gõ được" | thường do input source đang là bàn phím Anh (xkb us) — IBus không nhận phím | `Super+Space` chuyển về MăngRS |
| Chế độ Smooth trên GNOME Wayland không xóa được | injection bị rơi ở daemon/mutter — đặc thù Wayland | engine tự fallback Preedit; dùng các mode này trên X11 |

## Dấu hiệu cần thêm App Rule

Chữ bị **nhân đôi** khi gõ dấu ở NonPreedit, hoặc **mất dấu**, hoặc text **nhảy vị trí**
— app đó đang nuốt xóa/gửi ST lạ. Mở GUI → Applications → thêm rule theo tên app (GUI
gợi ý danh sách app đã thấy, lưu trong `~/.cache/ibus-mangrs/clients`). Preedit luôn là
lựa chọn an toàn.

Chi tiết kỹ thuật phân loại client: [../02-kien-truc-bo-go.md](../02-kien-truc-bo-go.md)
và `src/engine.rs::effective_mode`. Kết quả kiểm thử thủ công theo từng app/phiên:
`audit/typing-modes-report.md` §11 (nhiều ô còn "chưa kiểm").

## Đọc tiếp

- [04-cau-hinh.md](04-cau-hinh.md) — cú pháp `[[app_rules]]`.
- [06-xu-ly-su-co.md](06-xu-ly-su-co.md) — chẩn đoán khi chưa rõ nguyên nhân.
