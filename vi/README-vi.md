# MăngRS

**Tiếng Việt** | [English](README.md)

Bộ gõ tiếng Việt cho **IBus**, được viết bằng **Rust** dựa trên thư viện [`bamboo-core`](https://crates.io/crates/bamboo-core). Tích hợp 9 kiểu gõ (Telex, VNI, VIQR, bàn phím Microsoft và 5 kiểu gõ kết hợp), 9 chế độ hiển thị (Preedit, NonPreedit, Surrounding, 3 chế độ uinput, 2 chế độ ForwardKeyEvent, TẮT), cửa sổ cài đặt bằng GTK4, gõ tắt (macro), từ điển cá nhân, gõ tắt emoji kèm bảng chọn emoji ngay tại con trỏ, quy tắc riêng cho từng ứng dụng và menu tùy chỉnh trên thanh hệ thống (panel).

> **Trạng thái nền tảng:** Được phát triển và kiểm thử trên **Arch Linux** (IBus 1.5.34, GNOME 51, Wayland; đã chạy thử nghiệm trên X11). Các bản phân phối (distro) và môi trường giao diện khác **chưa** được kiểm thử trực tiếp — yêu cầu bắt buộc duy nhất là IBus ≥ 1.5 và GTK4 + libadwaita, do đó phần mềm sẽ hoạt động trên bất kỳ hệ thống nào hỗ trợ IBus. Rất hoan nghênh các phản hồi và báo lỗi từ những bản phân phối khác.

## Tính năng nổi bật

* **9 kiểu gõ cài sẵn**: Telex (`tieengs` → `tiếng`), Telex 2 (dùng `[`/`]` để gõ ơ/ư), Telex W (dùng `z` để xóa dấu thanh), Telex + VNI, Telex + VNI + VIQR, VNI (`tie6ng1`), VNI cho bàn phím Tiếng Pháp/AZERTY, VIQR và kiểu gõ Microsoft (`ti3ng8` → `tiếng`).

* **11 bảng mã đầu ra**: Unicode (mặc định), Unicode NFD, TCVN3, VNI, VIQR, VISCII, VPS, CP1258, BKHCM, Vietware X, Vietware Full.

* **9 chế độ hiển thị** (xem chi tiết bên dưới), cho phép thiết lập riêng cho từng ứng dụng thông qua **Luật theo app**.

* **Quy tắc ứng dụng riêng cho các phần mềm chạy native trên Wayland** (Chrome, Firefox, Sublime Text, ...): Nhờ Gnome Extension đi kèm. Bình thường IBus chỉ nhận diện các ứng dụng này dưới dạng relay ẩn danh của Mutter, extension này sẽ giúp lấy chính xác tên cửa sổ đang active từ GNOME Shell. Tính năng này là tùy chọn — nếu không cài extension, bộ gõ vẫn áp dụng quy tắc cho các ứng dụng mà IBus tự định danh được và các cửa sổ X11.

* **Tùy chọn gõ linh hoạt**: Đặt dấu thanh kiểu mới (`hòa` thay vì kiểu cũ `hoà`), đặt dấu tự do (`hoangf` → `hoàng`), tự động sửa chính tả nguyên âm/phụ âm, tự động phục hồi từ tiếng Anh/nước ngoài, tự động tắt gõ tiếng Việt sau các ký tự URL/email/đường dẫn (`/ : @ . _ ~ = # & % ?`), chuyển `w` → `ư`, `[`/`]` → `ơ`/`ư`, `--` → `—`, tự động viết hoa chữ cái đầu câu, gõ hai khoảng trắng thành dấu chấm.

* **3 cách xóa dấu bằng Backspace**: xóa dấu trước (`hạ` → `ha` → `h`), xóa cả dấu và chữ theo mô hình màn hình (`việt` → `việ` → `vi` → `v`), hoặc kiểu Unikey lùi từng phím thô với dấu xoay vòng ngược (`hạ` → `hã` → `hả` → `hà` → `há` → `ha`). Ba chế độ uinput luôn giữ kiểu lùi từng phím.

* **Hiện chữ có dấu từng bước ở Smooth** (bật theo mặc định): `ee` hiện ngay thành `ê` thay vì giữ phím thô đến hết từ; Backspace giữa từ xóa đúng ký tự cuối nhìn thấy và từ vẫn tiếp tục được gõ dấu (`việt` Backspace `t` → `việt` lại như cũ).

* **Không bao giờ mất dở từ khi click chuột**: Chuỗi ký tự đang gõ dở (preedit) sẽ được ứng dụng tự động chèn vào văn bản khi ứng dụng reset ngữ cảnh nhập (click chuột, đổi focus), đảm bảo từ đang gõ dở luôn xuất hiện đầy đủ trong văn bản. Các từ viết tắt tiếng Việt không bị biến dạng (`ddc` → `đc`, `ddk` → `đk`, `ddt` → `đt`).

* **Gõ tắt (Macro)**: Hỗ trợ chèn tự động thời gian/ngày tháng (`$TIME` / `$DATE`) và viết hoa thông minh; các từ gõ tắt trùng với từ tiếng Việt hợp lệ sẽ không bị đè khi bạn đang gõ từ đó một cách bình thường.

* **Từ điển cá nhân**: Quản lý danh sách các từ tuyệt đối không bao giờ bị hệ thống tự động phục hồi về chữ không dấu.

* **Gõ tắt và Bảng chọn Emoji**: Gõ dạng `:tên_emoji:` để chuyển thành biểu tượng emoji (mặc định tắt), hoặc nhấn phím tắt tùy chỉnh để mở bảng chọn emoji ngay tại vị trí con trỏ — tích hợp sẵn 1.940 emoji với từ khóa tìm kiếm bằng tiếng Việt và tiếng Anh, hỗ trợ điều hướng hoàn toàn bằng bàn phím (mũi tên, PageUp/PageDown, phím số, gõ để tìm kiếm, Enter). Bạn cũng có thể thêm/đè các từ khóa dạng `tên = "ký_tự"` trong tệp `~/.config/ibus-mangrs/emoji.toml`.

* **Tùy biến Menu Panel và Phím tắt**: Menu trên thanh hệ thống (thư mục, chức năng, nhóm radio, đường phân cách, thứ tự, tên hiển thị) được thiết kế lại hoàn toàn trong giao diện cài đặt; menu mặc định vẫn giữ nguyên nếu chưa lưu cấu hình mới. Hỗ trợ gán phím tắt tùy chỉnh cho 14 chức năng, cùng 4 phím tắt tích hợp sẵn (bật/tắt tiếng Việt, chuyển nhanh chế độ hiển thị, mở bảng chọn luật ứng dụng và bảng chọn emoji).

* **Giao diện Cài đặt GTK4 + libadwaita** (Phong cách GNOME Settings: thanh bên phân nhóm, 11 trang cấu hình, hỗ trợ tiếng Việt, tìm kiếm trang thông minh tự mở các mục đang thu gọn); mọi thay đổi cấu hình đều được áp dụng ngay lập tức (hot-reload) trong khoảng 1 giây.

* **Sao lưu & Phôi phục**: Cho phép lưu toàn bộ thiết lập — bao gồm cấu hình, danh sách gõ tắt, từ điển và bảng emoji — thành một tệp JSON duy nhất.

* **Bộ kiểm thử toàn diện**: 348 unit test bao gồm bảng kiểm thử chuẩn (golden table) 894 trường hợp phím gõ → văn bản (102 hàng Telex + 96 hàng VNI + 24 hàng VIQR + 72 hàng kiểu gõ khác x 3 biến thể hoa/thường, 12 hàng Microsoft); khi biên dịch với tùy chọn `--features gui`, bộ test widget GTK sẽ thêm 1 test nữa (tổng cộng 349).

## Các chế độ hiển thị

| Chế độ | Cách hoạt động | Phù hợp nhất cho | 
| ----- | ----- | ----- | 
| Preedit (Mặc định) | Từ đang gõ hiển thị gạch chân trực tiếp tại con trỏ, chèn vào văn bản khi hoàn thành từ | Tương thích rộng rãi nhất | 
| Non-preedit | Mỗi phím gõ được chèn ngay lập tức, bộ gõ tự xóa phần đuôi để sửa khi có dấu | Firefox, các ứng dụng GTK | 
| Surrounding text | Giống Non-preedit + xác minh tính chính xác dựa trên văn bản thực tế ứng dụng phản hồi | Sublime Text | 
| Smooth (uinput) | Gửi phím thô khi gõ, dùng phím Backspace thực tế xóa phần đuôi Latin khi hoàn thành từ | Terminal, các ứng dụng lỗi vặt trên X11 | 
| Super Smooth (uinput) | Tương tự Smooth nhưng bỏ qua bước kiểm tra an toàn trước khi ghi đè | Gõ tốc độ cao trên X11 | 
| Select (uinput) | Gửi Shift+← để bôi đen phần đuôi, gửi phím Delete để xóa rồi chèn từ hoàn chỉnh | Ứng dụng bị lỗi xử lý phím Backspace trên X11 | 
| ForwardKeyEvent | Giống Non-preedit nhưng xóa phần đuôi bằng cách gửi sự kiện phím Backspace qua IBus | Ứng dụng bỏ qua tín hiệu xóa văn bản: Chrome, Electron, GNOME Console, Sublime Text | 
| ForwardKeyEvent II | Tương tự trên, nhưng phần đuôi được bôi đen bằng Shift+← và đè lên khi chèn từ hoàn chỉnh | Ứng dụng xử lý lỗi phím Backspace chuyển tiếp, ứng dụng X11/XIM (JetBrains) | 
| TẮT | Tắt gõ tiếng Việt, các phím gõ tắt (macro) vẫn hoạt động | Dùng khi gõ kết hợp Tiếng Việt và Tiếng Anh/Code | 

Bạn có thể chuyển đổi chế độ hiển thị từ menu trên thanh hệ thống, cửa sổ cài đặt, gán phím tắt chuyển nhanh, hoặc tự động theo ứng dụng thông qua **Luật theo app**. Các chế độ `uinput` yêu cầu quyền truy cập `/dev/uinput` và sẽ tự động fallback về `preedit` nếu không thể giả lập phím vào ứng dụng (trên GNOME Wayland chúng sẽ tự chuyển thành `preedit`; các chế độ này chủ yếu phục vụ phiên làm việc X11).

## Yêu cầu hệ thống

| Bản phân phối | Các gói phụ thuộc (Installer tự động cài đặt) | 
| ----- | ----- | 
| Arch | `ibus` `gtk4` `libadwaita` `dbus`, `gcc` `make` `pkgconf` | 
| Debian / Ubuntu | `ibus` `libgtk-4-dev` `libadwaita-1-dev` `libdbus-1-dev`, build-essential | 
| Fedora | `ibus` `gtk4-devel` `libadwaita-devel` `dbus-devel`, `gcc` `make` | 

Kịch bản cài đặt sẽ tự nhận diện bản phân phối (apt / pacman / dnf), chỉ cài bổ sung các gói còn thiếu và tự quản lý Rust thông qua `rustup`. Yêu cầu GTK ≥ 4.10 và libadwaita ≥ 1.5 (Ubuntu 22.04 hiện chỉ có GTK 4.6). Hiện tại mới chỉ phiên bản Arch Linux được kiểm thử chính thức — tên các gói trên Debian/Ubuntu và Fedora là các tên chuẩn nhưng chưa được xác minh thực tế. IBus phải được chọn làm bộ gõ mặc định của hệ thống (mặc định trên GNOME).

### Tự cài các gói phụ thuộc (đúng những gì trình cài 1 dòng chạy)

```
# Arch / Manjaro / EndeavourOS
sudo pacman -S --needed gcc make pkgconf tar gzip curl ibus dbus gtk4 libadwaita desktop-file-utils

# Debian / Ubuntu / Mint / Pop!_OS
sudo apt install build-essential pkg-config tar gzip curl ibus desktop-file-utils \
    libdbus-1-dev libgtk-4-dev libadwaita-1-dev

# Fedora / RHEL / CentOS
sudo dnf install gcc make pkgconf-pkg-config tar gzip curl ibus dbus-devel \
    gtk4-devel libadwaita-devel desktop-file-utils

# Rust (mọi bản phân phối)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
```

## Biên dịch

```
cargo build --release --features gui

```

Lõi của bộ gõ lấy từ crate `bamboo-core` trên crates.io (phiên bản 0.5.4).

## Cài đặt

### Cài đặt nhanh 1 dòng (Online)

Tự động tải mã nguồn, biên dịch và cài đặt; các thư viện phụ thuộc còn thiếu (IBus, GTK4, công cụ build, Rust) sẽ được nhận diện và cài bổ sung, bạn chỉ cần có sẵn `curl` (hoặc `wget`) và quyền `sudo` — chạy lại sẽ không cài thêm gì mới. Quy tắc udev cho `/dev/uinput` và nhóm người dùng `input` cũng sẽ được cấu hình tự động. Sau khi cài thành công, các gói chỉ dùng để build mà lần chạy này vừa cài sẽ được **gỡ lại** (`KEEP_BUILD_DEPS=1` để giữ lại); gói có sẵn trên máy không bao giờ bị đụng tới, các thư viện runtime vẫn giữ, và Rust (rustup) cũng giữ — không cần thì gỡ bằng `rustup self uninstall`:

```
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
# hoặc khi máy không có curl:
wget -qO- https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh

```

*(Lưu ý: Việc chuyển trực tiếp script từ internet vào `sh` sẽ chạy mã lệnh từ mạng — hãy kiểm tra nội dung script trước nếu bạn cần đảm bảo an toàn.)*

### Cài đặt từ mã nguồn cục bộ

Biên dịch, cài đặt và khởi động lại IBus:

```
cargo build --release --features gui
sudo ./scripts/install.sh
ibus restart

```

Kịch bản cài đặt sẽ sao chép tệp thực thi vào `/usr/lib/ibus-mangrs/`, đăng ký component tại `/usr/share/ibus/component/`, cài đặt quy tắc udev cho `/dev/uinput` và thêm user của bạn vào group `input` (quyền hạn nhóm sẽ có hiệu lực ở lần đăng nhập tiếp theo).

Để gỡ cài đặt (giữ lại cấu hình cá nhân):
`sudo ./scripts/uninstall.sh`

Gói cài đặt cho Arch Linux (sử dụng đường dẫn udev chuẩn của hệ thống):

```
makepkg -si

```

### Cấu hình thủ công quyền `/dev/uinput` (Nếu cần)

Bộ gõ cần gửi **phím Backspace thực tế** qua `/dev/uinput` khi ứng dụng không hỗ trợ xóa văn bản qua IBus. Nếu không có quyền ghi, các ứng dụng này sẽ không thể gõ được dấu thanh. Tạo tệp `/etc/udev/rules.d/80-uinput.rules` (có sẵn mẫu tại `data/80-uinput.rules`):

```
KERNEL=="uinput", MODE="0660", GROUP="input"

```

Sau đó thực hiện các lệnh:

```
sudo modprobe uinput                               # Nạp kernel module ngay lập tức
sudo usermod -aG input $USER              # Có hiệu lực từ lần đăng nhập sau
sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput
echo uinput | sudo tee /etc/modules-load.d/uinput.conf   # Tự động nạp khi khởi động hệ thống

```

Kiểm tra lại bằng lệnh `ls -l /dev/uinput`, kết quả phải hiển thị dạng: `crw-rw---- root input`.

## Kích hoạt bộ gõ

1. Chạy `ibus restart` (sau khi cài đặt hoặc nâng cấp).

2. Vào **Settings → Keyboard → Input Sources → +** → chọn *Vietnamese* → chọn **MăngRS** (hoặc dùng lệnh `ibus engine ibus-mangrs`).

3. Dùng tổ hợp phím `Super+Space` để chuyển đổi giữa các bộ gõ; biểu tượng bánh răng bên cạnh tên bộ gõ dùng để mở cửa sổ cài đặt.

## Hướng dẫn sử dụng

| Thao tác | Cách thực hiện | 
| ----- | ----- | 
| Bật/tắt gõ tiếng Việt | Menu trên panel → *Bật tiếng Việt*, hoặc nhấn `Super+Space` | 
| Mở cửa sổ cài đặt | Menu trên panel → *Cài đặt…*, hoặc chạy lệnh `ibus-engine-ibus-mangrs -gui` | 
| Đổi kiểu gõ | Menu → *Input Method* (9 kiểu gõ), hoặc chỉnh trong giao diện GUI | 
| Đổi bảng mã | Menu → *Charset* (11 bảng mã), hoặc chỉnh trong giao diện GUI | 
| Đổi chế độ hiển thị | Menu → *Display* (mặc định hiện 4 chế độ; menu có thể tùy biến), GUI (9 chế độ), hoặc dùng phím tắt | 
| Phím tắt đổi chế độ | Giao diện GUI → trang *Phím tắt* → *Thứ tự khi đổi chế độ* (Ví dụ: `Control+Alt+v`) | 
| Cài đặt nhanh | Menu → *Quick Settings* (gõ tắt, tự động viết hoa, viết tắt, phục hồi từ tiếng Anh) | 
| Chèn Emoji | Gõ `:tên_emoji:` (cần bật trong GUI trang *Emoji*), hoặc bấm phím tắt để mở bảng chọn tại con trỏ | 
| Tùy biến menu panel | Giao diện GUI → trang *Menu trên panel* (thư mục, chức năng, nhóm radio, đường phân cách) | 
| Gán phím tắt | Giao diện GUI → trang *Phím tắt* (14 chức năng — nhấp vào hàng và bấm tổ hợp phím) | 
| Gõ tiếng Anh / Mã code | Gõ bình thường — từ không hợp lệ sẽ tự khắc phục hồi; URL/email/đường dẫn giữ nguyên thô | 
| Cấu hình chế độ theo app | Giao diện GUI → trang *Luật theo app* (xem chi tiết mục bên dưới) | 

## Luật theo app (Cấu hình chế độ hiển thị riêng)

Luật theo app cho phép ép buộc một chế độ hiển thị cụ thể cho từng ứng dụng, ghi đè lên thiết lập chung của hệ thống. Các quy tắc được ưu tiên đối chiếu từ trên xuống dưới, **quy tắc nào khớp trước sẽ được áp dụng**, do đó bạn nên đặt các quy tắc cụ thể ở trên các quy tắc chung.

**Cấu hình qua giao diện:** Vào trang *Luật theo app*. Chọn tên ứng dụng ở mục tên (hoặc bấm sao chép app đang focus), chọn chế độ hiển thị tương ứng, rồi bấm *Thêm luật*: quy tắc sẽ được lưu cùng toàn bộ tệp cài đặt. Mỗi quy tắc là một hàng có thể kéo thả để thay đổi thứ tự (menu ⋮ hỗ trợ Lên/Xuống/Xóa; khi xóa có nút Hoàn tác), mục *Thử luật* giúp bạn kiểm tra xem tên ứng dụng bất kỳ sẽ khớp với quy tắc nào. Danh sách thả xuống chỉ hiện các ứng dụng bộ gõ đã từng ghi nhận (50 app gần nhất trong `~/.cache/ibus-mangrs/clients`) cùng hai đối tượng đặc biệt: `gnome-shell (app Wayland)` — đại diện cho mọi ứng dụng Wayland khi chưa cài extension — và `(app không báo tên)` — các ứng dụng không trả về tên. Bạn chỉ cần gõ một ký tự trong ứng dụng bất kỳ là tên ứng dụng đó sẽ xuất hiện trong danh sách.

**Cấu hình thủ công:** Thêm các khối `[[app_rules]]` vào tệp `~/.config/ibus-mangrs/config.toml` (tự động cập nhật sau khoảng 1 giây):

```
[[app_rules]]
pattern = "chrome"
mode = "preedit"

[[app_rules]]
pattern = "sublime_text"
mode = "surrounding"

```

**Cơ chế khớp tên:** Khớp chuỗi con không phân biệt hoa thường; các tiền tố của toolkit (`gtk4-im:`, `wayland:`, `x11:`) sẽ được tự động bỏ qua. Tên ứng dụng được lấy ưu tiên từ GNOME Extension đi kèm, sau đó đến cửa sổ X11 đang active (`xprop`), cuối cùng là prgname do IBus báo về. Các ứng dụng X11/XIM (như JetBrains và ứng dụng Java) thường chỉ báo về tên cố định là `xim`, do đó bộ gõ sẽ lấy tên từ Extension/`xprop` — một quy tắc dạng `pattern = "jetbrains"` vẫn sẽ khớp chính xác với chúng. Các mẫu ngắn sẽ khớp rộng hơn (`term` sẽ khớp cả `xterm`). Thiết lập `pattern = "none"` dùng để nhắm tới các ứng dụng hoàn toàn không khai báo tên.

**Các chế độ:** `auto`, `preedit` (mặc định), `nonpreedit`, `surrounding`, `smooth`, `supersmooth`, `select` (3 chế độ này cần `/dev/uinput` và dành cho X11), `forwardkeyevent`, `forwardselect` (tên khác: `forwardkeyevent2`), `off` (cấu hình cũ dùng `defaulttyping` sẽ tự động chuyển sang chế độ này). Hai chế độ forward sẽ chèn văn bản giống `nonpreedit` nhưng xóa phần đuôi bằng cách gửi sự kiện phím qua IBus thay vì gửi tín hiệu xóa. Điều này giúp tương thích tốt với các ứng dụng bỏ qua lệnh `DeleteSurroundingText` (như Chrome, Electron, GNOME Console) kể cả trên Wayland (nơi `/dev/uinput` không tác động được), cũng như các ứng dụng X11/XIM (JetBrains) vốn không khai báo tính năng ngữ cảnh: `forwardkeyevent` sẽ gửi phím Backspace, còn `forwardselect` sẽ bôi đen phần đuôi bằng Shift+← và đè nội dung mới lên. Các chế độ này chỉ hoạt động nếu ứng dụng chấp nhận phím gửi qua IBus, nên hãy thử nghiệm riêng cho từng app.

**Các quy tắc tích hợp sẵn** (hiển thị trong ô văn bản khi `config.toml` chưa có mục `app_rules`; bạn có thể sửa hoặc xóa trực tiếp tại đây): Chrome, Chromium, Brave, Edge, Vivaldi, các ứng dụng Electron (VS Code, Slack, Discord) và GNOME Console → `preedit`; Sublime Text → `surrounding`. Nguyên nhân là do các ứng dụng này thường bỏ qua lệnh `DeleteSurroundingText`, dẫn đến việc bị lặp chữ khi gõ.

## Tùy chọn X11/XIM

Bộ gõ cung cấp 2 công tắc tương thích trong cửa sổ cài đặt (Trang *Tinh chỉnh* → mục *Tương thích X11/XIM*). Cả hai đều **tắt theo mặc định**, và chỉ cần thiết khi ứng dụng kết nối với bộ gõ theo các cơ chế đặc thù:

| Công tắc | Tên trong `config.toml` | Chức năng | 
| ----- | ----- | ----- | 
| Delete without reporting surrounding elements | `x11_delete_without_report` | Cho phép ứng dụng X11 trực tiếp (Sublime Text, các app GTK3 X11 khác) xóa phần đuôi qua IBus dù ứng dụng chưa bao giờ báo về nội dung văn bản — tương tự cơ chế của ibus-bamboo. Nếu không bật, các ứng dụng này sẽ bị giữ lại ký tự thô khi không thể dùng chế độ giả lập phím (`tungf`). Rủi ro: Do không có báo cáo văn bản, số lượng ký tự xóa không được giới hạn an toàn, nếu con trỏ bị di chuyển thủ công có thể dẫn đến xóa nhầm ký tự. | 
| Preedit cho app XIM | `preedit_for_xim` | Giữ chế độ gạch chân cho các ứng dụng XIM (JetBrains và các app Java, vốn luôn báo tên cố định là `xim`). Server XIM sẽ tự vẽ chữ đang gõ trên cửa sổ riêng và chèn vào văn bản khi hoàn tất từ. Nếu không bật, các app này không thể dùng chế độ mặc định và bắt buộc phải dùng quy tắc `forwardkeyevent`. Rủi ro: Ứng dụng nào đàm phán `XIMPreeditNothing` sẽ không hiện chữ cho đến khi gõ xong từ. | 

## Các tệp cấu hình

```
~/.config/ibus-mangrs/config.toml     # Tất cả cài đặt hệ thống, tự động cập nhật sau ~1s
~/.config/ibus-mangrs/macros.toml     # Tệp gõ tắt (mỗi dòng dạng key=expansion, hỗ trợ $TIME / $DATE)
~/.config/ibus-mangrs/emoji.toml      # Tệp emoji cá nhân (dạng tên = "ký_tự", cùng cấu trúc với macros.toml)
~/.config/ibus-mangrs/dictionary.txt  # Từ điển cá nhân chứa các từ không bao giờ tự động phục hồi

```

## Xử lý lỗi thường gặp

* **Bộ gõ không xuất hiện trong danh sách Input Sources**: Chạy lệnh `ibus write-cache --system` sau đó chạy `ibus-daemon -drx`; đảm bảo tệp `/usr/share/ibus/component/ibus-mangrs.xml` đã tồn tại.

* **Không gõ được dấu trong một số app** (Sublime, ứng dụng XWayland): Quyền truy cập `/dev/uinput` bị thiếu hoặc không có quyền ghi — xem hướng dẫn thiết lập quyền ở mục trên.

* **Thay đổi chế độ hiển thị không có tác dụng**: Đã có một **Luật theo app** đang áp dụng cho ứng dụng đó và ghi đè lên chế độ chung.

* **Luật theo app không hoạt động với Chrome, Firefox hoặc Sublime Text trên GNOME Wayland**: IBus chỉ nhận diện các ứng dụng này là một relay ẩn danh của Mutter nên bộ gõ không có tên để khớp quy tắc. Hãy cài đặt GNOME Shell Extension đi kèm (`data/gnome-shell-extension/README.md`), sau đó **đăng xuất và đăng nhập lại** (chỉ bật hoặc sửa extension tại chỗ sẽ không có tác dụng).

* **Các lỗi khác**: Bật tùy chọn *Ghi log gỡ lỗi* (Trong GUI trang *Cài đặt khác*, hoặc sửa `debug_log = true` trong `config.toml`), tái hiện lại lỗi, sau đó đính kèm tệp `~/.cache/ibus-mangrs/debug.log` khi báo lỗi — bộ gõ sẽ ghi lại chi tiết các sự kiện đổi focus, tính năng ứng dụng, phím gõ, lệnh xóa/chèn kèm mốc thời gian tính theo miligiây để đối chiếu với `journalctl`. Tệp log này chỉ ghi lại các quyết định của bộ gõ (con trỏ, độ dài, số lượng ký tự xóa) chứ **không bao giờ ghi lại nội dung văn bản bạn gõ**. Để debug lỗi mất chữ hoặc gõ sai từ, bạn có thể bật công tắc thứ hai: *Log cả nội dung đang gõ* (`debug_log_text`), công tắc này sẽ ghi thêm nội dung văn bản xung quanh con trỏ vào cùng tệp log trên: **chỉ bật tùy chọn này khi đang kiểm thử lỗi và tuyệt đối không đính kèm tệp log chứa nội dung này lên các trang báo lỗi công khai (Issue)**.

## Kiểm thử (Testing)

```
cargo test                      # Chạy 348 unit test, bao gồm bảng kiểm thử golden table 894 trường hợp
cargo clippy --all-targets      # (Tổng 349 test nếu thêm tùy chọn --features gui)

```

Chạy thử trực tiếp với daemon IBus mà không cần cài đặt vào hệ thống:

```
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs

```

## Kiến trúc mã nguồn

* `src/server/` — D-Bus, bộ lọc tin nhắn, menu panel, phím tắt, theo dõi tệp cấu hình, truyền tải phím, bảng chọn luật ứng dụng và bảng chọn emoji (bảng tra cứu IBus tại vị trí con trỏ).

* `src/engine/` — Máy trạng thái (State machine): quản lý chế độ hiển thị, luật ứng dụng, gõ tắt, phục hồi chữ, xác minh và sửa lỗi văn bản.

* `src/bamboo_wrapper.rs` — Phần mã duy nhất tương tác với `bamboo-core`; tự tính toán biến động đầu ra (output delta).

* `src/uinput.rs` — Bàn phím ảo: gửi phím Backspace theo nhịp, chọn văn bản, Ctrl+V (bảng phím US-QWERTY).

* `src/paste.rs` — Kênh Clipboard Unicode cho các chế độ uinput (phục hồi debounced).

* `src/macros.rs` — Bảng gõ tắt + từ điển cá nhân.

* `src/emoji.rs` — Bảng gõ tắt emoji: danh sách tích hợp sẵn và tệp `emoji.toml` của người dùng.

* `src/backup.rs` — Sao lưu/Phục hồi toàn bộ cấu hình vào 1 tệp JSON: thiết lập, gõ tắt, từ điển, bảng emoji.

* `src/golden.rs` — 894 trường hợp kiểm thử chuẩn (keystroke → text).

* `src/ibus_prop.rs` — Mã hóa menu thuộc tính IBus; `src/keymap.rs` — Giải mã keysym.

* `src/debug_log.rs` — Log gỡ lỗi 2 cấp tại `~/.cache/ibus-mangrs/debug.log` (mặc định chỉ ghi quyết định xử lý; ghi nội dung gõ nằm sau công tắc riêng).

* `src/config.rs` — Cấu hình TOML dùng chung giữa engine + GUI; `src/gui/` — Cửa sổ cài đặt libadwaita với 11 trang (khi bật feature `gui`), nội dung giao diện lấy từ `data/gui-content.toml`.

## Giấy phép bản quyền

* [**MăngRS**](https://github.com/fundokukiri/ibus-mangrs) được phát hành theo giấy phép [GPL-3.0 License](LICENSE).

* [**IBus Bamboo:**](https://github.com/BambooEngine/ibus-bamboo) Bản quyền (C) 2018 thuộc về Luong Thanh Lam. [GPL-3.0 License](https://github.com/bambooengine/ibus-bamboo?tab=GPL-3.0-1-ov-file)

* [**bamboo-core (Rust):**](https://github.com/bamboo-vi/bamboo-core) Thư viện xử lý thuật toán gõ tiếng Việt viết bằng Rust. [MIT License](https://github.com/bamboo-vi/bamboo-core?tab=MIT-1-ov-file)

* [**bamboo-core (Go):**](https://github.com/BambooEngine/bamboo-core) Bản quyền (C) 2018 thuộc về Luong Thanh Lam. [MIT License](https://github.com/BambooEngine/bamboo-core?tab=MIT-1-ov-file)