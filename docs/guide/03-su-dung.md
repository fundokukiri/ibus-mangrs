# 03 — Sử dụng

Mọi thao tác dưới đây đều có ở cửa sổ cài đặt (biểu tượng bánh răng cạnh tên engine
trong menu nguồn gõ, hoặc `ibus-engine-ibus-mangrs -gui`). Chế độ hiển thị mặc định là
**Preedit** (gạch chân), kiểu gõ **Telex**, bảng mã **Unicode**.

## Bật/tắt tiếng Việt

Menu nguồn gõ → **Enable Vietnamese**, hoặc tắt hẳn bằng cách chuyển sang nguồn bàn
phím Anh. Khi tắt, macro vẫn chạy nếu `macro_in_off_mode` bật.

## Bốn kiểu gõ

Chọn trong menu → *Input Method* hoặc GUI tab Kiểu gõ. Bảng cách gõ:

| Kết quả | Telex | VNI | VIQR | Microsoft |
|---|---|---|---|---|
| â ê ô | `aa ee oo` | `a6 e6 o6` | `a^ e^ o^` | `a1 e3 o4` (chèn) |
| ă | `aw` | `a8` | `a(` | `a1` (chèn) |
| ơ ư | `ow uw` | `o7 u7` | `o+ u+` (hoặc `o* u*`) | `o] u[` (bracket) |
| đ | `dd` | `d9` | `dd` | `d0` (chèn đ) |
| sắc huyền hỏi ngã nặng | `s f j x r` | `1 2 3 4 5` | `' `` ? ~ .` | `8 5 6 7 9` |
| xóa dấu | (gõ lại phím dấu) | `0` | `0` | — |

Ví dụ: `tieengs` / `tie6ng1` → **tiếng**; `hoangf` → **hoàng** (dấu tự do, bật mặc định);
`ddc` → **đc** (viết tắt giữ đ, bật mặc định).

Lưu ý VIQR/Microsoft dùng chính ký tự dấu câu/bracket làm phím — engine phân biệt theo
ngữ cảnh; gõ đôi để ra ký tự thật (`hoi..` → `hoi.`).

## Hoàn tác dấu và sửa từ

Gõ lại phím dấu để gỡ dấu (`tô` + `o` → `too`). **Backspace** hoàn tác *phím cuối* theo
chuẩn unikey: `hoàng` → Backspace → `hoang` (gõ lại `f` có dấu trở lại).

## Gõ tiếng Anh, URL, email

Chỉ gõ bình thường — từ vô nghĩa tự khôi phục nguyên bản (Auto-restore). Từ ngay sau
`/ : @ . _ ~ = # & % ?` giữ nguyên (`example.tieeng` không bị biến thành
`example.tiêng`).

## Chế độ hiển thị

Menu → *Chế độ gõ* (Preedit / NonPreedit / Surrounding / Off) hoặc đủ 8 chế độ trong
GUI tab Chế độ gõ; gán phím tắt chuyển vòng trong GUI (Cycle Mode Hotkey). Mỗi app có
thể dùng chế độ riêng qua **App Rules** (GUI tab Applications).

| Chế độ | Dùng khi |
|---|---|
| Preedit (mặc định) | an toàn nhất, gạch chân dưới chữ đang gõ |
| NonPreedit | không muốn gạch chân, app xử lý xóa tốt (Firefox, gedit) |
| Surrounding | app nuốt xóa nhưng báo surrounding text |
| Smooth / Super Smooth / Select | terminal/app quirky trên **X11** (cần `/dev/uinput`) |
| OFF / Default Typing | gõ tiếng Anh thuần / app chạm vào mọi phím |

## Tính năng khác

- **Macro** (`macro_enabled`): `~/.config/ibus-mangrs/macros.toml`, ví dụ
  `ddq = "đại đội trưởng"`, hỗ trợ `$TIME`/`$DATE`. GUI có trình soạn.
- **Viết hoa tự động** sau `. ! ?` Enter (`auto_capitalize`, mặc định tắt).
- `--` → `—` (`em_dash`), hai space → `. ` (`double_space_period`, thử nghiệm).
- `w` → `ư` (`w2u`), `[ ]` → `ơ ư` (`bracket`) — tùy chọn kiểu gõ phụ.
- **Ẩn gạch chân** preedit (`hide_underline`).
- **Clipboard channel** cho chế độ uinput: chữ gửi qua clipboard + Ctrl+V (thay clipboard
  600ms) — dùng cho game/app đọc phím thô.
- **Hành vi khi click giữa chừng preedit** (`click_action`): Bamboo (mặc định — nghỉ
  800ms tự commit, dấu muộn vẫn áp được), Keep word (click ẩn, quay lại khôi phục),
  Relocate (từ luôn rơi đúng chỗ đã gõ).

Chi tiết kỹ thuật từng tính năng: [../05-tinh-nang.md](../05-tinh-nang.md).

## Đọc tiếp

- [04-cau-hinh.md](04-cau-hinh.md) — bảng tùy chọn đầy đủ.
- [05-tuong-thich-ung-dung.md](05-tuong-thich-ung-dung.md) — app nào nên dùng chế độ nào.
