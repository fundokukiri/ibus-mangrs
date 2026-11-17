<p align="center">
  <img src="data/icons/logo.png" alt="Logo MăngRS" width="360">
</p>

# MăngRS

**Tiếng Việt** | [English](README.en.md)

Bộ gõ tiếng Việt cho **IBus**, viết bằng **Rust** trên nền
[`bamboo-core`](https://crates.io/crates/bamboo-core). 9 kiểu gõ, 11 bảng mã, 5 chế độ gõ,
cửa sổ cài đặt GTK4, gõ tắt, bảng chọn emoji tại con trỏ và luật riêng cho từng ứng dụng.

> Phát triển và thử nghiệm trên **Arch Linux** (IBus 1.5.34, GNOME 51, Wayland; X11 đã chạy
> thử). Chỉ cần IBus ≥ 1.5 và GTK4 + libadwaita — nơi nào IBus chạy được thì bộ gõ này chạy
> được. Rất mong phản hồi từ người dùng các distro khác.

Toàn bộ tính năng và hướng dẫn sử dụng nằm ở **[FEATURES-vi.md](docs/FEATURES-vi.md)** — file này chỉ
tập trung vào cài đặt và xử lý sự cố.

## Yêu cầu

| Bản phân phối   | Gói cần có (trình cài một dòng tự lo)                                       |
| --------------- | ---------------------------------------------------------------------------- |
| Arch            | `ibus` `gtk4` `libadwaita` `dbus` + `gcc` `make` `pkgconf`                   |
| Debian / Ubuntu | `ibus` `libgtk-4-dev` `libadwaita-1-dev` `libdbus-1-dev` + build-essential   |
| Fedora          | `ibus` `gtk4-devel` `libadwaita-devel` `dbus-devel` + `gcc` `make`           |

Trình cài nhận diện distro (apt / pacman / dnf / zypper), thiếu gói gì cài nấy, Rust cài qua
rustup. Cần GTK 4.10+ và libadwaita 1.5+ (Ubuntu 22.04 chỉ có GTK 4.6). Chỉ Arch được thử kỹ.
IBus phải là bộ quản lý input của hệ thống (GNOME mặc định đã vậy).

## Cài đặt

### Một dòng lệnh (online)

Tải mã nguồn, biên dịch và cài luôn; thiếu gì trình cài tự cài thêm nên máy chỉ cần có `curl`
(hoặc `wget`) và `sudo`. Quyền `/dev/uinput` được thiết lập qua ACL của phiên đăng nhập (không
cần nhóm), gói chỉ dùng để biên dịch sẽ được gỡ sau khi xong, chạy lại không cài đúp. Thêm
`--demo` để chạy thử toàn bộ các bước mà không đụng gì vào máy:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
# hoặc khi máy không có curl:
wget -qO- https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
```

(Đẩy script từ mạng vào `sh` là chạy code người khác — không yên tâm thì đọc script trước.)

Muốn gỡ sạch:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/uninstall.sh | sh
```

Thêm `PURGE_CONFIG=1` phía trước lệnh để xoá luôn `~/.config/ibus-mangrs`; chạy thường giữ
nguyên cấu hình của bạn. `--demo` cho xem quá trình gỡ mà không xoá gì.

### Từ mã nguồn

```sh
# Arch / Manjaro / EndeavourOS
sudo pacman -S --needed gcc pkgconf ibus dbus gtk4 libadwaita desktop-file-utils

# Debian / Ubuntu / Mint / Pop!_OS
sudo apt install gcc libc6-dev pkg-config ibus libdbus-1-dev libgtk-4-dev \
    libadwaita-1-dev desktop-file-utils

# Fedora / RHEL / CentOS
sudo dnf install gcc pkgconf-pkg-config ibus dbus-devel gtk4-devel \
    libadwaita-devel desktop-file-utils

# openSUSE
sudo zypper install gcc pkg-config ibus dbus-1-devel gtk4-devel \
    libadwaita-devel desktop-file-utils

# Rust (mọi distro)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

cargo build --release --features gui
sudo ./scripts/install.sh
ibus restart
```

Script copy engine vào `/usr/lib/ibus-mangrs/`, đăng ký component cho IBus, cài udev rule và
**gỡ bạn khỏi nhóm `input`** nếu bản cũ đã thêm (hiệu lực từ lần đăng nhập sau). Gỡ:
`sudo ./scripts/uninstall.sh` (giữ nguyên cấu hình). Trên Arch thì gọn hơn: `makepkg -si`.

### Cấp quyền `/dev/uinput` (làm tay)

Bộ gõ nhấn **phím thật** qua `/dev/uinput` cho những app không xoá chữ được qua IBus; thiếu
quyền ghi thì app đó không gõ được dấu. Quyền đến từ ACL của **phiên đăng nhập** (logind),
không phải từ nhóm. Tạo `/etc/udev/rules.d/70-uinput.rules` (mẫu có sẵn trong
`data/rules/70-uinput.rules`):

```
KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", TAG+="uaccess"
```

Sau đó nạp lại rule và đăng nhập lại một lần (ACL chỉ được cấp khi phiên đang hoạt động):

```sh
sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=/dev/uinput
echo uinput | sudo tee /etc/modules-load.d/uinput.conf   # tự nạp mỗi lần khởi động
getfacl -p /dev/uinput            # phải có user:<bạn>:rw-
```

**Nếu bản cũ bảo bạn vào nhóm `input`, hãy ra khỏi nhóm đó** (`sudo gpasswd -d $USER input`,
đăng nhập lại sau đó) — nhóm này đọc được mọi bàn phím của máy. **Nếu còn
`/etc/udev/rules.d/80-uinput.rules` từ bản cũ**, xoá nó đi: udev đọc cả hai file nên bản cũ
vẫn còn hiệu lực. Chi tiết đầy đủ: [SECURITY.md](SECURITY.md).

## Bật bộ gõ

1. `ibus restart` (sau khi cài hoặc nâng cấp binary).
2. **Settings → Keyboard → Input Sources → +** → *Vietnamese* → **MăngRS**
   (hoặc `ibus engine ibus-mangrs`).
3. `Super+Space` đổi giữa các nguồn nhập (phím riêng của GNOME); bánh răng cạnh tên bộ gõ
   mở cửa sổ cài đặt.

## Các tệp cấu hình

```
~/.config/ibus-mangrs/config.toml     # toàn bộ cài đặt, sửa là áp dụng sau khoảng 1 giây
~/.config/ibus-mangrs/macros.toml     # gõ tắt, mỗi dòng key=nội dung, có $TIME / $DATE
~/.config/ibus-mangrs/emoji.toml      # emoji riêng, dạng tên = "ký tự"
~/.config/ibus-mangrs/dictionary.txt  # những từ không bao giờ bị tự sửa
```

## Lỗi thường gặp

- **Không thấy bộ gõ trong danh sách Input Sources**: chạy `ibus write-cache --system` rồi
  `ibus-daemon -drx`; kiểm tra `/usr/share/ibus/component/ibus-mangrs.xml` có tồn tại.
- **Gõ không ra dấu trong vài app** (Sublime, app XWayland): `/dev/uinput` thiếu hoặc không
  ghi được — xem phần cấp quyền ở trên. Kiểm tra ACL bằng `getfacl -p /dev/uinput`.
- **Vừa cài/nâng cấp xong vẫn hành xử như bản cũ**: engine đang chạy vẫn là code cũ —
  `ibus restart` sau mỗi lần thay binary.
- **Đổi chế độ mà chẳng ăn gì**: app đó đang có luật riêng đè lên chế độ chung (xem
  FEATURES.md phần *Luật theo app*).
- **App XIM (JetBrains) hay app X11 khó tính**: thử hai công tắc *Tương thích X11/XIM* trong
  trang *Tinh chỉnh* của GUI.

## Tài liệu khác

- [FEATURES-vi.md](docs/FEATURES-vi.md) — tính năng đầy đủ, 5 chế độ gõ, hướng dẫn sử dụng từng phần.
- [README-en.md](docs/README-en.md) — English readme.
- [SECURITY.md](SECURITY.md) — ranh giới tin cậy, rủi ro còn lại, cách kiểm tra và thu hồi quyền
  (bản tiếng Anh: [docs/SECURITY-en.md](docs/SECURITY-en.md)).
- [ARCHITECTURE.md](ARCHITECTURE.md) — kiến trúc engine (dành cho người sửa code).
- [CHANGELOG.md](CHANGELOG.md) — thay đổi theo từng bản phát hành.
- Phím tắt mở bằng GUI; luật cho Chrome/Firefox trên GNOME Wayland cần GNOME Shell extension
  kèm theo (`data/extension/README.md`).

## Bản quyền

- [**MăngRS**](https://github.com/fundokukiri/ibus-mangrs) — giấy phép [GPL-3.0](LICENSE).
- [**IBus Bamboo:**](https://github.com/BambooEngine/ibus-bamboo) Bản quyền (C) 2018 Luong Thanh Lam, giấy phép [GPL-3.0](https://github.com/bambooengine/ibus-bamboo?tab=GPL-3.0-1-ov-file).
- [**bamboo-core (Rust):**](https://github.com/bamboo-vi/bamboo-core) thư viện thuật toán gõ tiếng Việt viết bằng Rust, giấy phép [MIT](https://github.com/bamboo-vi/bamboo-core?tab=MIT-1-ov-file).
- [**bamboo-core (Go):**](https://github.com/BambooEngine/bamboo-core) Bản quyền (C) 2018 Luong Thanh Lam, giấy phép [MIT](https://github.com/BambooEngine/bamboo-core?tab=MIT-1-ov-file).
