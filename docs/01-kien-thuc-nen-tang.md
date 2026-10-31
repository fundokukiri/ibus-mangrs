# 01 — Kiến thức nền tảng

> Chương này cung cấp đúng đủ nền: IBus tương tác với hệ thống như thế nào, và tiếng Việt
> đặt ra những ràng buộc gì cho một bộ gõ. Nếu bạn đã rõ hai thứ này, có thể bỏ qua.

## Input method trên Linux: IBus, D-Bus, engine

**IBus** (Intelligent Input Bus) là khung input method phổ biến trên Linux. Kiến trúc của
nó gồm ba vai trò:

- **ibus-daemon** — tiến trình điều phối. Nó tạo ra một *private D-Bus bus* riêng (không
  phải session bus), quản lý registry các engine (đăng ký qua *component XML*, ví dụ
  `data/ibus-mangrs.xml` của dự án), và route phím từ ứng dụng tới engine đang active.
- **Engine** — tiến trình riêng do daemon spawn khi người dùng chọn input source. Engine
  của dự án là binary `ibus-engine-ibus-mangrs` (`Cargo.toml` mục `[[bin]]`), nằm trên bus
  với tên `org.freedesktop.IBus.mangrs` và object path `/org/freedesktop/IBus/Engine/1`
  (`src/server.rs`, hằng `BUS_NAME`/`ENGINE_PATH`).
- **Ứng dụng** — qua IM module của toolkit nó dùng (GTK, Qt) hoặc XIM trên X11. Trên
  GNOME Wayland, mọi client được **mutter relay**: engine chỉ thấy context là
  `gnome-shell` (`src/engine.rs::relay_context`).

Giao tiếp là D-Bus thuần: ứng dụng gọi method `ProcessKeyEvent(keyval, keycode, state)`;
engine trả `true` nếu *nuốt* phím (key consumed), `false` nếu forward. Engine phát
kết quả bằng các **signal** — chi tiết từng loại ở chương 02. Các fact đã kiểm chứng
trên dây (kiểu dữ liệu trả lời, cách gói `IBusText`...) được ghi trong `ARCHITECTURE.md`
mục "IBus Protocol Facts" — dự án cố tình không suy diễn lại chúng.

Ba khái niệm mà toàn bộ tài liệu này xoay quanh:

| Khái niệm | Nghĩa | Trong code |
|---|---|---|
| **preedit** | Dải text đang gõ, hiển thị inline trong ứng dụng với gạch chân, *chưa* phải text thật. Ứng dụng render nó, có thể xóa/thay toàn bộ khi engine gửi bản mới | signal `UpdatePreeditText` (`src/server.rs::emit_with`, arm `Action::UpdatePreedit`) |
| **commit** | Đưa text thật vào buffer ứng dụng, tại vị trí con trỏ hiện tại | signal `CommitText` |
| **surrounding text (ST)** | Client *báo* cho engine biết buffer của nó: `(text, cursor, anchor)`. Đây là mắt xích để engine kiểm chứng text mình đã gửi có thực sự nằm đó không | method `SetSurroundingText` (`src/server.rs::engine_call`), engine yêu cầu bằng signal `RequireSurroundingText` khi FocusIn |

## Đặc trưng của bộ gõ tiếng Việt

### Bốn kiểu gõ phổ biến

Một âm tiết tiếng Việt có cấu trúc **thanh + đầu + vần (nguyên âm + dấu hình) + cuối**.
Các kiểu gõ mã hóa phần "vần + thanh" bằng chuỗi phím Latin:

