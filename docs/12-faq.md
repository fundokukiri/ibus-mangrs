# 12 — FAQ kỹ thuật

> Trả lời từ mã nguồn; câu nào không xác minh được từ code thì nói rõ. Nhóm: Khái niệm,
> Kiến trúc, Hành vi gõ, Kỹ thuật, Phát triển, Hạn chế.

## Khái niệm

**1. Bộ gõ IBus khác gì bộ gõ trên Windows (Unikey/EVKey)?**
Unikey hook ở mức ứng dụng/IME framework riêng của Windows; ở Linux, IBus là tầng trung
gian chuẩn: daemon route phím, engine nhận qua D-Bus. MăngRS phải tự lo *cách đưa text
vào app* (preedit/commit/surrounding/inject) — tầng Unikey không cần nghĩ tới. Chuỗi xử
lý tiếng Việt thì tương đồng (chuẩn unikey cho Backspace được cài lại tường minh trong
`src/bamboo_wrapper.rs::backspace`).

**2. Vì sao cần core riêng (`bamboo-core`) tách khỏi tầng IBus?**
Vì hai mối quan hệ khác hẳn: core thuần text (phím → từ, O(µs), không side effect) và
tầng IBus phải đối thoại D-Bus với từng loại app. Tách cho phép core test không cần
hệ thống (crate doc `bamboo-core/src/lib.rs`) và wrapper thay core mà engine không đổi.

**3. Preedit là gì, khác commit ở đâu?**
Preedit là dải text *đang soạn* app render giúp (thường gạch chân), chưa vào buffer;
commit mới là text thật. Engine chọn: preedit (`Action::UpdatePreedit`) hay commit ngay
từng phím (`Action::Commit`) — tùy mode (`src/engine.rs::on_edit`).

**4. "Surrounding text" là gì, dùng để làm gì?**
Client báo cho engine biết buffer `(text, cursor, anchor)` (`src/server.rs`, method
`SetSurroundingText`). Engine đối chiếu với `tail` — text nó tin là đã commit — để phát
hiện app nuốt delete hoặc tự sửa text (`src/engine.rs::verify_surrounding`).

## Kiến trúc

**5. Vì sao engine tách protocol khỏi logic?**
`engine.rs` không import kiểu D-Bus nào — 207 test chạy offline, hành vi chốt bằng
golden (`src/golden.rs`). Chi tiết ở ADR-05 (chương 11).

**6. Core có phụ thuộc IBus không? Có dùng cho nền tảng khác không?**
Không — `bamboo-core` là thư viện text thuần (có cả `ffi`/`wasm` module). Tầng MăngRS
gắn IBus, nhưng `Action` trung gian cho phép backend khác tái dùng engine.

**7. Engine chạy mấy thread? State có thread-safe không?**
Một thread chính: message filter xử lý phím; main loop chỉ `tick` deadline. State nằm
trong `Arc<Mutex<Engine>>` (`src/server.rs::SharedEngine`) — lock poison-tolerant
(`lock_engine`), GUI process riêng nên không cạnh tranh trực tiếp.

**8. Tại sao method reply phải trả trong message filter?**
libdbus tự đáp UnknownMethod nếu filter không claim message; trả muộn qua MsgHandler là
quá muộn (`src/server.rs::handle` comment; `ARCHITECTURE.md` "IBus Protocol Facts").

## Hành vi gõ

**9. Vì sao có app gõ dấu bị nháy rồi lùi ("accent flashes and reverts")?**
Commit đi D-Bus nhanh hơn phím Backspace inject đi compositor; echo Backspace về sau
thì xóa mất chữ vừa commit. Engine defer commit tới đủ echo
(`src/server.rs::emit_with`, arm Commit) — echo mất thì stale 150ms flush
(`Engine::ECHO_STALE`, tick — TM-002).

**10. Vì sao một số app phải đổi chế độ?**
App khác nhau khác nhau về delete/ST: Chromium nuốt `DeleteSurroundingText`, mutter
relay cấm repair-delete (crash), Sublime không render preedit. Engine tự chọn theo
heuristic/App Rules (`src/engine.rs::effective_mode`); bảng mặc định trong
`src/config.rs::default_app_rules`.

**11. Gõ từ tiếng Anh thì sao?**
Auto-correct của core revert âm tiết vô nghĩa về raw; cuối từ nếu word != raw và vô
nghĩa thì auto-restore commit raw (`src/engine.rs::on_finish`). Chữ sau `/ : @ . _ ...`
giữ raw luôn (bypass URL, `URL_CONTEXT_CHARS`).

**12. Vì sao `tooi'` gõ được nhưng `don't` không thành `doń`?**
Rule chỉ áp khi ngữ cảnh hợp lệ (nguyên âm đích) — bamboo-core tự giữ literal khi
không áp được; chữ số/punctuation là terminator theo method
(`src/bamboo_wrapper.rs::process_char`).

