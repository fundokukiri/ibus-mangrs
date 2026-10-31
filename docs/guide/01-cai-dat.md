# 01 — Cài đặt

MăngRS là engine IBus: một binary + một file đăng ký (component XML). Bạn có thể cài từ
gói hoặc tự build. Sau cài đặt luôn cần `ibus restart` một lần.

## Yêu cầu hệ thống

| Thành phần | Ghi chú |
|---|---|
| Linux + IBus ≥ 1.5 | GNOME thường có sẵn; đã kiểm trên IBus 1.5.34 |
| `gtk4` | cho cửa sổ cài đặt (build cần `libgtk-4-dev`/`gtk4-devel`) |
| Rust toolchain | chỉ khi build từ source |
| `/dev/uinput` | cho chế độ gõ bằng phím thật (Smooth/Super Smooth/Select) và sửa lỗi xóa ở một số app; không bắt buộc |

Dự án được phát triển và kiểm trên **Arch Linux (GNOME, Wayland + X11)**; các distro
khác chưa được kiểm (`README.md` — Platform status).

## Cài từ gói (Arch)

```bash
makepkg -si
```

`PKGBUILD` cài binary vào `/usr/lib/ibus-mangrs/`, component vào
`/usr/share/ibus/component/`, udev rule và `vn-words.txt`.

## Cài từ source

```bash
git clone https://github.com/fundokukiri/ibus-mangrs && cd ibus-mangrs
cargo build --release --features gui
sudo ./scripts/install.sh        # cài binary + component + udev rule + thêm nhóm input
ibus restart                     # bắt buộc sau khi cài/nâng cấp binary
```

Script cài đặt (`scripts/install.sh`) làm những việc sau — bạn có thể làm tay nếu muốn:

```bash
install -Dm755 target/release/ibus-engine-ibus-mangrs /usr/lib/ibus-mangrs/
install -Dm644 data/ibus-mangrs.xml   /usr/share/ibus/component/ibus-mangrs.xml
install -Dm644 data/vn-words.txt      /usr/share/ibus-mangrs/vn-words.txt
ibus write-cache --system             # làm mới registry của daemon
install -Dm644 data/80-uinput.rules   /etc/udev/rules.d/80-uinput.rules
sudo usermod -aG input $USER          # quyền /dev/uinput — hiệu lực sau đăng nhập lại
```

**Kiểm tra bước cài thành công:**

```bash
ls /usr/share/ibus/component/ibus-mangrs.xml    # tồn tại
ls -l /dev/uinput                                # crw-rw---- ... input
```

Một lệnh online (tải source, build, cài — xem kỹ script trước khi pipe):

```bash
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/dev/scripts/install-online.sh | sh
```

## Gỡ cài đặt

```bash
sudo ./scripts/uninstall.sh
ibus restart
```

Config `~/.config/ibus-mangrs/` được giữ lại; nhắc input source trong Settings nếu bạn
không dùng nữa.

## Thư viện hệ thống theo distro

| Distro | Runtime | Build |
|---|---|---|
| Arch | `ibus` `gtk4` | `rust` |
| Debian/Ubuntu | `ibus` `libgtk-4-1` (tên gói runtime chưa kiểm trên distro thật) | `libgtk-4-dev build-essential` |
| Fedora | `ibus` `gtk4` | `gtk4-devel rust cargo` |

⚠️ Chưa xác minh: chỉ dòng Arch có căn cứ từ repo (`PKGBUILD`); hai dòng còn lại là tên
gói chuẩn của distro, chưa được thử.

## Đọc tiếp

- [02-thiet-lap-ibus.md](02-thiet-lap-ibus.md) — thêm MăngRS vào input sources.
- [07-build-tu-source.md](07-build-tu-source.md) — build debug, test, chạy chế độ dev.
