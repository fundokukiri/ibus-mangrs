# 02 — Thiết lập IBus

Sau khi cài (xem [01-cai-dat.md](01-cai-dat.md)) và chạy `ibus restart`, thêm MăngRS
vào danh sách nguồn gõ.

## GNOME

1. **Settings → Keyboard → Input Sources → +** → chọn **Vietnamese** → **MăngRS**.
2. `Super+Space` chuyển qua lại giữa các nguồn gõ; biểu tượng bánh răng cạnh engine mở
   cửa sổ cài đặt.

Bằng dòng lệnh (tương đương bước trên):

```bash
gsettings set org.gnome.desktop.input-sources sources \
  "[('xkb', 'us'), ('ibus', 'ibus-mangrs')]"
```

**Kiểm tra:** `ibus engine` — phải in `ibus-mangrs` khi nguồn đang chọn là MăngRS.

## KDE và môi trường khác

KDE Plasma dùng IBus qua hệ thống input method của it (System Settings → Input &
Output → Keyboard → Virtual Keyboard). Các bước chi tiết trên KDE chưa được kiểm trên
máy thật — ⚠️ Chưa xác minh: dự án chỉ kiểm trên GNOME.

## X11 và Wayland

- **Wayland (GNOME)**: mọi ứng dụng được compositor relay — chế độ Preedit/NonPreedit/
  Surrounding hoạt động; ba chế độ gõ bằng phím thật (Smooth/Super Smooth/Select) tự
  rơi về Preedit vì injection không tới được app (hành vi có chủ đích, xem
  `docs/02-kien-truc-bo-go.md`).
- **X11**: cả bảy chế độ dùng được; các chế độ uinput cần `/dev/uinput` ghi được
  (xem [01-cai-dat.md](01-cai-dat.md)).

## Khi engine không xuất hiện

Nếu MăngRS không liệt kê trong Input Sources sau khi cài: chạy
`ibus write-cache --system` rồi `ibus restart`; kiểm tra file
`/usr/share/ibus/component/ibus-mangrs.xml` tồn tại. Chi tiết thêm ở
[06-xu-ly-su-co.md](06-xu-ly-su-co.md).

## Đọc tiếp

- [03-su-dung.md](03-su-dung.md) — bắt đầu gõ tiếng Việt.
