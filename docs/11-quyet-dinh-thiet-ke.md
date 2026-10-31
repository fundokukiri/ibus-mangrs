# 11 — Quyết định thiết kế (ADR) và hạn chế đã biết

> Mỗi mục: vấn đề → lựa chọn → đánh đổi. Format rút gọn kiểu ADR; các quyết định sắp
> theo tầm ảnh hưởng. Cuối chương là danh sách hạn chế đã biết và việc còn dở.

## ADR-01: Không tin `EditDelta` của bamboo-core — tự tính delta

- **Vấn đề**: `EditDelta` của core tính theo `prev_preedit` nội bộ, trường này không
  được refresh sau khi app/xóa text — phím sau Backspace report xóa một ký tự đã biến
  mất (`tie` → BS → `n` ra `tn`).
- **Lựa chọn**: wrapper giữ `last_output` (text *tin là đang trên màn hình*) và tự diff
  (`src/bamboo_wrapper.rs::delta_from_last`); `process_key_edit` được gọi nhưng giá trị
  trả về bị bỏ (`let _ =`).
- **Đánh đổi**: một phép diff chuỗi mỗi phím; đổi lại delta luôn đúng theo *thứ mà ứng
  dụng thật sự giữ* — kể cả sau rebuild/repair (`src/engine.rs::sync_from_surrounding`
  chỉ hoạt động được nhờ điều này).

## ADR-02: Backspace = replay theo unikey, không dùng `remove_last_output_char`

- **Vấn đề**: xóa ký tự để transform stack stale → trạng thái "nhiễm độc" tái xuất bất
  định (`toool`+BS giữ nguyên `tool`, `mười` thành `mừi`).
- **Lựa chọn**: pop phím raw cuối + `engine.reset()` + replay các phím còn lại
  (`src/bamboo_wrapper.rs::backspace`).
- **Đánh đổi**: replay O(k) mỗi backspace (k = độ dài từ, thực tế <10) thay vì O(1);
  đổi lại kết quả deterministic và ngữ nghĩa đúng chuẩn unikey (backspace hoàn tác
  *phím* — gõ lại `f` trả đủ dấu). Guard kép cho poison còn sót: output dài hơn raw →
  rebuild (`process_char`), và `undo_in_word` cấm auto-restore phá từ đã hoàn tác dấu.

## ADR-03: Terminator chặn ở wrapper, theo method

- **Vấn đề**: bamboo-core giữ dấu câu/số trong composition (và xóa cả từ khi gặp số ở
  Telex) — replay những hành vi đó lên app thật là mất chữ.
- **Lựa chọn**: `src/bamboo_wrapper.rs::process_char` chặn terminator **trước** khi tới
  core; bộ terminator có exemption theo method (`viqr_mark_key`, `ms_bracket_key`,
  `bracket_key`, `digit_key`).
- **Đánh đổi**: bảng terminator phải biết method (bug VIQR/MsLayout lịch sử nằm đúng
  đây); đổi lại mọi từ được kết thúc có chủ đích, bamboo không bao giờ giữ text phải
  replay.

## ADR-04: Protocol delete chỉ khi có ST report; repair cấm trên relay

- **Vấn đề**: `DeleteSurroundingText` tới client mà compositor chưa thấy buffer →
  mutter segfault → logout (`ARCHITECTURE.md`, crash 02:37).
- **Lựa chọn**: gate `deletes_allowed()` = ST report thật (không tin capability bit —
  daemon flip nó tùy hứng) HOẶC uinput khả dụng; repair-delete thêm cấm relay
  (`src/engine.rs::repair_delete_forbidden`); clamp mọi số xóa theo report
  (`clamp_delete`).
- **Đánh đổi**: một số client sẽ nhận raw-forward thay vì dấu (đã log); đổi lại không
  thể crash session.

## ADR-05: Engine protocol-free, một file chạm D-Bus

- **Vấn đề**: test engine cần daemon; logic vướng wire format.
- **Lựa chọn**: `Action` trung gian; chỉ `server.rs` biết D-Bus.
- **Đánh đổi**: một lớp translation (emit_with) — nhưng 207 test offline và khả năng
  fork sang backend khác (Fcitx5...) là giá đủ lớn.

