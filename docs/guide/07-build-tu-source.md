# 07 — Build từ source (cho developer)

Bản đồ code: [../06-ma-nguon.md](../06-ma-nguon.md). Trang này là các lệnh, tất cả đã
chạy xác minh trên repo.

## Build

```bash
cargo build --release --features gui   # binary + GTK4 settings window
cargo build                            # debug, không GUI — build nhanh khi chỉnh engine
```

Release profile bật `lto`, `codegen-units = 1`, `strip` (`Cargo.toml`).

## Test, lint

```bash
cargo test                             # 207 test: golden 678 ca, FakeClient matrix, fuzz seed 0xC0FFEE
cargo clippy --all-targets             # phải sạch trước commit
cargo fmt                              # chuẩn hóa format
env -u RUSTC_WRAPPER cargo test        # nếu rustup shim báo "unknown proxy name"
```

Test không cần daemon/D-Bus/uinput; test uinput tự skip khi không có thiết bị, ép chạy
bằng `MANGRS_REQUIRE_UINPUT=1`.

## Chạy engine chế độ phát triển

Không cài, dùng component trong repo — **nhớ `-t refresh`** hoặc daemon dùng registry cũ:

```bash
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

Gõ thử qua D-Bus (không cần bàn phím thật; chuỗi phải quote `'như-vậy'`):

```bash
ADDR=$(grep -o 'unix:path=[^,]*' ~/.config/ibus/bus/*)
M() { gdbus call --address "$ADDR" --dest org.freedesktop.IBus.mangrs \
      --object-path /org/freedesktop/IBus/Engine/1 \
      --method "org.freedesktop.IBus.Engine.$1" "${@:2}"; }
M SetCapabilities "uint32 41"
M ProcessKeyEvent "uint32 116" "uint32 0" "uint32 0"   # phím 't'
```

Xem signal: `dbus-monitor --address "$ADDR"`. Log gỡ lỗi: bật `debug_log = true`,
đọc `~/.cache/ibus-mangrs/debug.log`.

## Gõ thật để kiểm chứng

`test/` có công cụ gõ qua kernel vào app đang focus (không chạy trong CI):

```bash
python3 test/typing_tool.py          # gõ chuỗi từ test/keys-test, so test/keys-test.expected
python3 test/perf_tool.py            # đo độ trễ mỗi phím (cần debug_log bật)
```

Chi tiết: `test/README.md`.

## Đóng gói

```bash
makepkg -si                          # Arch — PKGBUILD cài vào /usr/lib/ibus-mangrs + component + rules
./scripts/dist.sh                    # tạo artifact phát hành (dist/)
```

Cấu trúc cài đặt của `scripts/install.sh`: binary
`/usr/lib/ibus-mangrs/ibus-engine-ibus-mangrs`, component
`/usr/share/ibus/component/ibus-mangrs.xml`, word list
`/usr/share/ibus-mangrs/vn-words.txt`, rule `/etc/udev/rules.d/80-uinput.rules`.

## Đọc tiếp

- [../10-mo-rong.md](../10-mo-rong.md) — điểm chạm khi thêm tính năng.
- [../09-kiem-thu.md](../09-kiem-thu.md) — cấu trúc test và cách thêm ca.
