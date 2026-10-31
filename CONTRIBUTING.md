# Contributing to MăngRS

Cảm ơn bạn quan tâm tới dự án. Trang này tóm tắt cách làm việc với codebase; chi tiết
kiến trúc nằm ở [docs/README.md](docs/README.md).

## Build & test

```bash
cargo build --release --features gui   # binary engine + GTK4 settings window
cargo test                             # 207 test, gồm 678 ca golden — không cần IBus daemon
cargo clippy --all-targets             # phải sạch
cargo fmt                              # trước khi commit
```

Nếu máy bạn dùng rustup shim bị lỗi `unknown proxy name`, chạy kèm
`env -u RUSTC_WRAPPER cargo ...`.

Chạy engine trực tiếp mà không cài (daemon cần `-t refresh` hoặc sẽ phục vụ registry cũ):

```bash
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

## Quy ước code

- Comment giải thích *vì sao*, không phải *làm gì*; một dòng là chủ yếu.
- Không `unwrap()`/`expect()` trong engine hay đường xử lý phím (test và unreachable
  mới được, kèm comment); không panic trong engine — crash làm chết gõ toàn desktop.
- Latency quan trọng: không blocking I/O hay cấp phát nặng trong key path.
- `src/engine.rs` không được import kiểu D-Bus; chỉ `server.rs` nói chuyện với IBus;
  chỉ `bamboo_wrapper.rs` chạm `bamboo-core`.
- Không có emoji trong các file `.md`.

## Commit & Pull Request

- Conventional Commits, subject ≤ 72 ký tự, một logic change mỗi commit; mỗi commit
  phải build và `cargo test` xanh.
- Body commit nói lỗi gì cho người dùng và bản sửa khắc phục thế nào, kèm số đo nếu có.
- PR một chủ đề, mô tả < 800 ký tự, nói rõ *chưa* test ở đâu (desktop, X11/Wayland).
- Khi thêm/sửa hành vi gõ: kèm test fail-trước-sau (red→green), golden không được lùi.
- Chỉ push lên nhánh được maintainer đồng ý; không commit các file nội bộ
  (`TODO.md`, `CHANGELOG.md` mục nội bộ, `DEVLOG.md`, `AGENTS.md`).

## Báo lỗi

Bật **Debug Logging** (GUI General tab hoặc `debug_log = true`), tái hiện lỗi, gửi
`~/.cache/ibus-mangrs/debug.log` — log ghi quyết định của engine, không ghi nội dung
bạn gõ.