## ADR-06: Preedit là mặc định cài đặt

- Lịch sử: non-preedit default trước Round 5. Preedit không cần xóa text nên an toàn
  nhất với client lạ (mặc định config `src/config.rs` + `docs/open-questions.md` mục
  settled). Đánh đổi: một số app render preedit xấu (Sublime không render, vte đảo màu)
  — giải pháp là App Rules.

## ADR-07: Bằng chứng giao delete (TM-001) và heal tự động

- **Vấn đề**: client nuốt delete không nằm trong danh sách tên → hỏng mãi không ai phát
  hiện (report kết thúc bằng tail nên classify là "khỏe").
- **Lựa chọn**: mỗi rewrite DeleteBefore kèm insert arm một proof (prefix dài bao nhiêu
  nếu delivered / nếu swallowed); report kế tiếp settle; swallow được chứng minh →
  `delete_dropped` → không uinput thì các từ sau chuyển Preedit
  (`src/engine.rs::awaiting_delete_proof`, `effective_mode`).
- **Đánh đổi**: từ hiện tại có thể đã hỏng (không thể cứu); pure-deletion không arm
  (stale report gây dương tính giả).

## ADR-08: uinput thay XTest; commit defer cho echo

- **Vấn đề**: client nuốt protocol delete cần "phím thật".
- **Lựa chọn**: `/dev/uinput` (kernel API, không cần X11 display — hoạt động cả X11 lẫn
  XWayland có quyền); commit sau injection defer tới khi đủ echo hoặc 150ms
  (`Engine::ECHO_STALE`); loop tick tự flush (TM-002).
- **Đánh đổi**: cần udev rule + group; latency có pacing 5ms/10ms; trên GNOME Wayland
  inject không tới được app relayed → mode fallback preedit (đo 31/10, `docs/modes.md`).

## ADR-09: Đơn luồng, tick theo deadline — không async

- **Lựa chọn**: message filter + `conn.iter(timeout)` với `ms_until_tick`.
- **Đánh đổi**: pacing uinput **chặn** thread D-Bus (sleep trong emit) — chấp nhận vì
  pacing vốn là mục đích; đổi lại thứ tự sự kiện tuyệt đối, không lock churn.

## ADR-10: Danh sách từ = advisory; auto-correct = phonotactics

- Danh sách 38 350 từ không tham gia quyết định (owner decision, `docs/gap-analysis.md`);
  hook `known_word()` sẵn cho tương lai.

## Hạn chế đã biết & việc còn dở

1. **uinput modes không hoạt động trên GNOME Wayland relayed** — injection bị rơi ở
   daemon/mutter (đo 31/10: 52 lần inject, echo về đủ, app không thấy gì); fallback
   preedit là con đường chính thức. Wayland thật sự cần IMv2 (backlog TODO).
2. **commit_interval_ms là dead config** — GUI chỉnh nhưng không đọc; wire-in hay xoá
   chờ owner.
3. **Spell check word list lookup-only** — `known_word()` chưa có consumer.
4. **App Rules theo tiến trình** — không theo website/tab; client không gửi FocusInId
   (Sublime/XIM) không match được.
5. **Chỉ test trên Arch/GNOME** — distro/DE khác chưa kiểm chứng (README ghi rõ).
6. **Log verify chứa `brief()` 24 ký tự của buffer app** — mâu thuẫn nhỏ với lời hứa
   "không log nội dung gõ"; chờ owner quyết.
7. **MsLayout giữ case thường cho ký tự chèn** (`TI3NG8` → `TIếNG`) — upstream core.
8. **Forward-as-commit / XTest / per-website rules** — chưa có (đối chiếu ibus-bamboo
   Go trong `audit/typing-modes-report.md` §3).

## Đọc tiếp

- [02-kien-truc-bo-go.md](02-kien-truc-bo-go.md) — bối cảnh kỹ thuật của từng ADR.
- [12-faq.md](12-faq.md) — câu hỏi thường gặp cùng chủ đề.
