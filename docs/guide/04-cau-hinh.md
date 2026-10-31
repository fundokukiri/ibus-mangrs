# 04 — Cấu hình

Tất cả cấu hình nằm trong `~/.config/ibus-mangrs/config.toml`, được nạp lại nóng trong
~1 giây (không cần `ibus restart` khi đổi). Bảng dưới khớp struct `Config` trong
`src/config.rs` (mặc định = giá trị `Default for Config` tại đó).

## Bật/tắt và phương thức

| Key | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `enabled` | bool | `true` | Bật xử lý tiếng Việt |
| `method` | string | `telex` | `telex` `vni` `viqr` `microsoft` |
| `mode` | string | `preedit` | 8 giá trị: `nonpreedit preedit surrounding smooth supersmooth select off defaulttyping` |
| `charset` | string | `unicode` | `unicode nfd tcvn3 vni viqr viscii vps cp1258 bkhcm vietwarex vietwarefull` |
| `cycle_mode_hotkey` | string | `""` | Phím tắt chuyển vòng chế độ, vd `Control+Alt+v` (bắt buộc có modifier) |

## Dấu và kiểm tra

| Key | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `modern_tone_style` | bool | `true` | `hòa` (mới) thay vì `hoà` (cũ) |
| `free_tone_marking` | bool | `true` | Dấu thanh gõ ở bất kỳ đâu trong từ |
| `auto_correct` | bool | `true` | Core hoàn nguyên âm tiết vô nghĩa (nạp cả word list spell check) |
| `auto_restore` | bool | `true` | Cuối từ khôi phục raw keys cho từ vô nghĩa |
| `abbreviations` | bool | `true` | Giữ đ trong viết tắt: `ddc` → `đc` |

## Hành vi gõ

| Key | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `bypass_url` | bool | `true` | Từ sau `/ : @ . _ ~ = # & % ?` giữ nguyên |
| `w2u` | string | `disabled` | `disabled` `nonstart` `everywhere` — `w` thành `ư` |
| `bracket` | string | `disabled` | `[ ]` thành `ơ ư` (bắt buộc bật khi dùng Microsoft layout) |
| `em_dash` | bool | `false` | `--` → `—` |
| `auto_capitalize` | bool | `false` | Viết hoa sau `. ! ?` Enter |
| `double_space_period` | bool | `false` | Hai space → `. ` (thử nghiệm) |
| `hide_underline` | bool | `false` | Preedit không gạch chân (app hỗ trợ) |

## Macro và từ điển

| Key | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `macro_enabled` | bool | `true` | Bật gõ tắt (`macros.toml`) |
| `capitalize_macro` | bool | `true` | `DD` → expansion viết hoa |
| `macro_in_off_mode` | bool | `false` | Macro vẫn chạy khi tắt tiếng Việt |
| `time_format` | string | `%H:%M` | Format `$TIME` (chrono strftime) |
| `date_format` | string | `%d/%m/%Y` | Format `$DATE` |
| `dictionary_enabled` | bool | `false` | Nạp `dictionary.txt` — từ trong đó không bị auto-restore |

## Click giữa chừng preedit (#14)

| Key | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `click_action` | string | `bamboo` | `bamboo` `keep` `relocate` |
| `preedit_commit_timeout_ms` | số | `800` | Nghỉ lâu hơn này → từ treo tự commit (0 = tắt; không có tác dụng khi `keep`) |

## Chế độ uinput (Smooth/Super Smooth/Select)

| Key | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `backspace_interval_ms` | số | `5` | Khoảng cách giữa các Backspace inject |
| `post_delay_ms` | số | `10` | Nghỉ sau rewrite trước khi gõ tiếp |
| `commit_interval_ms` | số | `20` | ⚠️ Hiện chưa được đọc bởi engine (dead config — chờ quyết định wire-in/xoá) |
| `unicode_channel` | string | `commit` | `commit` = CommitText; `clipboard` = clipboard + Ctrl+V (thay clipboard 600ms) |

## Khác

| Key | Kiểu | Mặc định | Ý nghĩa |
|---|---|---|---|
| `debug_log` | bool | `false` | Ghi quyết định engine vào `~/.cache/ibus-mangrs/debug.log` |
| `app_rules` | bảng `[[app_rules]]` | built-in chrome-family → preedit, sublime_text → surrounding | `pattern` (substring prgname, không phân biệt hoa thường) + `mode` |

Ví dụ một rule:

```toml
[[app_rules]]
pattern = "kate"
mode = "preedit"
```

Ghi chú kỹ thuật: file thiếu key → dùng mặc định; TOML hỏng → fallback an toàn;
`Config::save` giữ nguyên key mà phiên bản này không biết (`src/config.rs::merge`).
File dữ liệu đi kèm: `macros.toml`, `dictionary.txt`, `vn-words.txt` — chi tiết ở
[../08-du-lieu-va-cau-hinh.md](../08-du-lieu-va-cau-hinh.md).

## Đọc tiếp

- [03-su-dung.md](03-su-dung.md) — tác dụng thực tế của từng nhóm tùy chọn.
- [06-xu-ly-su-co.md](06-xu-ly-su-co.md) — khi config có vẻ không ăn.
