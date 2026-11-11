<p align="center">
  <img src="../data/logo.png" alt="Logo MăngRS" width="360">
</p>

# MăngRS

**Tiếng Việt** | [English](../README.md)

Bộ gõ tiếng Việt cho **IBus**, viết bằng **Rust** trên nền
[`bamboo-core`](https://crates.io/crates/bamboo-core). 9 kiểu gõ, 11 bảng mã, 9 chế độ gõ,
cửa sổ cài đặt GTK4, gõ tắt, bảng chọn emoji ngay tại con trỏ và luật riêng cho từng
ứng dụng.

> Phát triển và thử nghiệm trên **Arch Linux** (IBus 1.5.34, GNOME 51, Wayland; X11 đã chạy
> thử). Chỉ cần IBus ≥ 1.5 và GTK4 + libadwaita — nơi nào IBus chạy được thì bộ gõ này chạy
> được. Rất mong nhận được phản hồi từ người dùng các distro khác.

## Tính năng

- **9 kiểu gõ sẵn** — Telex (`tieengs` → `tiếng`), Telex 2, Telex W, Telex + VNI,
  Telex + VNI + VIQR, VNI (`tie6ng1`), VNI bàn phím Pháp, VIQR và bố cục Microsoft
  (`ti3ng8` → `tiếng`) — kèm **11 bảng mã** xuất ra (Unicode, VNI, VIQR, TCVN3, VISCII, ...).
- **9 chế độ gõ** (bảng dưới), đặt riêng cho từng ứng dụng qua **Luật theo app**.
- **Tùy chọn gõ linh hoạt**: dấu kiểu mới `hòa` hay kiểu cũ `hoà`, gõ dấu tự do
  (`hoangf` → `hoàng`), tự sửa âm tiết sai, tự buông từ tiếng nước ngoài, tự nhả tay khi gõ
  URL/email/đường dẫn, `w` → `ư`, `--` → `—`, tự viết hoa đầu câu, hai dấu cách thành dấu chấm.
- **Ba kiểu Backspace xoá dấu**: xoá dấu trước (`hạ` → `ha`), xoá nguyên ký tự màn hình
  (`việt` → `việ`), hay kiểu Unikey lùi từng phím (`hạ` → `hã` → `hà` → ...).
- **Chữ hiện dần từng phím ở chế độ Flow/Turbo** (bật sẵn): gõ `ee` thấy ngay `ê` thay vì đợi
  hết từ. Click chuột hay đổi cửa sổ cũng không mất chữ đang gõ dở — preedit vào văn bản
  đúng như đang hiện, kể cả chỗ sửa tắt như `ddc` → `đc`.
- **Gõ tắt** có `$TIME` / `$DATE` và viết hoa thông minh; **từ điển cá nhân** chứa những từ
  không bao giờ bị tự sửa.
- **Emoji**: gõ `:tên:` ra emoji (mặc định tắt), hoặc bấm phím tắt mở bảng chọn ngay tại con
  trỏ — 1.940 emoji tìm được bằng cả tiếng Việt lẫn tiếng Anh; thêm bộ riêng dạng
  `tên = "ký tự"` trong `emoji.toml`.
- **Menu trên panel sửa được** (thư mục, chức năng, nhóm chọn, đường phân cách) và 14 chức
  năng gán phím tắt được, cộng 4 khe phím tắt có sẵn (mặc định chưa gán phím nào).
- **Cửa sổ cài đặt GTK4 + libadwaita** (12 trang, tiếng Việt hoặc tiếng Anh — hàng ở
  _Cài đặt khác_ đổi ngôn ngữ): sửa gì cũng áp dụng sau khoảng 1 giây, và toàn bộ thiết lập
  sao lưu ra một tệp JSON duy nhất.

## Các chế độ gõ

| Chế độ (nhãn)      | Giá trị config    | Cách hoạt động                                                  |
| ------------------ | ----------------- | ---------------------------------------------------------------- | 
| Gạch chân (preedit)| `preedit`         | chữ hiện ngay tại con trỏ có gạch chân, hết từ thì vào văn bản   | 
| Không gạch chân    | `nonpreedit`      | phím nào vào ngay phím đó, có dấu thì xoá đuôi viết lại          | 
| Surrounding text   | `surrounding`     | như Không gạch chân, thêm bước đối chiếu với chữ app báo về      | 
| Flow (uinput)      | `smooth`          | gửi phím thô khi gõ, hết từ dùng Backspace thật xoá đuôi         | 
| Turbo (uinput)     | `supersmooth`     | như Flow nhưng bỏ bước kiểm tra an toàn                          | 
| Select (uinput)    | `select`          | Shift+← bôi đen đuôi, Delete xoá, từ vào văn bản                 | 
| Forward Backspace  | `forwardkeyevent` | xoá đuôi bằng sự kiện phím Backspace gửi qua IBus                | 
| Forward Selection  | `forwardselect`   | bôi đen đuôi bằng Shift+← gửi qua IBus, chữ mới đè lên vùng chọn | 
| OFF                | `off`             | tắt xử lý tiếng Việt, gõ tắt và tự viết hoa vẫn chạy             | 

Nhãn là tên hiện trong menu bộ gõ và cửa sổ cài đặt; giá trị config là thứ `config.toml` và
Luật theo app nhận.

Đổi chế độ ở menu bộ gõ, hàng _Chế độ gõ_ trong cửa sổ cài đặt, phím tắt, hoặc theo từng app
qua **Luật theo app**. Ba chế độ uinput nhấn phím thật qua `/dev/uinput`: cần quyền ghi thiết
bị này, không có thì tự rơi về preedit (Select còn cần app báo surrounding-text). Chúng chạy
trên cả X11 lẫn GNOME Wayland. Hai chế độ Forward không cần `/dev/uinput` cũng không cần báo
cáo surrounding-text — phần xoá hay vùng chọn đi qua sự kiện phím gửi tiếp. Config hay luật
còn giữ giá trị `auto` hoặc `defaulttyping` (đã bỏ) tự chuyển sang `preedit` và `off`.

## Yêu cầu

| Bản phân phối  | Gói cần có (trình cài một dòng tự lo)                          |
| -------------- | --------------------------------------------------------------- |
| Arch           | `ibus` `gtk4` `libadwaita` `dbus` + `gcc` `make` `pkgconf`      |
| Debian / Ubuntu| `ibus` `libgtk-4-dev` `libadwaita-1-dev` `libdbus-1-dev` + build-essential |
| Fedora         | `ibus` `gtk4-devel` `libadwaita-devel` `dbus-devel` + `gcc` `make` |

Trình cài nhận diện distro (apt / pacman / dnf / zypper), thiếu gói gì cài nấy, Rust cài qua
rustup. Cần GTK 4.10+ và libadwaita 1.5+ (Ubuntu 22.04 chỉ có GTK 4.6). Chỉ Arch được thử
kỹ. IBus phải là bộ quản lý input của hệ thống (GNOME mặc định đã vậy).

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
3. `Super+Space` đổi giữa các nguồn nhập (phím riêng của GNOME); bánh răng cạnh tên bộ gõ
   mở cửa sổ cài đặt.

## Cách dùng

| Việc cần làm          | Cách làm                                                                                   |
| --------------------- | ------------------------------------------------------------------------------------------- |
| Bật/tắt tiếng Việt    | menu bộ gõ → *Bật tiếng Việt*, hoặc phím tắt gán được (mặc định chưa gán)                    |
| Mở cài đặt            | menu bộ gõ → *Cấu hình*, hoặc `ibus-engine-mangrs -gui`                                      |
| Đổi kiểu gõ           | menu → *Kiểu gõ* (9 kiểu), hoặc GUI                                                          |
| Đổi bảng mã           | menu → *Bảng mã* (11 bảng mã), hoặc GUI                                                      |
| Đổi chế độ gõ         | menu → *Chế độ gõ*, GUI (đủ 9 chế độ), hoặc phím tắt đổi nhanh                               |
| Gõ emoji              | gõ `:tên:` (bật trong trang *Emoji*), hoặc phím tắt mở bảng chọn tại con trỏ                 |
| Chọn luật theo app    | phím tắt luật theo app mở bảng chọn ngay tại con trỏ: app đang dùng và chế độ của nó         |
| Gán phím tắt          | GUI trang *Phím tắt* (bấm vào hàng, nhấn tổ hợp)                                             |
| Gõ tiếng Anh / code   | cứ gõ bình thường — từ không phải tiếng Việt tự buông qua, URL/email/đường dẫn giữ nguyên    |
| Chế độ theo app       | GUI trang *Luật theo app* (bên dưới)                                                         |

Cả bốn khe phím tắt có sẵn (bật/tắt tiếng Việt, đổi chế độ gõ, chọn luật theo app, bảng
emoji) mặc định chưa gán phím — đặt trên trang *Phím tắt*. `Super+Space` là phím đổi nguồn
nhập của GNOME, không phải phím tắt của MăngRS.

## Luật theo app

Một luật ép một chế độ gõ cho một ứng dụng và đè lên chế độ chung; luật nào khớp trước ăn
trước nên xếp luật cụ thể lên trên luật chung. Tên app khớp theo chuỗi con không phân biệt
hoa/thường (`term` ăn cả `xterm`); pattern đặc biệt `none` nhắm những app không bao giờ báo
tên. Làm trong GUI trang *Luật theo app* (chọn app, chọn chế độ), qua phím tắt bảng chọn
luật, hoặc sửa tay trong `~/.config/ibus-mangrs/config.toml` (sửa là áp dụng sau khoảng
1 giây):

```toml
[[app_rules]]
pattern = "chrome"
mode = "preedit"

[[app_rules]]
pattern = "sublime_text"
mode = "surrounding"
```

Giá trị `mode`: `preedit` (mặc định), `nonpreedit`, `surrounding`, `smooth`, `supersmooth`,
`select` (ba chế độ này nhấn phím thật qua `/dev/uinput`), `forwardkeyevent`,
`forwardselect`, `off`.

Bộ gõ không mang luật nào sẵn. Engine có sẵn cơ chế bảo vệ cho các client hay nuốt thao tác
xoá (Chrome, Chromium, Edge, Brave, Vivaldi và các app Electron như VS Code, Slack, Discord):
nếu một chế độ có xoá-ghi-lại đang bật ở đó, engine gõ bằng preedit thay vì để chữ bị lặp.
Muốn app đó dùng chế độ khác, tự thêm luật.

Chrome và Firefox trên GNOME Wayland chỉ hiện với IBus như một relay vô danh: muốn đặt luật
cho chúng phải cài kèm GNOME Shell extension (`data/gnome-shell-extension/README.md`) rồi
đăng xuất đăng nhập lại.

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

- [**MăngRS**](https://github.com/fundokukiri/ibus-mangrs) — giấy phép [GPL-3.0](../LICENSE).
- [**IBus Bamboo:**](https://github.com/BambooEngine/ibus-bamboo) Bản quyền (C) 2018 Luong Thanh Lam, giấy phép [GPL-3.0](https://github.com/bambooengine/ibus-bamboo?tab=GPL-3.0-1-ov-file).
- [**bamboo-core (Rust):**](https://github.com/bamboo-vi/bamboo-core) thư viện thuật toán gõ tiếng Việt viết bằng Rust, giấy phép [MIT](https://github.com/bamboo-vi/bamboo-core?tab=MIT-1-ov-file).
- [**bamboo-core (Go):**](https://github.com/BambooEngine/bamboo-core) Bản quyền (C) 2018 Luong Thanh Lam, giấy phép [MIT](https://github.com/BambooEngine/bamboo-core?tab=MIT-1-ov-file).
