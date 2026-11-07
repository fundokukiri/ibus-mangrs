<p align="center">
  <img src="data/logo.png" alt="Logo MăngRS" width="360">
</p>

# MăngRS

**Tiếng Việt** | [English](README-en.md)

Bộ máy gõ tiếng Việt cho **IBus**, viết bằng **Rust** trên nền
[`bamboo-core`](https://crates.io/crates/bamboo-core). Chín preset kiểu gõ (Telex, VNI, VIQR,
bố cục Microsoft và năm tổ hợp), mười kiểu hiển thị (Auto, Preedit, NonPreedit, Surrounding,
ba kiểu uinput, hai kiểu ForwardKeyEvent, OFF), cửa sổ cài đặt GTK4, macro, từ điển tùy chọn,
emoji qua shortcode kèm bảng chọn ngay tại con trỏ, luật theo từng ứng dụng và menu panel
tùy chỉnh được.

> **Trạng thái nền tảng:** phát triển và kiểm thử trên **Arch Linux** (IBus 1.5.34, GNOME 51,
> Wayland; X11 đã kiểm thử nhanh). Các bản phân phối và môi trường desktop khác **chưa được
> kiểm thử** — IBus ≥ 1.5 và GTK4 + libadwaita là các yêu cầu bắt buộc duy nhất, nên nó phải hoạt động ở
> bất cứ đâu IBus hoạt động. Rất mong nhận phản hồi từ các distro khác.

---

## Tính năng

- **Chín preset kiểu gõ**: Telex (`tieengs` → `tiếng`), Telex 2 (`[`/`]` gõ ơ/ư), Telex W
  (`z` bỏ dấu thanh), Telex + VNI, Telex + VNI + VIQR, VNI (`tie6ng1`), VNI cho bàn phím
  Pháp/AZERTY, VIQR và bố cục Microsoft (`ti3ng8` → `tiếng`).
- **Mười một bộ bảng mã đầu ra**: Unicode (mặc định), Unicode NFD, TCVN3, VNI, VIQR, VISCII,
  VPS, CP1258, BKHCM, Vietware X, Vietware Full.
- **Mười kiểu hiển thị** (xem bên dưới), chọn được riêng cho từng ứng dụng qua Luật ứng dụng.
- **Luật theo ứng dụng cho các ứng dụng Wayland thuần** (Chrome, Firefox, Sublime Text, ...)
  thông qua extension GNOME Shell đi kèm: IBus chỉ báo cáo các ứng dụng đó như một mutter
  relay ẩn danh, nên extension sẽ định danh cửa sổ đang focus từ bên trong Shell. Tùy chọn —
  nếu không cài, luật vẫn khớp với các ứng dụng IBus tự định danh và các cửa sổ X11.
- **Tùy chọn gõ**: đặt dấu theo kiểu hiện đại (`hòa`) so với kiểu truyền thống (`hoà`), đánh
  dấu tự do (`hoangf` → `hoàng`), tự sửa âm tiết không hợp lệ (kiểm tra kết hợp âm vị), tự
  khôi phục từ không phải tiếng Việt, bỏ qua phím thô sau các ký tự URL/email/đường dẫn
  (`/ : @ . _ ~ = # & % ?`), `w` → `ư`, `[`/`]` → `ơ`/`ư`, `--` → `—`, tự viết hoa, hai dấu
  cách → dấu chấm.
- **Cú nhấp chuột không làm mất từ đang gõ dở**: preedit được chính ứng dụng commit khi nó
  reset input context (nhấp chuột, đổi focus), nên từ đang soạn luôn lọt vào văn bản ở mọi
  thời điểm gián đoạn. Các từ viết tắt tiếng Việt được giữ nguyên (`ddc` → `đc`, `ddk` →
  `đk`, `ddt` → `đt`).
- **Macro** với mở rộng `$TIME` / `$DATE` và viết hoa thông minh; phím tắt macro là một từ
  tiếng Việt hợp lệ thì không bao giờ tự mở rộng khi bạn đang gõ từ đó bình thường.
- **Từ điển tùy chọn**: những từ không bao giờ bị tự khôi phục.
- **Emoji qua shortcode và bảng chọn**: gõ `:name:` sẽ được thay bằng emoji (tắt theo mặc
  định), và một phím tắt có thể gán mở bảng chọn ngay tại con trỏ — 1.940 emoji có sẵn
  với từ khóa tiếng Việt và tiếng Anh, điều hướng được bằng bàn phím (mũi tên, đổi trang, chữ
  số, gõ để tìm, Enter). Các cặp `name = "glyph"` riêng trong `~/.config/ibus-mangrs/emoji.toml`
  ghi đè hoặc mở rộng bảng này.
