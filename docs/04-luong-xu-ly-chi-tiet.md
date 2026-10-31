# 04 — Luồng xử lý chi tiết theo kịch bản

> Chương này lần theo từng phím trong các kịch bản gõ thật, với bảng trạng thái từng
> bước: phím vào → trạng thái wrapper/engine → text trên màn hình. Mọi chuỗi đều là
> hành vi thật của engine (có test chốt trong `src/golden.rs`, `src/engine.rs::tests`).
> Môi trường giả định: Telex (trừ khi nói khác), client lý tưởng có ST, Unicode.

Ký hiệu: **Màn hình** = buffer ứng dụng (text thật); **Composition** = preedit đang hiển thị
hoặc buffer nội bộ của wrapper.

## Kịch bản 1: gõ "viet" rồi "j" → "việt" (Telex, `j` = nặng)

| # | Phím | Wrapper (`process_char`) | Engine làm gì | Màn hình |
|---|---|---|---|---|
| 1 | `v` | raw="v", bamboo → "v", PassThrough | Forward | `v` |
| 2 | `i` | "vi", PassThrough | Forward | `vi` |
| 3 | `e` | "vie", PassThrough | Forward | `vie` |
| 4 | `t` | "viet" (âm tiết hợp lệ), PassThrough | Forward | `viet` |
| 5 | `j` | rule DauNang → "việt" | `Outcome::Edit{delete 1, insert "ệt"}` → NonPreedit: `DeleteBefore(1)` + `Commit("ệt")` | `việt` |
| 6 | Space | terminator → `Finish{word:"việt", valid:true}` | Preedit: `HidePreedit + Commit("việt")`; inline: chỉ Forward | `việt ` |

NonPreedit chế độ hành xử như các bước trên; ở **Preedit** các bước 1–5 chỉ cập nhật
`UpdatePreedit("v", "vi", ..., "việt")`, chữ thật chỉ xuất hiện ở bước 6. Dấu vân tay đầy
đủ từng action được pin trong `src/golden.rs::fingerprint`.

## Kịch bản 2: gõ dấu sau — free tone marking (`hoangf` → `hoàng`)

`free_tone_marking = true` (mặc định, `src/config.rs`). Chuỗi `h o a n g f`:

| # | Phím | Kết quả |
|---|---|---|
| 1–5 | `hoang` | composition "hoang", màn hình `hoang` (PassThrough từng phím) |
| 6 | `f` | rule DauHuyen vẫn áp được lên vần "ang" → `Edit{delete 2, insert "àng"}` → `hoàng` |

Ngược lại nếu `free_tone_marking = false`, bamboo-core yêu cầu dấu gõ ngay sau vị trí
đặt dấu (`hoangf` bị revert về raw). Quyền bật/tắt nằm ở `bamboo_core::ConfigBuilder::free_tone_marking`,
đi từ config qua `src/config.rs::bamboo_config`.

## Kịch bản 3: hoàn tác dấu bằng gõ lặp (`aa` → `â` → `a`)

| # | Phím | Wrapper | Màn hình | Ghi chú |
|---|---|---|---|---|
| 1 | `a` | PassThrough | `a` | |
| 2 | `a` | rule A_Â → "â" | `â` (inline: `DeleteBefore(1)+Commit("â")`) | |
| 3 | `a` | rule A_Â áp lên "â" → bamboo undo dấu → "a" | `a` | wrapper nhận thấy "dấu bị gỡ trong từ" → `undo_in_word = true` (`src/bamboo_wrapper.rs::process_char`), cấm auto-restore "sửa" từ này ở cuối từ |

Trạng thái `undo_in_word` quan trọng: sau khi hoàn tác, state nội bộ của bamboo còn
"nhiễm độc" (`too` + `l` từng ra `toool`). Wrapper chặn bằng hai lớp: guard
`output > raw keys` → rebuild từ raw (`src/bamboo_wrapper.rs::process_char`), và
`output_override` ghim text hiển thị. Kết quả deterministic, có test
(`backspace_rebuild_keeps_later_keys_clean`).

## Kịch bản 4: gõ từ tiếng Anh xen kẽ (`window `)

| # | Phím | Hành vi |
|---|---|---|
| 1–6 | `window` | từng cặp chữ tạo composition không hợp lệ (vd. `wi`) → bamboo **auto_correct** revert về raw → wrapper thấy output == raw → PassThrough |
| 7 | Space | `Finish{word:"window", raw:"window", valid:false}` — word == raw nên không cần viết lại; ở Preedit vẫn commit "window" |

