# 13 — Thuật ngữ và tài liệu tham khảo

> Bảng đối chiếu Việt–Anh cho các thuật ngữ dùng xuyên suốt tài liệu, kèm nghĩa *trong
> ngữ cảnh dự án này*.

## Thuật ngữ

| Thuật ngữ (EN) | Tiếng Việt / giải thích trong dự án |
|---|---|
| Input method (IM) | Bộ gõ: thành phần chuyển chuỗi phím thành chữ của ngôn ngữ đích |
| Engine | Tiến trình xử lý phím cho một input source của IBus (`src/server.rs`) |
| IBus daemon | Điều phối input method; private D-Bus bus, spawn engine qua component XML |
| Component XML | File đăng ký engine với daemon (`data/ibus-mangrs.xml`) |
| keyval / keysym | Mã X11 của phím đã map layout (vd. `0x61` = 'a'); decode ở `src/keymap.rs` |
| Preedit | Text đang soạn hiển thị inline trong app (thường gạch chân), chưa vào buffer |
| Commit | Đưa text thật vào buffer app tại caret |
| Surrounding text (ST) | Báo cáo buffer của client: `(text, cursor, anchor)` |
| Capability (caps) | Bit client báo khả năng (preedit, ST...); bit có thể flip, ST report mới đáng tin |
| Relay context | Client do mutter/ibus-daemon giữ hộ (gnome-shell/mutter/fake/client-None) — cấm repair-delete và injection |
| Composition | Trạng thái đang ghép một từ; buffer nội bộ của engine/core |
| Transformation | Quy tắc biến đổi phím (Telex `aa`→â, VNI `6`→â...) — dữ liệu ở core |
| Tone mark | Dấu thanh (sắc/huyền/hỏi/ngã/nặng) |
| Diacritic / vowel mark | Dấu hình (â ê ô ơ ư ă đ) |
| Modern / Traditional tone style | Vị trí dấu thanh `hòa` (mới) / `hoà` (cũ) — `ToneStyle` |
| Free tone marking | Dấu thanh gõ bất kỳ vị trí trong từ (`hoangf` → `hoàng`) |
| Terminator | Ký tự kết thúc từ (space, dấu câu, số...) — `is_word_terminator` |
| Auto-correct / revert | Core hoàn nguyên composition vô nghĩa về raw |
| Auto-restore | Ở cuối từ, thay từ vô nghĩa đã biến đổi bằng raw keys (`on_finish`) |
| Macro | Gõ tắt `key` → expansion, `$TIME`/`$DATE` |
| Word-mode lock | Khóa mode suốt một từ, chống flip giữa chừng (`key_mode`) |
| Echo | Phím inject qua `/dev/uinput` quay về engine như key event; phải forward nguyên vẹn |
| Deferred commit | Commit chờ đủ echo để không chạy trước phép xóa (`defer_commit`) |
| Stale (ST report / echo) | Cũ: report trước commit gần nhất / echo quá 150ms |
| Repair delete | Xóa sửa drift theo ST (`DeleteSurrounding` với offset âm) |
| RewriteByBackspace | Nhấn k+extra Backspace thật rồi commit lại hậu tố chung |
| uinput | Thiết bị bàn phím ảo kernel (`/dev/uinput`) để inject phím thật |
| XTest | Cơ chế X11 giả lập phím — dự án **không** dùng (uinput thay) |
| PassThrough / Forward | Phím không bị xử lý, để app tự nhận (`Action::Forward`) |
| Poison state / poison guard | Trạng thái core bị "nhiễm" sau undo dấu; wrapper chặn bằng `output_override` |
| Golden test | Test bảng (chuỗi phím → text cuối) qua buffer giả lập (`src/golden.rs`) |
| FakeClient | Client giả lập có thể cấu hình lỗi (nuốt delete, stale ST...) |
| Crate | Thư viện/package Rust |
| Trait | Kiểu interface của Rust |

## Tài liệu tham khảo

- `ARCHITECTURE.md` — protocol facts đã kiểm chứng trên dây + engine invariants (nguồn
  tốt nhất khi nghi ngờ một hành vi protocol).
- `docs/gap-analysis.md` — mọi tùy chọn × nơi cài × trạng thái (từ code).
- `docs/bamboo-audit.md` — API core được dùng / bị bỏ và lý do.
- `audit/typing-modes-report.md` — audit chế độ gõ: bản đồ mode, dấu vân tay, TM-findings.
- ibus-bamboo (Go, tham chiếu hành vi): `analysis/ibus-bamboo/` — bảng mode gốc tại
  `config/flags.go::ImLookupTable`.
- bamboo-core API surface: doc crate trong `bamboo-core/src/lib.rs`.
- IBus engine interface: `ibus/engineproxy.c` (được nhắc trong ARCHITECTURE.md cho việc
  xác minh FocusId) — tài liệu chính thức của project IBus.

## Đọc tiếp

- [12-faq.md](12-faq.md) — các câu hỏi đã được trả lời bằng chính thuật ngữ này.
- [README.md](README.md) — mục lục tài liệu.
