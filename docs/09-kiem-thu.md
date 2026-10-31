# 09 — Chiến lược kiểm thử

> Chương này giải thích cấu trúc 207 test, triết lý "golden qua buffer giả lập", cách
> FakeClient/chờ-lỗi hoạt động, và cách thêm ca mới. Tất cả chạy offline: không cần
> D-Bus, daemon hay `/dev/uinput`.

## Các tầng test

| Tầng | Nơi | Kiểm chứng gì |
|---|---|---|
| Golden bảng phím→text | `src/golden.rs` (TELEX 102 + VNI 96 + VIQR 24 dòng ×3 biến thể hoa, MS_LAYOUT 12) | 678 ca: chuỗi phím qua **engine thật** vào buffer giả lập ứng dụng phải ra đúng text |
| Unit engine/wrapper | `#[cfg(test)]` trong `src/engine.rs` (~120 test), `src/bamboo_wrapper.rs` (18) | hành vi từng nhánh: terminator, undo dấu, poison, auto-restore, click #14, re-entry… |
| Transport/emit | `src/server.rs` tests (20) | wire format `IBusText`, defer-commit-cho-echo, chọn transport theo client, dead-device |
| FakeClient matrix + fuzz | `src/golden.rs::matrix` | mode × fault (no_st, drop_delete, stale_st, app_edit, caret_moved, no_caps, no_client_id) × chuỗi; fuzz xorshift seed 0xC0FFEE |
| Hạ tầng | `src/keymap.rs`, `src/ibus_text.rs`, `src/config.rs`, `src/macros.rs`, `src/paste.rs`, `src/uinput.rs` | decode keysym, wire format, schema, clipboard debounce, layout input_event |

Lệnh: `cargo test` (mặc định uinput test tự skip khi không có `/dev/uinput`; đặt
`MANGRS_REQUIRE_UINPUT=1` để ép — `src/uinput.rs::device_opens_with_correct_permissions`).
Lưu ý môi trường dev của repo: rustup shim lỗi `unknown proxy name` → chạy
`env -u RUSTC_WRAPPER cargo ...` (ghi trong `AGENTS.md`).

## Triết lý golden: hành vi cuối cùng, không phải intermediate state

`typed_word` (`src/golden.rs`) nối `Engine::process_char` với một buffer `Vec<char>`:
mỗi `Action` được áp lên buffer đúng ngữ nghĩa IBus (xóa trước cursor, xóa theo
offset...), `Forward` đẩy phím thô vào, sau đó buffer được report lại qua
`set_surrounding` — y như một client thật. Kết quả so sánh với text kỳ vọng:

```rust
("tieengs", "tiếng"),
```

Chỉ hai dòng cho một từ, nhưng fail nghĩa là *text trên màn hình sai* — không phải chỉ
state nội bộ lệch. 678 ca gồm 3 biến thể (thường / Viết-hoa-chữ-đầu / HOA) — trừ
MS_LAYOUT chạy 1 biến thể vì rule insert upstream giữ case thường
(`src/golden.rs::run_table`, tham số `case_variants`).

Thêm ca mới: thêm dòng vào bảng tương ứng + dòng kỳ vọng vào `expected_for`
(`src/golden.rs::matrix`). Nếu kết quả thật khác kỳ vọng của bạn: kiểm tra trước xem
kỳ vọng có theo đúng quy tắc kiểu gõ đó không (VIQR gõ â bằng `a^`, không phải `aa`) —
đừng sửa test cho khớp code vô phương chứng minh.

## FakeClient: client có thể bị "hỏng"

`Faults` (7 cờ) + `FakeClient` mô hình hóa ứng dụng thật: buffer + caret, report ST,
và các lỗi — nuốt protocol delete, report stale, app tự chèn suggestion, caret nhảy,
không ST, thiếu cap preedit, không client id. Ma trận chạy 3 mode × 8 bộ lỗi × 10 chuỗi
phím; invariant được assert **trên từng batch action**:

- không `DeleteBefore` khi không có ST và không uinput;
- không repair `DeleteSurrounding` cho relay/no-ST;
- focus out xong: không preedit treo, không deferred, không pending echo
  (`assert_clean_after_focus_out`).

Fuzz (`fuzz_random_keystrokes_hold_the_invariants`) dùng xorshift64* seed cố định —
lỗi replay được bằng đúng seed, không cần dependency proptest (giới hạn của dự án:
không thêm crate khi không cần).

## Test transport mà không có thiết bị

`server.rs::tests` không bơm phím thật: `emit_with` nhận `Transports` injectable
(`fake_transports` — backspace luôn thành công/thất buggy theo test), nên đường
defer-commit-cho-echo, dead-device-keep-raw, wire format `(iu)`/`(vubu)` đều test được
offline. Echo mô phỏng bằng `add_injected` + `take_injected_echo`.

## Các test "hồi quy lịch sử" đáng đọc

Mỗi crash/lỗi thật của dự án có một test mang tên nó — đọc là hiểu lịch sử:

- `no_surrounding_data_never_produces_a_delete` — crash mutter/logout.
- `backspace_then_type_does_not_eat_a_letter` — delta bamboo stale.
- `toool_with_protocol_delete_undoes_the_circumflex` — poison state.
- `digits_terminate_the_word_and_are_forwarded` — số xóa từ.
- `lost_echoes_flush_in_the_main_loop_tick` — commit treo vì echo mất (TM-002).
- `dead_device_on_delete_dropping_client_keeps_raw` — nhân đôi khi thiết bị chết (TM-003).
- `swallowed_repairs_switch_to_backspace_injection` — repair pipeline.

Công cụ ngoài crate: `test/typing_tool.py`, `test/perf_tool.py` gõ qua kernel vào app
thật (xem `test/README.md`) — không chạy được trong CI sandbox, phục vụ kiểm chứng thủ
công trên desktop.

## Đọc tiếp

- [04-luong-xu-ly-chi-tiet.md](04-luong-xu-ly-chi-tiet.md) — nội dung của các ca golden.
- [10-mo-rong.md](10-mo-rong.md) — khi thêm tính năng, test nào phải đi kèm.