| Kiểu gõ | Cách đặt dấu hình (ă â ê ô ơ ư đ) | Cách đặt dấu thanh (sắc huyền hỏi ngã nặng) | Ví dụ "tiếng" |
|---|---|---|---|
| **Telex** | nhân đôi/chèn phím: `aa`→â, `ee`→ê, `oo`→ô, `ow`→ơ, `uw`→ư, `aw`→ă, `dd`→đ | phím cuối từ: `s f j x r` | `tieengs` |
| **VNI** | số theo nguyên âm: `6`→â/ê/ô, `7`→ơ/ư, `8`→ă, `9`→đ | số 1–5 | `tie6ng1` |
| **VIQR** | ký tự dấu: `^`→âêô, `+`/`*`→ơư, `(`→ă, `d`→đ | `'` `` ` `` `?` `~` `.` | `to^i'` |
| **Microsoft layout** | chèn nguyên âm đã đánh dấu (3=ê, 1=ă...) | số 5–9 | `ti3ng8` |

Bảng định nghĩa chuẩn của từng kiểu nằm trong `bamboo-core/src/input_method/definitions.rs`
(map phím → transformation, dạng dữ liệu `phf_map!` chứ không hardcode trong code). Các
preset được MăngRS chọn qua `src/bamboo_wrapper.rs::Method::to_bamboo`.

### Hai quy tắc đặt dấu thanh

- **Truyền thống**: dấu đặt trên ký tự đầu của vần khi vần có `ơ/ư/u + nguyên âm`
  kiểu `hoà`, `dưởng`.
- **Hiện đại (mặc định)**: dấu đặt trên ký tự mang dấu hình, `hòa`, `đường`.
  Điều khiển bằng `modern_tone_style` (`src/config.rs::bamboo_config` →
  `bamboo_core::ToneStyle`).

**Gõ dấu tự do** (`free_tone_marking`, mặc định bật): dấu thanh có thể gõ ở bất kỳ vị trí
nào trong từ, `hoangf` vẫn ra `hoàng` — bamboo-core xử lý, cấu hình đi qua
`src/config.rs::bamboo_config`.

### Vì sao gõ tiếng Việt khó hơn gõ Latin

1. **Một phím có hai nghĩa.** `s` vừa là chữ vừa là phím dấu sắc. Engine phải quyết định
   theo ngữ cảnh: có đang trong âm tiết hợp lệ không (`bamboo-core` phonotactics,
   `src/bamboo_wrapper.rs` dựa vào `is_valid_word`). Người dùng thoát bằng cách gõ lại
   phím dấu để hoàn tác — hành vi hoàn tác được wrapper cài lại theo chuẩn unikey
   (`src/bamboo_wrapper.rs::backspace`, pop phím raw cuối + replay).
2. **Text phải sửa trong ứng dụng.** `tie` đã commit rồi thì `e` thứ hai phải *xóa* phần
   đuôi đã nằm trên màn hình. Xóa trong app của người khác là nơi mọi lỗi phát sinh:
   có app nuốt `DeleteSurroundingText`, có app không báo ST, có app tự chèn text
   (autocomplete). Toàn bộ vận máy của dự án xoay quanh việc phân loại client
   (`src/engine.rs::effective_mode`, `client_cannot_delete_surrounding`) và chọn
   transport xóa an toàn.
3. **Ký tự kết thúc từ nhiều nghĩa.** Dấu câu, số, `_` đều là terminator — nhưng VIQR
   lại *dùng chính dấu câu làm phím dấu*, Microsoft layout dùng bracket cho ơ/ư. Bộ lọc
   terminator vì thế phải biết phương thức gõ đang active
   (`src/bamboo_wrapper.rs::process_char`, các exemption `viqr_mark_key`,
   `ms_bracket_key`, `digit_key`).
4. **Bảng mã cũ đổi độ dài ký tự.** VNI/TCVN3/VIQR... encode `â` thành 2+ ký tự, nên
   không thể replay delta từng phím — phải viết lại cả từ với số lượng xóa đếm theo
   *sau khi encode* (`src/engine.rs::on_edit` nhánh legacy, `src/engine.rs::rewrite`).

## Đọc tiếp

- [02-kien-truc-bo-go.md](02-kien-truc-bo-go.md) — tầng engine MăngRS xử lý tất cả những điều trên ra sao.
- [04-luong-xu-ly-chi-tiet.md](04-luong-xu-ly-chi-tiet.md) — lần theo từng phím trong các kịch bản thật.
