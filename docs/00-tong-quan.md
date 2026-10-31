# 00 — Tổng quan dự án

> Chương này giúp bạn hiểu MăngRS giải quyết vấn đề gì, cấu thành từ những phần nào,
> và triết lý thiết kế chi phối toàn bộ codebase. Đọc xong bạn sẽ có "bức tranh lớn"
> trước khi đi vào từng tầng kỹ thuật.

## MăngRS là gì?

**MăngRS** (`ibus-mangrs`) là một engine input method cho **IBus** trên Linux, cho phép
gõ tiếng Việt bằng các kiểu gõ quen thuộc (Telex, VNI, VIQR, Microsoft layout) trong mọi
ứng dụng có hỗ trợ IBus. Engine viết bằng **Rust**, chạy như một tiến trình riêng do
`ibus-daemon` khởi tạo, và giao tiếp với ứng dụng qua D-Bus.

Bộ não xử lý tiếng Việt — quy tắc ghép phím thành dấu, đặt dấu thanh, kiểm tra tính
hợp lệ của âm tiết — không nằm trong dự án này mà thuộc về crate **`bamboo-core`**
(phiên bản `0.5.3`, khai báo trong `Cargo.toml`). MăngRS là **tầng hệ thống**: nó nhận
sự kiện phím từ IBus, gọi `bamboo-core` để biến đổi, rồi quyết định *cách đưa kết quả
vào ứng dụng* — và đây mới là phần phức tạp thật sự, vì mỗi ứng dụng/toolkit xử lý
text khác nhau.

Vai trò các bên:

| Thành phần | Vai trò | Mã nguồn |
|---|---|---|
| IBus daemon | Điều phối input method hệ thống; khởi tạo engine, routing phím | (hệ thống, không phải code của dự án) |
| **MăngRS** (repo này) | Tầng IBus + state machine hiển thị + transport xóa/ghi text | `src/` |
| **bamboo-core** | Engine tiếng Việt thuần: phím → text, không biết IBus tồn tại | crate `bamboo-core` 0.5.3; submodule `bamboo-core/` pin cùng tag để đọc mã nguồn |
| ibus-bamboo (Go) | Dự án tham chiếu; so sánh hành vi, **không** sao chép code (license khác) | `analysis/ibus-bamboo/` (bản vendor để đối chiếu) |

Quan hệ với dự án gốc: `analysis/ibus-bamboo/` chứa bản vendor của
[ibus-bamboo](https://github.com/BambooEngine/ibus-bamboo) (Go, GPL) dùng làm *behavioral
reference* — ví dụ bảng chế độ gõ được đối chiếu trong `audit/typing-modes-report.md`
§3. Code của MăngRS không phái sinh từ nó; license của MăngRS là MIT (`LICENSE`,
`Cargo.toml` dòng `license = "MIT"`).

## Triết lý thiết kế

Bốn nguyên tắc nhìn thấy được xuyên suốt mã nguồn:

1. **Engine tách khỏi protocol.** `src/engine.rs` biến phím thành danh sách
   `Action` trừu tượng (`Commit`, `DeleteBefore`, `UpdatePreedit`, `Forward`…) mà
   *không import bất kỳ kiểu D-Bus nào*. Chỉ `src/server.rs` biết IBus. Nhờ vậy toàn bộ
   engine chạy được trong unit test không cần daemon
   (xem `src/golden.rs::typed_word`).
2. **Một cửa duy nhất chạm bamboo-core.** Chỉ `src/bamboo_wrapper.rs` gọi crate. Dữ liệu
   do bamboo trả về bị đối xử thiếu tin cậy có chủ đích: wrapper **tự tính delta** dựa trên
   `last_output` của chính nó thay vì tin `EditDelta` của bamboo
   (`src/bamboo_wrapper.rs::delta_from_last`, lý do ghi trong comment tại đó).
3. **An toàn xóa text là bất biến.** Gửi `DeleteSurroundingText` nhầm sang client mà
   compositor chưa từng thấy buffer từng gây crash GNOME Shell (logout). Mọi đường xóa
   đều qua gate `deletes_allowed()` / `surrounding_data_ok()`
   (`src/engine.rs`, phần "Engine Invariants" trong `ARCHITECTURE.md`).
4. **Hành vi được chốt bằng test.** 678 ca golden (phím → text cuối trong buffer giả lập
   ứng dụng) + ma trận client lỗi + fuzz có seed trong `src/golden.rs`.

## Bức tranh lớn một trang

```mermaid
flowchart LR
    U[Người dùng gõ phím] --> APP[Ứng dụng\n(GTK/Qt/Chromium...)]
    APP -- "xlib/wl_text_input" --> MUTTER[Compositor / ibus-daemon\nmột private D-Bus bus]
    MUTTER -- "ProcessKeyEvent(keyval, keycode, state)" --> ENG[ibEngine-ibus-mangrs\ntiến trình engine]
    subgraph Engine nội bộ
        ENG --> SV[server.rs\nD-Bus, transports]
        SV --> E[engine.rs\nstate machine, modes]
        E --> W[bamboo_wrapper.rs\nnơi duy nhất chạm core]
        W --> BC[(bamboo-core\nphím → tiếng Việt)]
    end
    ENG -- "CommitText / UpdatePreeditText /\nDeleteSurroundingText" --> MUTTER
    ENG -. "phím Backspace thật" .-> UINPUT[/dev/uinput/] -.-> APP
    MUTTER -- key events --> APP
```

Sơ đồ trên tóm tắt vòng đời một phím: ứng dụng (qua compositor trên Wayland) gửi
`ProcessKeyEvent` tới engine trên private bus của ibus-daemon; engine gọi core, quyết định
hành động, và trả kết quả bằng các signal IBus hoặc bơm phím thật qua `/dev/uinput`.

## Số liệu nhanh (cuối đợt audit)

- 207 unit test, trong đó 678 ca golden qua `src/golden.rs`.
- 8 chế độ hiển thị, 4 kiểu gõ, 11 bảng mã (`src/engine.rs::Mode`,
  `src/config.rs::ModeConfig`, `src/bamboo_wrapper.rs::Charset`).
- Không `unwrap`/`expect` trên trạng thái engine trong đường đi phím
  (mutex engine là poison-tolerant qua `server.rs::lock_engine`).

## Đọc tiếp

- [01-kien-thuc-nen-tang.md](01-kien-thuc-nen-tang.md) — IBus và bộ gõ tiếng Việt hoạt động thế nào ở tầng hệ thống.
- [02-kien-truc-bo-go.md](02-kien-truc-bo-go.md) — kiến trúc chi tiết tầng MăngRS.
- [06-ma-nguon.md](06-ma-nguon.md) — bản đồ mã nguồn và thứ tự đọc đề xuất.