- **Menu panel tùy chỉnh và phím tắt riêng**: menu trên panel (thư mục, chức năng, nhóm radio,
  dấu phân cách, thứ tự, tên) được thiết kế lại trong cửa sổ cài đặt, và menu mặc định được giữ
  nguyên khi chưa có thiết kế nào được lưu; 14 chức năng nhận phím tắt gán được, bên cạnh bốn
  phím tắt có sẵn (bật/tắt tiếng Việt, xoay kiểu hiển thị, bảng chọn luật theo app và bảng
  chọn emoji).
- **Cửa sổ cài đặt GTK4 + libadwaita** (kiểu GNOME Settings: thanh bên có nhóm, 11 trang,
  tiếng Việt, ô tìm kiếm mở sẵn các mục đang thu gọn khi có truy vấn); mọi thay đổi được nạp
  lại nóng trong khoảng 1 giây.
- **Sao lưu/khôi phục** toàn bộ cài đặt — config, macro, từ điển và bảng emoji — thành một
  file JSON.
- 300 unit test, trong đó có bảng golden 894 ca phím→văn bản (102 hàng Telex + 96 hàng VNI +
  24 hàng VIQR + 72 hàng kiểu gõ bổ sung x 3 biến thể hoa/thường, 12 hàng Microsoft); các test
  widget GTK thêm một ca nữa khi build với `--features gui` (301).

## Các kiểu hiển thị

| Kiểu                  | Cách hoạt động                                                                          | Phù hợp nhất với                                                                  |
| --------------------- | --------------------------------------------------------------------------------------- | --------------------------------------------------------------------------------- |
| Auto                  | engine tự chọn kiểu an toàn nhất cho từng ứng dụng dựa trên những gì ứng dụng báo cáo   | đặt một lần rồi thôi (mới từ v1.1.6)                                              |
| Preedit (mặc định)    | từ hiển thị ngay trong dòng với gạch chân, commit khi hết từ                            | tương thích rộng nhất                                                             |
| Non-preedit           | mỗi phím commit ngay, phần đuôi được viết lại khi có dấu                                | Firefox, ứng dụng GTK                                                             |
| Surrounding text      | non-preedit + đối chiếu với báo cáo văn bản của chính ứng dụng                          | Sublime Text                                                                      |
| Smooth (uinput)       | phím thô được chuyển tiếp trong lúc gõ, phần đuôi latin bị xóa bằng Backspace thật khi hết từ | terminal, ứng dụng khó tính (X11)                                            |
| Super Smooth (uinput) | như trên, nhưng bỏ bước kiểm tra an toàn trước khi viết lại                             | gõ nhanh (X11)                                                                    |
| Select (uinput)       | Shift+← chọn phần đuôi, Delete xóa nó, từ được commit                                   | ứng dụng xử lý Backspace sai (X11)                                                |
| ForwardKeyEvent       | non-preedit nhưng phần đuôi được viết lại bằng cách xóa qua sự kiện phím Backspace chuyển tiếp qua IBus | ứng dụng bỏ qua tín hiệu xóa: Chrome, Electron, GNOME Console, Sublime Text |
| ForwardKeyEvent II    | tương tự, nhưng phần đuôi được chọn bằng Shift+← chuyển tiếp và commit sẽ thay thế nó   | ứng dụng xử lý sai Backspace chuyển tiếp, ứng dụng X11/XIM (JetBrains)            |
| OFF                   | tắt tiếng Việt, macro vẫn hoạt động                                                     | phiên gõ trộn tiếng Việt/tiếng Anh                                                 |

Chuyển kiểu từ menu nguồn nhập, cửa sổ cài đặt, một phím tắt xoay vòng có thể gán, hoặc theo
từng ứng dụng bằng **Luật ứng dụng**. Các kiểu uinput cần `/dev/uinput` và sẽ quay về preedit
ở bất cứ đâu việc bơm phím thật không với tới ứng dụng (trên GNOME Wayland chúng chuyển thành
preedit; chúng dành cho phiên X11).

## Yêu cầu

