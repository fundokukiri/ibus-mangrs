# 05 — Tính năng: những gì thực sự có trong code

> Chương này liệt kê từng tính năng với ba thông tin: nó hoạt động bên trong ra sao,
> mã nguồn ở đâu, và hạn chế đã biết. Chỉ liệt kê thứ có thật — các mục "tương lai"
> nằm ở chương 11 (Hạn chế & việc còn dở).

## Kiểu gõ (4)

Telex, VNI, VIQR, Microsoft layout — preset của bamboo-core, chọn qua
`MethodConfig` (`src/config.rs`), áp qua `src/bamboo_wrapper.rs::set_method`.
Bảng định nghĩa từng preset là dữ liệu (`bamboo-core/src/input_method/definitions.rs`).
Hạn chế: Microsoft layout là kiểu *insert* — chữ hoa chèn vào giữ nguyên thường
(`TI3NG8` → `TIếNG`, upstream bamboo-core); ơ/ư chỉ nằm trên bracket nên được miễn
terminator riêng (`ms_bracket_key`).

## Bảng mã (11)

Unicode (mặc định), NFD, TCVN3, VNI, VIQR, VISCII, VPS, CP1258, BKHCM, Vietware X,
Vietware Full — map trong `src/bamboo_wrapper.rs::Charset::to_bamboo` sang
`bamboo_core::Charset` (17 biến thể upstream, MăngRS expose 11). Encode chỉ áp dụng cho
text commit, không áp dụng preedit (`src/engine.rs::on_finish`/`rewrite`). Hạn chế:
bảng cũ đổi số ký tự nên luôn viết lại cả từ (`src/engine.rs::rewrite`), không chơi
delta từng phím.

## Tám chế độ hiển thị

Chi tiết cơ chế ở chương 02. Danh sách + fallback trong `src/engine.rs::effective_mode`
và `src/config.rs::ModeConfig`. Chọn qua menu (radio `display:`), GUI, cycle hotkey
(`cycle_mode_hotkey`), hoặc App Rules theo app.

## App Rules (mode theo ứng dụng)

`[[app_rules]]` trong config: `pattern = mode`, khớp substring không phân biệt hoa
thường trên prgname từ `FocusInId` (`src/engine.rs::client_rule_mode`). Danh sách
default nhắm client nuốt delete (chrome/electron-family → preedit, sublime_text →
surrounding) — `src/config.rs::default_app_rules`. GUI ghi lại các app đã thấy vào
`~/.cache/ibus-mangrs/clients` (`src/server.rs::note_client`, tối đa 50).
Hạn chế: app không gửi client id (Sublime/XIM) không match được rule; chỉ match được
theo tiến trình, không theo tab/website.

## Bộ máy click giữa chừng preedit (issue #14)

Ba hành vi chọn được (`click_action`): **bamboo** (mặc định — từ treo tự commit sau
800ms `preedit_commit_timeout_ms`, click muộn chỉ mất từ dở, phím dấu muộn re-entry
vào từ đã commit qua `idle_raw` + `replay_raw`), **keep** (click ẩn từ, quay lại chỗ
cũ khôi phục, không bao giờ idle-commit), **relocate** (xóa span giữa caret mới và chỗ
gõ, commit lại với từ kèm — fallback park khi client nuốt delete). Toàn bộ trong
`src/engine.rs` (`on_client_reset`, `set_surrounding`, `click_relocate_word`,
`park_pending_word`, `tick`). Hạn chế: protocol không thể ghi text vào vị trí caret đã
rời đi — keep chỉ "giữ", relocate di chuyển cả span kề.

## Gõ tắt (macro)

`~/.config/ibus-mangrs/macros.toml`, `key = "expansion"`, `$TIME`/`$DATE` theo format
strftime, viết hoa thông minh (key hoa toàn phần → expansion hoa toàn phần).
`src/macros.rs::Macros::expand`. Kích hoạt ở cuối từ trên raw keys; key trùng từ tiếng
Việt hợp lệ không expand (`src/engine.rs::expand_macro`, gate `valid`).
Hạn chế: expansion cần transport xóa — bị gate như mọi rewrite.