Nếu bamboo đã biến đổi (`word != raw`) mà từ vô nghĩa và không có trong từ điển:
`auto_restore` (mặc định bật) **khôi phục raw keys** ở cuối từ
(`src/engine.rs::on_finish`, nhánh auto-restore; đã có gate transport sau TM-fix trên
`dev`). Từ có trong `dictionary.txt` thì không bị restore (`src/engine.rs::on_finish`,
`dict_hit`). Hoàn tác dấu cũng được miễn restore (`undo_in_word`).

## Kịch bản 5: Backspace giữa từ — chuẩn unikey

| # | Phím | Hành vi | Màn hình |
|---|---|---|---|
| 1 | `hoangf` | → `hoàng` | `hoàng` |
| 2 | Backspace | `src/bamboo_wrapper.rs::backspace`: pop phím raw cuối (`f`), **reset engine, replay** các phím còn lại → "hoang" | `hoang` |
| 3 | Backspace | pop `g` → "hoan" | `hoan` |
| 4 | `i` `f` | gõ tiếp bình thường → "hoàng"... từ lại gõ dấu được | `hoàng` |

Điểm mấu chốt: Backspace hoàn tác *phím*, không phải *ký tự* — gõ lại `f` trả đủ dấu.
Vì sao replay thay vì gọi `remove_last_output_char` của bamboo: replay giữ transform
stack nhất quán; con đường kia từng gây kết quả không xác định (`mừi` thay vì `mười`) —
được khử bằng chính replay này (test `backspace_rebuild_keeps_later_keys_clean`).
Engine phía trên chuyển thành `DeleteBefore(n)` (+ `Commit` khi dấu di chuyển vị trí),
gated theo transport (`src/engine.rs::backspace`).

## Kịch bản 6: ngắt từ

Mọi ký tự sau đều kết thúc từ trong `src/bamboo_wrapper.rs::is_word_terminator`:
Space, Tab, Enter, dấu câu, số (với Telex/VIQR), `_`. Đặc thù theo method:

- **Telex/VIQR**: chữ số là terminator (`digit_key`) — `x1` không bị bamboo xóa mất từ.
  Riêng VIQR `0` là phím XoaDauThanh nên được miễn.
- **VIQR**: các phím dấu `'` `` ` `` `?` `~` `.` `^` `+` `*` `(` được miễn terminator để
  tới bamboo; bamboo áp dấu khi hợp lệ, giữ literal khi không (`(em` → `(em`,
  `hoi..` → `hoi.` — gõ đôi ra dấu câu, test golden VIQR).
- **Microsoft layout**: `[ ] { }` được miễn (`ms_bracket_key`) vì là phím ư/ơ duy nhất
  của layout.
- **bracket mode bật** (tùy chọn `bracket`): `[ ]` thành phím ơ/ư cho Telex
  (`src/bamboo_wrapper.rs::bracket_key`).

Terminator tới engine → `Outcome::Finish` → `on_finish`: macro → auto-restore → commit
(precht)/không cần (inline). Phím terminator bản thân luôn được Forward.

## Kịch bản 7: viết hoa

- Người dùng gõ chữ hoa: keysym decode giữ case (`src/keymap.rs::keysym_to_char`),
  bamboo-core giữ case trong composition — golden chạy cả biến thể Capitalized/UPPER
  (`src/golden.rs::variants`).
- **Auto-capitalize** (`auto_capitalize`, mặc định tắt): sau `. ! ?` Enter, chữ cái đầu
  từ mới được đưa vào composition ở dạng hoa (`src/engine.rs::process_char`, `cap`);
  nếu PassThrough thì commit chữ hoa thay vì forward chữ thường (app sẽ gõ chữ thường).

## Kịch bản 8: chữ số và `_` kề từ (Telex)

| # | Phím | Hành vi |
|---|---|---|
| 1 | `tieeng` | composition "tiêng" (preedit) |
| 2 | `2` | digit → terminator: `Finish{word:"tiêng"}` → commit, forward `2` | 
| 3 | kết quả | `tiêng2` — bamboo-core nếu nhận trực tiếp số sẽ **xóa cả composition** (quirk), lý do wrapper chặn trước |

Test: `digits_terminate_the_word_and_are_forwarded`,
`digit_after_a_word_keeps_the_word` (`src/engine.rs::tests`).

## Đọc tiếp

- [05-tinh-nang.md](05-tinh-nang.md) — danh sách tính năng đầy đủ kèm hạn chế từng cái.
- [09-kiem-thu.md](09-kiem-thu.md) — các chuỗi ở trên được khẳng định bằng test nào.