| Distro          | Gói do trình cài một dòng cài                                              |
| --------------- | -------------------------------------------------------------------------- |
| Arch            | `ibus` `gtk4` `libadwaita` `dbus`, `gcc` `make` `pkgconf`                  |
| Debian / Ubuntu | `ibus` `libgtk-4-dev` `libadwaita-1-dev` `libdbus-1-dev`, build-essential  |
| Fedora          | `ibus` `gtk4-devel` `libadwaita-devel` `dbus-devel`, `gcc` `make`          |

Trình cài nhận diện distro (apt / pacman / dnf), chỉ cài những gói còn thiếu và để Rust cho
rustup; cần GTK 4.10+ và libadwaita 1.5+ (Ubuntu 22.04 chỉ có GTK 4.6). Chỉ đường dẫn Arch là
đã được kiểm thử — tên gói Debian/Ubuntu và Fedora là tên chuẩn nhưng chưa được xác minh ở đó.
IBus phải là khung phương thức nhập đang hoạt động (trên GNOME thì mặc định đã là vậy).

## Build

```sh
cargo build --release --features gui
```

Phần lõi engine đến từ crate `bamboo-core` trên crates.io. Có thể xem mã nguồn của bản ghim
`v0.5.3` qua `git submodule update --init` (tùy chọn; bản build dùng crates.io).

## Cài đặt

### Cài một dòng (online)

Tải mã nguồn, build và cài; các phụ thuộc build còn thiếu (IBus, GTK4, toolchain build, Rust)
được cài giúp bạn, nên ban đầu chỉ cần `curl` và `sudo`. Udev rule cho `/dev/uinput` và nhóm
`input` cũng được thiết lập sẵn:

```sh
curl -fsSL https://raw.githubusercontent.com/fundokukiri/ibus-mangrs/master/scripts/install-online.sh | sh
```

(Bơm script vào `sh` nghĩa là chạy code từ mạng — bạn nên đọc qua nó trước nếu điều đó quan
trọng với mình.)

### Từ bản checkout cục bộ

Build, cài và khởi động lại IBus:

```sh
cargo build --release --features gui
sudo ./scripts/install.sh
ibus restart
```

Script cài đặt sao chép engine vào `/usr/lib/ibus-mangrs/`, đăng ký component vào
`/usr/share/ibus/component/`, cài udev rule cho `/dev/uinput` và thêm người dùng của bạn vào
nhóm `input` (thành viên nhóm có hiệu lực ở lần đăng nhập kế tiếp). Gỡ cài đặt:
`sudo ./scripts/uninstall.sh` — giữ lại cấu hình của bạn.

Gói Arch (dùng thư mục udev của hệ thống cho udev rule):

```sh
makepkg -si
```

### Quyền `/dev/uinput` (cách thủ công dự phòng)

Engine nhấn **phím Backspace thật** qua `/dev/uinput` khi một ứng dụng không thể xóa văn bản
qua IBus. Không có quyền ghi, các ứng dụng đó không thể gõ dấu thanh. Tạo
`/etc/udev/rules.d/80-uinput.rules` (kèm sẵn dưới dạng `data/80-uinput.rules`):

```
KERNEL=="uinput", MODE="0660", GROUP="input"
```

Sau đó:

```sh
sudo modprobe uinput                      # nạp module nhân ngay bây giờ
sudo usermod -aG input $USER              # có hiệu lực ở lần đăng nhập kế tiếp
sudo udevadm control --reload-rules && sudo udevadm trigger /dev/uinput
echo uinput | sudo tee /etc/modules-load.d/uinput.conf   # nạp ở mỗi lần khởi động
```

Kiểm tra: `ls -l /dev/uinput` phải hiện `crw-rw---- root input`.

## Bật nguồn nhập

1. `ibus restart` (sau khi cài hoặc nâng cấp binary).
2. **Settings → Keyboard → Input Sources → +** → _Vietnamese_ → **MăngRS**
   (hoặc `ibus engine ibus-mangrs`).
3. `Super+Space` chuyển giữa các nguồn nhập; bánh răng cạnh engine mở cửa sổ cài đặt.

## Sử dụng

