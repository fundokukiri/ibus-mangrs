# Tài liệu kỹ thuật MăngRS

Bộ tài liệu giải thích **bộ gõ hoạt động bên trong như thế nào** — từ lúc nhấn một phím
đến lúc chữ tiếng Việt xuất hiện trên màn hình. Viết cho lập trình viên muốn đóng góp/
fork và người tò mò; không phải hướng dẫn sử dụng (xem `docs/guide/`).

| File | Nội dung |
|---|---|
| [00-tong-quan.md](00-tong-quan.md) | Dự án là gì, quan hệ với bamboo-core và ibus-bamboo, triết lý thiết kế, bức tranh lớn |
| [01-kien-thuc-nen-tang.md](01-kien-thuc-nen-tang.md) | IBus/D-Bus/preedit/commit/ST; đặc trưng bộ gõ tiếng Việt; vì sao khó |
| [02-kien-truc-bo-go.md](02-kien-truc-bo-go.md) | Vòng đời engine, luồng một phím (sequence diagram), 8 chế độ, transports, trạng thái |
| [03-kien-truc-core.md](03-kien-truc-core.md) | bamboo-core: module, transformation, DFA, orthography, encoder; dùng/bỏ API nào |
| [04-luong-xu-ly-chi-tiet.md](04-luong-xu-ly-chi-tiet.md) | Bảng trạng thái từng bước: "viet"+j, gõ dấu tự do, hoàn tác dấu, tiếng Anh, Backspace… |
| [05-tinh-nang.md](05-tinh-nang.md) | Mọi tính năng có thật: cơ chế nội bộ + file code + hạn chế |
| [06-ma-nguon.md](06-ma-nguon.md) | Cây thư mục có chú thích, dependency graph, thứ tự đọc |
| [07-thu-vien.md](07-thu-vien.md) | Dependency bảng tổng hợp: phiên bản, license, vì sao chọn |
| [08-du-lieu-va-cau-hinh.md](08-du-lieu-va-cau-hinh.md) | config.toml/macros/dictionary/word list/component XML: schema + luồng nạp |
| [09-kiem-thu.md](09-kiem-thu.md) | Golden 678 ca, FakeClient/fault matrix, fuzz seed 0xC0FFEE, cách thêm ca |
| [10-mo-rong.md](10-mo-rong.md) | Thêm kiểu gõ/bảng mã/tính năng/chế độ/transport: điểm chạm code |
| [11-quyet-dinh-thiet-ke.md](11-quyet-dinh-thiet-ke.md) | ADR-01..10 + hạn chế đã biết & việc còn dở |
| [12-faq.md](12-faq.md) | 26 câu hỏi thường gặp theo nhóm |
| [13-thuat-ngu.md](13-thuat-ngu.md) | Thuật ngữ Việt–Anh + tài liệu tham khảo |

## Tài liệu cùng repo

- **Hướng dẫn sử dụng & cài đặt**: [guide/](guide/README.md) — cài đặt, thiết lập IBus,
  sử dụng, cấu hình, tương thích ứng dụng, xử lý sự cố, build từ source.
- `ARCHITECTURE.md` (repo root) — protocol facts đã kiểm chứng trên dây + engine
  invariants; tham chiếu xuyên suốt bộ docs này.
- `audit/typing-modes-report.md` — báo cáo audit chế độ gõ (mode map, findings TM-*).
- `CHANGELOG.md` — lịch sử thay đổi.