**13. Gõ nhầm dấu muốn gỡ thì làm sao?**
Gõ lại chính phím dấu (chuẩn unikey): `tô` + `o` → `too`; wrapper pop phím raw cuối khi
Backspace, và lock `undo_in_word` để auto-restore không "sửa" từ
(`src/bamboo_wrapper.rs`, `src/engine.rs::on_finish`).

**14. Viết hoa có ảnh hưởng không?**
Không — keysym giữ case (`src/keymap.rs`), golden có cả biến thể HOA. Auto-capitalize
(mặc định tắt) viết hoa chữ đầu câu bằng cách đưa chữ hoa vào composition
(`src/engine.rs::process_char`).

## Kỹ thuật

**15. Mỗi phím tốn bao nhiêu công?**
Core tuyên bố sub-µs (doc crate). Wrapper thêm một diff chuỗi + vài String clone ngắn;
tầng server từng clone cả buffer ST mỗi phím — đã giảm còn một clone
(`src/engine.rs::set_surrounding`); config không còn được đọc mỗi phím (cache hotkey,
`maybe_reload_config` mtime-gated).

**16. Có cấp phát bộ nhớ nhiều không?**
Hot path cấp phát vài `String`/`Vec` nhỏ mỗi phím (output, delta, action list) — không
zero-alloc như core hứa, nhưng bounded; word list 38k từ chỉ nạp khi Spell Check bật và
reload theo mtime.

**17. Engine crash thì sao?**
Mutex poison-tolerant (`src/server.rs::lock_engine`); engine không panic trên đường
phím (không unwrap/expect trên state); một panic chỉ làm state nửa vời chứ không chết
session. SIGTERM/SIGINT exit sạch nhả bus name (`src/main.rs`).

**18. Có bao giờ gửi protocol delete mù quáng không?**
Không — hai gate độc lập: ST report thật hoặc uinput; repair cấm trên relay; số xóa
clamp theo report; delete kèm insert còn có proof phát hiện bị nuốt
(`src/engine.rs::tracked_delete`).

**19. Làm sao gỡ lỗi khi gõ sai ở app cụ thể?**
`debug_log = true`, tái hiện, đọc `~/.cache/ibus-mangrs/debug.log` (ms timestamp,
khớp journalctl; ghi quyết định, gần như không ghi nội dung gõ) — `src/debug_log.rs`.

## Phát triển

**20. Muốn thêm kiểu gõ thì bắt đầu từ đâu?**
Chương 10 §"Thêm một kiểu gõ mới" — tóm lại: preset ở core (nếu có sẵn), `Method` +
`MethodConfig`, exemption terminator, `is_late_mark_key`, bảng golden mới.

**21. Chạy test thế nào?**
`cargo test` (thêm `env -u RUSTC_WRAPPER` nếu rustup shim lỗi trên máy của bạn); chi
tiết tầng test ở chương 09. Fuzz seed 0xC0FFEE replay được.

**22. Đọc code nên đi theo thứ tự nào?**
Chương 06 §"Thứ tự đọc đề xuất": golden.rs → engine.rs → bamboo_wrapper.rs → server.rs.

**23. Commit theo quy ước gì?**
Conventional Commits, subject ≤72 ký tự, một logic change/commit, mỗi commit build + test
xanh (`AGENTS.md` — internal). Xem `CONTRIBUTING.md`.

## Hạn chế

**24. Wayland có hỗ trợ đầy đủ không?**
Preedit/NonPreedit/Surrounding hoạt động trên Wayland (kể cả mutter relayed); ba mode
uinput thì không — injection bị rơi ở relay, engine tự fallback preedit
(`src/engine.rs::effective_mode`; đo 31/10 trong `docs/modes.md`).

**25. Chưa hỗ trợ gì?**
Forward-as-commit/XTest mode (không có, xem đối chiếu Go ở chương 03/11), per-website
App Rules, candidate window, `commit_interval_ms` chưa wired, word list advisory-only —
danh sách đầy đủ ở chương 11 "Hạn chế đã biết".

**26. Vì sao LibreOffice/JetBrains/Sublime cần chú ý riêng?**
Là các client không gửi FocusInId hoặc xử lý IM khác chuẩn GTK — engine phân loại bằng
heuristic client-None (`src/engine.rs::relay_context`), hành vi từng app trong
`docs/guide/05-tuong-thich-ung-dung.md` và ma trận thủ công `audit/typing-modes-report.md` §11.

## Đọc tiếp

- [11-quyet-dinh-thiet-ke.md](11-quyet-dinh-thiet-ke.md) — bối cảnh của các câu 5, 15, 18, 25.
- [13-thuat-ngu.md](13-thuat-ngu.md) — bảng thuật ngữ.