| Hành động                 | Cách                                                                                   |
| ------------------------- | -------------------------------------------------------------------------------------- |
| Bật/tắt tiếng Việt        | menu nguồn nhập → _Enable Vietnamese_, hoặc `Super+Space`                              |
| Mở cửa sổ cài đặt         | menu nguồn nhập → _Cài đặt…_, hoặc `ibus-engine-ibus-mangrs -gui`                      |
| Đổi kiểu gõ               | menu → _Input Method_ (9 preset), hoặc GUI                                             |
| Đổi bảng mã               | menu → _Charset_ (11 bảng mã), hoặc GUI                                                |
| Đổi kiểu hiển thị         | menu → _Display_ (4 kiểu theo mặc định; menu tùy chỉnh được), hoặc GUI (10 kiểu), hoặc phím tắt xoay vòng |
| Phím tắt đổi kiểu hiển thị | GUI trang _Phím tắt_ → _Thứ tự khi đổi chế độ_ (vd. `Control+Alt+v`)                  |
| Cài đặt nhanh             | menu → _Quick Settings_ (macro, tự viết hoa, từ viết tắt, khôi phục từ ngoại lai)      |
| Gõ emoji                  | gõ `:name:` (bật ở trang _Emoji_), hoặc phím tắt emoji mở bảng chọn tại con trỏ        |
| Sắp lại menu trên panel   | GUI trang _Menu trên panel_ (thư mục, chức năng, nhóm radio, dấu phân cách)            |
| Gán phím tắt              | GUI trang _Phím tắt_ (14 chức năng — nhấp vào hàng, nhấn tổ hợp)                       |
| Gõ tiếng Anh / code       | cứ gõ bình thường — từ không hợp lệ quay về phím thô; URL/email/đường dẫn giữ nguyên   |
| Kiểu hiển thị theo ứng dụng | GUI trang _Luật theo app_ (xem Luật ứng dụng bên dưới)                               |

## Luật ứng dụng (kiểu hiển thị theo từng ứng dụng)

Một luật ép một kiểu hiển thị cho một ứng dụng và ghi đè kiểu toàn cục. Các luật được đánh
giá theo thứ tự và **luật khớp đầu tiên thắng**, nên hãy đặt các pattern cụ thể phía trên các
pattern tổng quát.

**Trong cửa sổ cài đặt:** trang _Luật theo app_. Chọn một ứng dụng trong ô tên (hoặc chép ứng
dụng đang focus), chọn một kiểu, nhấn _Thêm luật_: luật được lưu cùng phần còn lại của cài
đặt. Mỗi luật là một dòng, kéo để sắp xếp lại (menu ⋮ có Lên/Xuống/Xóa; xóa có toast Hoàn
tác), và _Thử luật_ cho biết một tên ứng dụng khớp luật nào. Dropdown chỉ liệt kê các ứng
dụng engine đã từng thấy (50 mục mới nhất,
`~/.cache/ibus-mangrs/clients`) cùng hai đích giả: `gnome-shell (app Wayland)` — mọi ứng dụng
Wayland khi chưa cài extension focus — và `(app không báo tên)` — các ứng dụng không báo tên
nào cả. Gõ một lần trong ứng dụng để nó xuất hiện trong danh sách.

**Bằng tay:** mỗi luật một khối `[[app_rules]]` trong `~/.config/ibus-mangrs/config.toml`
(nạp lại nóng trong khoảng 1 giây):

```toml
[[app_rules]]
pattern = "chrome"
mode = "preedit"

[[app_rules]]
pattern = "sublime_text"
mode = "surrounding"
```

**Cách khớp:** so khớp chuỗi con không phân biệt hoa thường trên tên ứng dụng; tiền tố
toolkit (`gtk4-im:`, `wayland:`, `x11:`) bị bỏ qua. Tên lấy từ extension focus đi kèm trước,
rồi đến cửa sổ active của X (`xprop`), rồi đến prgname mà IBus báo cáo. Các ứng dụng
X11/XIM (JetBrains và các ứng dụng Java khác) chỉ báo cáo tên cố định `xim`, nên chúng được
định danh từ extension/`xprop` — một luật như `pattern = "jetbrains"` cũng nhắm tới chúng.
Pattern ngắn khớp rộng (`term` cũng khớp `xterm`). `pattern = "none"` nhắm tới các ứng dụng
không có tên nào cả — cách duy nhất để khớp chúng.

