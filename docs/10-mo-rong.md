# 10 — Mở rộng: thêm cái gì thì chạm vào đâu

> Chương này là bản đồ điểm chạm (touch points) cho ba việc hay gặp: thêm kiểu gõ, thêm
> bảng mã, thêm tính năng hành vi. Mục tiêu là *khái niệm + danh sách file/hàm*, không
> phải recipe từng bước.

## Thêm một kiểu gõ mới

Ví dụ muốn thêm preset `telex_2` (đã có sẵn upstream, MăngRS chưa expose):

1. **Core đã có**: kiểm tra `bamboo-core/src/input_method/definitions.rs` — nếu preset
   tồn tại (`TELEX_2`, `TELEX_VNI`, `VIQR`...) thì không cần đụng core. Nếu chưa: thêm
   `InputMethodDef` (dữ liệu `phf_map!`) + preset enum variant
   (`input_method/mod.rs::InputMethodPreset`).
2. **Wrapper**: thêm variant vào `src/bamboo_wrapper.rs::Method` + mapping trong
   `Method::to_bamboo`.
3. **Config**: thêm variant `src/config.rs::MethodConfig` (serde lowercase) + `from_key`
   (menu key) + `From<MethodConfig> for Method`.
4. **Terminator**: tự hỏi phương thức mới dùng ký tự nào trong
   `is_word_terminator` làm phím dấu — thêm exemption theo mẫu `viqr_mark_key`/
   `ms_bracket_key`/`digit_key` (`src/bamboo_wrapper.rs::process_char`). Đây là bẫy
   kinh điển: VIQR từng gõ không được dấu nào vì thiếu exemption.
5. **Re-entry dấu muộn**: `src/engine.rs::is_late_mark_key` cần nhánh cho method mới
   (danh sách phím dấu theo method).
6. **Test**: bảng golden mới (`src/golden.rs`, thêm `const` + test runner) — mục tiêu
   tối thiểu một từ cho *mỗi* rule của preset, ×3 biến thể hoa; thêm một test
   `..._in_every_display_mode` như `viqr_marks_apply_in_every_display_mode`.
7. **Menu/GUI**: `src/ibus_prop.rs` (radio `method:`) và `src/gui.rs`.

## Thêm một bảng mã mới

1. `src/bamboo_wrapper.rs::Charset` thêm variant + `to_bamboo` (bamboo-core có sẵn 17
   biến thể — `encoder/tables.rs`; nếu bảng chưa có ở core thì phải thêm tại core
   trước).
2. `src/config.rs::CharsetConfig` + `key`/`from_key` (serde name = menu key).
3. Nếu bảng mới **đổi số ký tự** (hầu hết bảng cũ vậy): mọi logic đã viết theo "encode
   rồi đếm" (`src/engine.rs::rewrite`) — thường không cần sửa; kiểm tra test
   `legacy_charset_rewrites_the_whole_word` rồi thêm biến thể của bảng mới vào
   `legacy_charsets_encode_vietnamese` (`src/bamboo_wrapper.rs::tests`).
4. GUI: `src/gui.rs` combo charset.

## Thêm một tính năng hành vi (kiểu `--` → `—`)

Mẫu có sẵn: em-dash. Điểm chạm theo thứ tự:

1. **Config**: key mới trong `src/config.rs::Config` + `Default` + serde (nhớ `merge`
   tự giữ key lạ — không cần sửa).
2. **Engine state**: trường mirror trong `src/engine.rs::Engine` + gán trong
   `apply_config`.
3. **Điểm bắn**: trong `process_char` (trước/sau nhánh mode?) — decide placement:
   per-key (mọi mode) hay chỉ inline modes. Nếu tính năng cần **xóa text**: bắt buộc
   qua `tracked_delete(n, insert_len)` + gate `deletes_allowed()` — mọi rewrite không
   gate đều là bug tiềm ẩn (lịch sử: TM-001, C3-gates trong `git log`).
4. **Menu toggle**: `src/ibus_prop.rs` + `src/server.rs::activate_property` (arm toggle)
   + `src/gui.rs`.
5. **Test**: unit có/không (mẫu `em_dash_replaces_double_hyphen` /
   `em_dash_disabled_by_default`), + hàng no-transport nếu tính năng xóa text
   (mẫu `no_delete_transport_keeps_em_dash_and_double_space_literal`).

## Thêm một chế độ hiển thị mới

Đắt nhất trong danh sách:

1. `src/engine.rs::Mode` + `src/config.rs::ModeConfig` (2 chiều `From`) + `from_key`.
2. `effective_mode()`: fallback khi thiếu điều kiện (mẫu: uinput modes cần
   `uinput_available` và không relay).
3. Đường xử lý phím: `process_char` match — nhánh riêng hay tái dùng? Nếu mode cần
   transport riêng: `Action` variant mới + `emit_with` arm + echo/defer semantics
   (đọc kỹ cơ chế `pending_injected`/`defer_commit` trước).
4. Word-mode lock phải bao phủ mode mới (tự nhận qua `key_mode`).
5. Test: golden + matrix thêm mode vào danh sách; quy tắc bắt buộc: hành vi phải giống
   nhau giữa các mode trên client lý tưởng (test `ideal_clients_reproduce_the_golden_
   text_in_every_mode` sẽ nhắc bạn).

## Thêm transport xóa mới (ví dụ XTest)

1. `src/uinput.rs` hoặc module mới với interface như `Transports`
   (`src/server.rs::Transports` — 4 closure: backspaces/keys/select_left/paste).
2. `emit_with`: chọn transport theo client — nhớ hai cấm đo: **không bao giờ** trên
   relay context (`engine.relay_context()`), **không bao giờ** khi chưa có ST report
   (bất biến #1 `ARCHITECTURE.md`).
3. Nếu transport không qua IBus: phải xử lý echo (`add_injected`, `take_injected_echo`,
   `ECHO_STALE`) — commit defer đến echo là bất biến #3.
4. Test với `fake_transports` — không yêu cầu thiết bị thật.

## Khi chạm vào bamboo-core (upstream)

Submodule `bamboo-core/` pin tag để đọc; build lấy crates.io. Muốn thay đổi core:
fork crate, trỏ `Cargo.toml` về fork, và nhớ mọi quirk wrapper đã bù cho core
(`docs/bamboo-audit.md` — phần "What ibus-mangrs implements itself, and why") sẽ cần
đối chiếu lại, đặc biệt: delta tự tính, replay backspace, terminator interception.

## Đọc tiếp

- [09-kiem-thu.md](09-kiem-thu.md) — dạng test đi kèm từng loại thay đổi.
- [11-quyet-dinh-thiet-ke.md](11-quyet-dinh-thiet-ke.md) — bối cảnh của các invariant phải giữ.
