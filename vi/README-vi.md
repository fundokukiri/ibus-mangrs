# MăngRS

**Tiếng Việt** | [English](README.md)

Bộ gõ tiếng Việt cho **IBus**, viết bằng **Rust** trên nền
[`bamboo-core`](https://crates.io/crates/bamboo-core). 9 kiểu gõ, 9 chế độ gõ, cửa sổ cài đặt
GTK4, gõ tắt, bảng chọn emoji ngay tại con trỏ và luật riêng cho từng ứng dụng.

> Phát triển và thử nghiệm trên **Arch Linux** (IBus 1.5.34, GNOME 51, Wayland; X11 đã chạy
> thử). Chỉ cần IBus ≥ 1.5 và GTK4 + libadwaita — nơi nào IBus chạy được thì bộ gõ này chạy
> được. Rất mong nhận được phản hồi từ người dùng các distro khác.

## Tính năng

- **9 kiểu gõ sẵn** — Telex (`tieengs` → `tiếng`), Telex 2, Telex W, Telex + VNI,
  Telex + VNI + VIQR, VNI (`tie6ng1`), VNI cho bàn phím AZERTY, VIQR và kiểu Microsoft
  (`ti3ng8` → `tiếng`) — kèm **11 bảng mã** xuất ra (Unicode, VNI, VIQR, TCVN3, ...).
- **9 chế độ gõ** (bảng dưới), đặt riêng cho từng ứng dụng qua **Luật theo app**.
- **Tùy chọn gõ linh hoạt**: dấu kiểu mới `hòa` hay kiểu cũ `hoà`, gõ dấu tự do
  (`hoangf` → `hoàng`), tự sửa âm tiết sai, tự buông từ tiếng nước ngoài, tự nhả tay khi gõ
  URL/email/đường dẫn, `w` → `ư`, `--` → `—`, tự viết hoa đầu câu, hai dấu cách thành dấu chấm.
- **Ba kiểu Backspace xoá dấu**: xoá dấu trước (`hạ` → `ha`), xoá nguyên ký tự màn hình
  (`việt` → `việ`), hay kiểu Unikey lùi từng phím (`hạ` → `hã` → `hà` → ...).
- **Chữ hiện dần từng phím ở chế độ Flow/Turbo** (bật sẵn): gõ `ee` thấy ngay `ê` thay vì đợi
  hết từ; click chuột hay đổi cửa sổ cũng không mất chữ đang gõ dở (`ddc` vẫn ra `đc`).
- **Gõ tắt** có `$TIME` / `$DATE` và viết hoa thông minh; **từ điển cá nhân** chứa những từ
  không bao giờ bị tự sửa.
- **Emoji**: gõ `:tên:` ra emoji (mặc định tắt), hoặc bấm phím tắt mở bảng chọn ngay tại con
  trỏ — 1.940 emoji tìm được bằng cả tiếng Việt lẫn tiếng Anh; thêm bộ riêng dạng
  `tên = "ký tự"` trong `emoji.toml`.
- **Menu trên panel sửa được** (thư mục, chức năng, nhóm chọn, đường phân cách) và 14 phím
  tắt tự gán, cộng 4 phím tắt có sẵn.
- **Cửa sổ cài đặt GTK4 + libadwaita** (11 trang): sửa gì cũng áp dụng sau khoảng 1 giây, và
  toàn bộ thiết lập sao lưu ra một tệp JSON duy nhất.

## Các chế độ gõ

| Chế độ             | Cách hoạt động                                                  | Hợp với                                       |
| ------------------ | ---------------------------------------------------------------- | --------------------------------------------- |
| Preedit (mặc định) | chữ hiện ngay tại con trỏ có gạch chân, hết từ thì vào văn bản   | dùng ở đâu cũng ổn                            |
| Non-preedit        | phím nào vào ngay phím đó, có dấu thì xoá đuôi viết lại          | Firefox, app GTK                              |
| Surrounding text   | như Non-preedit, thêm bước đối chiếu với chữ app báo về          | Sublime Text                                  |
| Flow (uinput)      | gửi phím thô khi gõ, hết từ dùng Backspace thật xoá đuôi         | terminal, app khó tính (X11)                  |
| Turbo (uinput)     | như Flow nhưng bỏ bước kiểm tra an toàn                          | gõ nhanh (X11)                                |
| Select (uinput)    | Shift+← bôi đen đuôi, Delete xoá, từ vào văn bản                 | app xử lý Backspace lỗi (X11)                 |
| Forward Backspace  | xoá đuôi bằng phím Backspace gửi qua IBus                        | Chrome, Electron, GNOME Console, Sublime Text |
| Forward Selection  | bôi đen đuôi bằng Shift+← rồi đè chữ mới lên                     | app X11/XIM (JetBrains)                       |
| TẮT                | tắt tiếng Việt, gõ tắt vẫn chạy                                  | gõ lẫn Việt – Anh                             |

Đổi chế độ ở menu bộ gõ, cửa sổ cài đặt, phím tắt, hoặc theo từng app qua **Luật theo app**.
Ba chế độ uinput cần `/dev/uinput`; nơi giả lập phím không với tới được app thì chúng tự rơi
về Preedit (trên GNOME Wayland là vậy) — nên chúng chủ yếu dành cho phiên X11.

## Yêu cầu

| Bản phân phối  | Gói cần có (trình cài một dòng tự lo)                          |
| -------------- | --------------------------------------------------------------- |
| Arch           | `ibus` `gtk4` `libadwaita` `dbus` + `gcc` `make` `pkgconf`      |
| Debian / Ubuntu| `ibus` `libgtk-4-dev` `libadwaita-1-dev` `libdbus-1-dev` + build-essential |
| Fedora         | `ibus` `gtk4-devel` `libadwaita-devel` `dbus-devel` + `gcc` `make` |

Trình cài nhận diện distro (apt / pacman / dnf), thiếu gói gì cài nấy, Rust cài qua rustup.
Cần GTK 4.10+ và libadwaita 1.5+ (Ubuntu 22.04 chỉ có GTK 4.6). Chỉ Arch được thử kỹ. IBus
phải là bộ quản lý input của hệ thống (GNOME mặc định đã vậy).

## Cài đặt

### Một dòng lệnh (online)

Tải mã nguồn, biên dịch và cài luôn; thiếu gì trình cài tự cài thêm nên máy chỉ cần có `curl`
(hoặc `wget`) và `sudo`. Quyền `/dev/uinput` và nhóm `input` được thiết lập sẵn, gói chỉ dùng
để biên dịch sẽ được gỡ sau khi xong, chạy lại không cài đúp. Thêm `--demo` để chạy thử toàn
bộ các bước mà không đụng gì vào máy:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
# hoặc khi máy không có curl:
wget -qO- https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
```

(Đẩy script từ mạng vào `sh` là chạy code người khác — không yên tâm thì đọc script trước.)

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
đưa bạn vào nhóm `input` (hiệu lực từ lần đăng nhập sau). Gỡ: `sudo ./scripts/uninstall.sh`
(giữ nguyên cấu hình). Trên Arch thì gọn hơn: `makepkg -si`.

### Cấp quyền `/dev/uinput` (làm tay)

Bộ gõ nhấn **phím Backspace thật** qua `/dev/uinput` với những app không xoá chữ được qua IBus;
thiếu quyền ghi thì app đó sẽ không gõ được dấu. Tạo `/etc/udev/rules.d/80-uinput.rules`
(mẫu có sẵn trong `data/80-uinput.rules`):

```
KERNEL=="uinput", MODE="0660", GROUP="input"
```

```sh
sudo modprobe uinput
sudo usermod -aG input $USER                    # hiệu lực từ lần đăng nhập sau
sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput
echo uinput | sudo tee /etc/modules-load.d/uinput.conf   # tự nạp mỗi lần khởi động
```

Kiểm tra: `ls -l /dev/uinput` phải hiện `crw-rw---- root input`.

## Bật bộ gõ

1. `ibus restart` (sau khi cài hoặc nâng cấp binary).
2. **Settings → Keyboard → Input Sources → +** → *Vietnamese* → **MăngRS**
   (hoặc `ibus engine ibus-mangrs`).
3. `Super+Space` đổi bộ gõ; bánh răng cạnh tên bộ gõ mở cửa sổ cài đặt.

## Cách dùng

| Việc cần làm          | Cách làm                                                                                   |
| --------------------- | ------------------------------------------------------------------------------------------- |
| Bật/tắt tiếng Việt    | menu bộ gõ → *Bật tiếng Việt*, hoặc `Super+Space`                                           |
| Mở cài đặt            | menu bộ gõ → *Cài đặt…*, hoặc `ibus-engine-mangrs -gui`                                     |
| Đổi kiểu gõ           | menu → *Input Method* (9 kiểu), hoặc GUI                                                    |
| Đổi bảng mã           | menu → *Charset* (11 bảng mã), hoặc GUI                                                     |
| Đổi chế độ gõ         | menu → *Display*, GUI (đủ 9 chế độ), hoặc phím tắt đổi nhanh                                |
| Gõ emoji              | gõ `:tên:` (bật trong trang *Emoji*), hoặc phím tắt mở bảng chọn tại con trỏ                |
| Gán phím tắt          | GUI trang *Phím tắt* (bấm vào hàng, nhấn tổ hợp)                                            |
| Gõ tiếng Anh / code   | cứ gõ bình thường — từ không phải tiếng Việt tự buông qua, URL/email/đường dẫn giữ nguyên   |
| Chế độ theo app       | GUI trang *Luật theo app* (bên dưới)                                                        |

## Luật theo app

Một luật ép một chế độ gõ cho một ứng dụng và đè lên chế độ chung; luật nào khớp trước ăn
trước nên xếp luật cụ thể lên trên luật chung. Làm trong GUI trang *Luật theo app* (chọn app,
chọn chế độ, bấm *Thêm luật*), hoặc sửa tay trong `~/.config/ibus-mangrs/config.toml` (sửa là
áp dụng sau khoảng 1 giây):

```toml
[[app_rules]]
pattern = "chrome"
mode = "preedit"

[[app_rules]]
pattern = "sublime_text"
mode = "surrounding"
```

Giá trị `mode`: `auto`, `preedit` (mặc định), `nonpreedit`, `surrounding`, `smooth`,
`supersmooth`, `select` (ba chế độ này cần `/dev/uinput`, dành cho X11), `forwardkeyevent`,
`forwardselect`, `off`. Tên app khớp theo chuỗi con không phân biệt hoa/thường (`term` ăn cả
`xterm`). Bộ gõ mang sẵn luật cho Chrome, Chromium, Brave, Edge, Vivaldi, các app Electron
(VS Code, Slack, Discord) và GNOME Console → `preedit`, Sublime Text → `surrounding` — mấy app
này bỏ qua tín hiệu xoá chữ, không có luật thì gõ dấu sẽ bị lặp chữ. Chrome và Firefox trên
GNOME Wayland chỉ hiện với IBus như một relay vô danh: muốn đặt luật cho chúng phải cài kèm
GNOME Shell extension (`data/gnome-shell-extension/README.md`) rồi đăng xuất đăng nhập lại.

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
  ghi được — xem phần cấp quyền ở trên.
- **Đổi chế độ mà chẳng ăn gì**: app đó đang có luật riêng đè lên chế độ chung.
- **App XIM (JetBrains) hay app X11 khó tính**: thử hai công tắc *Tương thích X11/XIM* trong
  trang *Tinh chỉnh* của GUI.

## Bản quyền

- [**MăngRS**](https://github.com/fundokukiri/ibus-mangrs) — giấy phép [GPL-3.0](LICENSE).
- [**IBus Bamboo:**](https://github.com/BambooEngine/ibus-bamboo) Bản quyền (C) 2018 Luong Thanh Lam, giấy phép [GPL-3.0](https://github.com/bambooengine/ibus-bamboo?tab=GPL-3.0-1-ov-file).
- [**bamboo-core (Rust):**](https://github.com/bamboo-vi/bamboo-core) thư viện thuật toán gõ tiếng Việt viết bằng Rust, giấy phép [MIT](https://github.com/bamboo-vi/bamboo-core?tab=MIT-1-ov-file).
- [**bamboo-core (Go):**](https://github.com/BambooEngine/bamboo-core) Bản quyền (C) 2018 Luong Thanh Lam, giấy phép [MIT](https://github.com/BambooEngine/bamboo-core?tab=MIT-1-ov-file).