**Các kiểu:** `auto`, `preedit` (mặc định), `nonpreedit`, `surrounding`, `smooth`,
`supersmooth`, `select` (ba kiểu này cần `/dev/uinput` và dành cho X11), `forwardkeyevent`,
`forwardselect` (bí danh `forwardkeyevent2`), `off` (config còn giữ `defaulttyping` đã bỏ sẽ
tự chuyển về đây). Hai kiểu forward commit giống
`nonpreedit` nhưng xóa phần đuôi đã viết lại bằng cách chuyển tiếp sự kiện phím qua IBus
thay vì dùng tín hiệu xóa — điều này với tới được các client bỏ qua `DeleteSurroundingText`
(Chrome, Electron, GNOME Console) ngay cả trên Wayland, nơi `/dev/uinput` không với tới
được, và các ứng dụng X11/XIM (JetBrains), vốn không có capability nào trong input context:
`forwardkeyevent` chuyển tiếp Backspace, `forwardselect` chọn phần đuôi bằng Shift+← và để
commit thay thế nó. Chúng chỉ hoạt động nếu ứng dụng chấp nhận sự kiện phím chuyển tiếp, nên
hãy thử theo từng ứng dụng.

**Luật có sẵn**, hiển thị trong ô văn bản khi `config.toml` chưa có khóa `app_rules` (sửa
hoặc xóa tại đó; lưu sẽ ghi chúng ra file): Chrome, Chromium, Brave, Edge, Vivaldi, các ứng
dụng Electron (VS Code, Slack, Discord) và GNOME Console → `preedit`; Sublime Text →
`surrounding`. Các client này bỏ qua `DeleteSurroundingText`, nên việc viết lại tại đó làm
nhân đôi văn bản.

## Tùy chọn X11/XIM

Hai công tắc tương thích trong cửa sổ cài đặt (trang _Tinh chỉnh_ → _Tương thích X11/XIM_). Cả
hai đều **tắt theo mặc định**, và cả hai chỉ cần cho các ứng dụng tiếp cận engine theo cách
bất thường:

| Công tắc                          | `config.toml`               | Tác dụng                                                                                                                                                                                                                                                                                                                                                                                       |
| --------------------------------- | --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Xoá không cần báo cáo surrounding | `x11_delete_without_report` | Cho phép client X11 trực tiếp (Sublime Text, các ứng dụng GTK3 trên X11 khác) xóa phần đuôi đã viết lại qua IBus dù nó chưa từng báo cáo văn bản của mình — như cách ibus-bamboo vẫn làm. Không có nó, các client đó giữ nguyên phím thô khi không thể bơm phím thật (`tungf`). Rủi ro: không có báo cáo, số ký tự xóa không bị kẹp giới hạn, nên con trỏ bị di chuyển sau lưng engine có thể làm mất vài ký tự. |
| Preedit cho app XIM               | `preedit_for_xim`           | Giữ kiểu gạch chân cho các client XIM (JetBrains và các ứng dụng Java khác, vốn đều báo cáo tên cố định `xim`). Máy chủ XIM tự vẽ văn bản đang soạn trong cửa sổ của nó và commit khi hết từ. Không có nó, các ứng dụng đó không thể dùng kiểu mặc định và cần một luật `forwardkeyevent`. Rủi ro: ứng dụng đàm phán `XIMPreeditNothing` sẽ không hiển thị gì cho đến khi từ kết thúc.             |

## File cấu hình

```
~/.config/ibus-mangrs/config.toml     # toàn bộ cài đặt, nạp lại nóng trong khoảng 1 giây
~/.config/ibus-mangrs/macros.toml     # mỗi dòng một key=expansion, $TIME / $DATE
~/.config/ibus-mangrs/emoji.toml      # các cặp name = "glyph", cùng định dạng với macros.toml
~/.config/ibus-mangrs/dictionary.txt  # các từ không bao giờ bị tự khôi phục
```

## Xử lý sự cố

- **Engine không xuất hiện trong Input Sources**: chạy `ibus write-cache --system` rồi
  `ibus-daemon -drx`; chắc chắn `/usr/share/ibus/component/ibus-mangrs.xml` tồn tại.
- **Dấu không xuất hiện trong một số ứng dụng** (Sublime, client XWayland): thiếu hoặc không
  ghi được vào `/dev/uinput` — xem phần quyền ở trên.