## Viết tắt giữ đ (abbreviations)

`ddc` → `đc`, `ddk` → `đk`... — auto_correct của bamboo revert composition vô nghĩa
(đc không có nguyên âm), wrapper nhận ra raw bắt đầu `dd` và **ghim đ**:
`src/bamboo_wrapper.rs::process_char` (nhánh `abbrev`), tắt được qua
`config.abbreviations`. Được ghim bằng `abbrev_fixed` để auto-restore không phá ở
cuối từ.

## Khôi phục từ nước ngoài (auto_restore)

Cuối từ: vô nghĩa + khác raw + không có trong dictionary + không có undo-in-word →
commit raw keys (`src/engine.rs::on_finish`). Tắt được. Từ trong `dictionary.txt`
(tự điển người dùng) được bảo vệ.

## Bypass URL/email/path

Chữ ngay sau `/ : @ . _ ~ = # & % ?` giữ raw tới terminator kế
(`src/engine.rs::URL_CONTEXT_CHARS`, `url_word`; `-` bị loại chủ ý để không phá em-dash).
Ví dụ `example.tieeng` không thành `example.tiêng`.

## Tính năng phụ trên đường phím

| Tính năng | Code | Ghi chú |
|---|---|---|
| `w` → `ư` (`w2u`: off/nonstart/everywhere) | bamboo `W2uMode` qua `src/config.rs` | |
| `[ ]` → `ơ ư` (`bracket`) | bamboo `BracketMode`; wrapper miễn terminator khi bật | |
| `--` → `—` (`em_dash`) | `src/engine.rs::process_char` | cần delete transport |
| Hai space → `. ` (`double_space_period`) | cùng nơi | mặc định tắt |
| Tự viết hoa sau `.!?` Enter (`auto_capitalize`) | `src/engine.rs` (`capitalize_next`) | không chạy ở OFF/uinput |
| Ẩn gạch chân preedit (`hide_underline`) | `src/server.rs::emit_with` bỏ `IBusAttribute` | phụ thuộc app render |
| Keypad số + `.` + KP Enter/Delete | `src/keymap.rs::keysym_to_char` | VNI gõ được bằng numpad |
| Unicode channel clipboard | `src/paste.rs` | chỉ uinput modes; thay clipboard 600ms |

## Kiểm tra chính tả & từ điển

`data/vn-words.txt` (38 350 từ, nguồn GPL — `data/README.md`) nạp khi Spell Check bật
(`src/spellcheck.rs`, 3 đường ưu tiên: config → `/usr/share/ibus-mangrs/` → repo).
⚠️ Lookup-only theo quyết định owner: `Engine::known_word()` chưa có consumer — danh sách
từ không tham gia auto-correct (đó là phonotactics của bamboo-core).
`dictionary.txt` thì có tác dụng thật: chặn auto-restore (`src/engine.rs::on_finish`).

## Cấu hình nóng

`config.toml` reload theo mtime tối đa 1 lần/giây trên đường phím
(`src/server.rs::maybe_reload_config`); đổi mode/charset giữa chừng commit từ dở trước
khi rebuild (`src/engine.rs::apply_config`). Schema đầy đủ ở chương 08 và
`docs/guide/04-cau-hinh.md`.

## Debug log

`debug_log = true` → `~/.cache/ibus-mangrs/debug.log`, ms-timestamp, ghi quyết định
(focus, caps, ST cursor/len, transport, echo, timeout) — **không ghi nội dung gõ**, trừ
`brief()` 24 ký tự của buffer app trong log verify (chủ dự án cần quyết giữ/gỡ).
`src/debug_log.rs` — handle mở một lần cho cả phiên.

## Đọc tiếp

- [08-du-lieu-va-cau-hinh.md](08-du-lieu-va-cau-hinh.md) — từng file config/data trong đĩa.
- [11-quyet-dinh-thiet-ke.md](11-quyet-dinh-thiet-ke.md) — hạn chế đã biết & việc còn dở.