- **Đổi kiểu như bị bỏ qua**: một Luật ứng dụng cho ứng dụng đó đang ghi đè kiểu toàn cục.
- **Luật ứng dụng bị bỏ qua với Chrome, Firefox hoặc Sublime Text trên GNOME Wayland**: IBus
  chỉ báo cáo các ứng dụng đó như một mutter relay ẩn danh, nên engine không có tên để khớp.
  Cài extension GNOME Shell đi kèm (`data/gnome-shell-extension/README.md`), rồi đăng xuất và
  đăng nhập lại — sửa hoặc bật tại chỗ là chưa đủ.
- **Trường hợp khác**: bật _Ghi log gỡ lỗi_ (GUI trang _Cài đặt khác_, hoặc
  `debug_log = true` trong `config.toml`), tái hiện sự cố, và đính kèm
  `~/.cache/ibus-mangrs/debug.log` — engine ghi log mọi thay đổi focus, capability, quyết
  định xử lý phím và xóa/bơm phím với dấu thời gian đến mili giây, đối chiếu được với
  `journalctl`. Log đó chỉ ghi quyết định của engine — con trỏ, độ dài, số lần xóa — không
  bao giờ ghi văn bản bạn đã gõ. Để gỡ lỗi mất chữ/sai ký tự còn một công tắc thứ hai, _Log
  cả nội dung đang gõ_ (`debug_log_text`), sẽ thêm nội dung văn bản xung quanh con trỏ và các
  diff verify vào cùng file: chỉ bật khi đang tái hiện, và **không** đính kèm log đó lên issue.

## Kiểm thử

```sh
cargo test                      # 300 unit test, gồm cả bảng golden 894 ca
cargo clippy --all-targets      # (301 với --features gui)
```

Kiểm thử trực tiếp với daemon thật mà không cần cài:

```sh
IBUS_COMPONENT_PATH=$PWD/data:/usr/share/ibus/component ibus-daemon -rx -t refresh
ibus engine ibus-mangrs
```

## Kiến trúc

- `src/server/` — D-Bus, message filter, menu panel, phím tắt, theo dõi cấu hình, các key transport, bảng chọn luật theo app và bảng chọn emoji (IBus lookup table ngay tại con trỏ)
- `src/engine/` — máy trạng thái: kiểu hiển thị, Luật ứng dụng, macro, khôi phục, xác minh/sửa chữa
- `src/bamboo_wrapper.rs` — nơi duy nhất chạm vào `bamboo-core`; tự tính delta đầu ra
- `src/uinput.rs` — bàn phím ảo: Backspace có nhịp, chọn vùng, Ctrl+V (bảng phím US-QWERTY)
- `src/paste.rs` — kênh Unicode qua clipboard cho các kiểu uinput (khôi phục có debounce)
- `src/macros.rs` — bảng macro + từ điển tùy chọn
- `src/emoji.rs` — các bảng shortcode: bảng nhúng và `emoji.toml` của người dùng
- `src/backup.rs` — sao lưu/khôi phục trong một file JSON: cài đặt, macro, từ điển, bảng emoji
- `src/golden.rs` — 894 ca golden phím→văn bản
- `src/ibus_prop.rs` — mã hóa menu property của IBus; `src/keymap.rs` — giải mã keysym
- `src/debug_log.rs` — log gỡ lỗi hai tầng tại `~/.cache/ibus-mangrs/debug.log` (mặc định chỉ ghi quyết định; văn bản gõ nằm sau một công tắc thứ hai)
- `src/config.rs` — cài đặt TOML dùng chung bởi engine + GUI; `src/gui/` — cửa sổ cài đặt libadwaita, 11 trang (feature `gui`)

## Giấy phép

- [**MăngRS**](https://github.com/fundokukiri/ibus-mangrs) được cấp phép theo
  [GPL-3.0 License](LICENSE).
- [**IBus Bamboo:**](https://github.com/BambooEngine/ibus-bamboo) Copyright (C) 2018 Luong
  Thanh Lam. [GPL-3.0 License](https://github.com/bambooengine/ibus-bamboo?tab=GPL-3.0-1-ov-file)
- [**bamboo-core (Rust):**](https://github.com/bamboo-vi/bamboo-core) thư viện thuật toán gõ
  tiếng Việt viết bằng Rust. [MIT License](https://github.com/bamboo-vi/bamboo-core?tab=MIT-1-ov-file)
- [**bamboo-core (Go):**](https://github.com/BambooEngine/bamboo-core) Copyright (C) 2018
  Luong Thanh Lam. [MIT License](https://github.com/BambooEngine/bamboo-core?tab=MIT-1-ov-file)
