# Tính năng và hướng dẫn sử dụng — MăngRS

Cài đặt và xử lý sự cố: [README.md](../README.md). Bản tiếng Anh: [FEATURES-en.md](FEATURES-en.md).

## Các chế độ gõ

Năm chế độ, mỗi chế độ một hợp đồng riêng (giá trị config trong ngoặc; mặc định nhà máy đầu
tiên):

| Chế độ (giá trị config)     | Cách hoạt động                                                                                              |
| --------------------------- | ----------------------------------------------------------------------------------------------------------- |
| **Gạch chân** (`preedit`, mặc định) | chữ hiện ngay tại con trỏ có gạch chân, hết từ thì vào văn bản; click hay đổi cửa sổ không mất chữ  |
| **Surrounding text** (`surrounding`) | phím nào vào ngay phím đó; đuôi có dấu được xoá viết lại qua `DeleteSurroundingText`, có đối chiếu với chữ app báo về để tự sửa |
| **Flow** (`flow`)           | phím thô được gửi thẳng khi gõ; mỗi phím đổi composition được áp dụng ngay thành mini-burst (chữ có dấu hiện từng phím); hết từ thì từ hoàn chỉnh vào văn bản |
| **Forward Backspace** (`forwardbackspace`) | phím nào vào ngay phím đó; phần xoá đuôi đi bằng sự kiện phím Backspace gửi qua IBus; không bao giờ rơi về preedit |
| **OFF** (`off`)             | tắt xử lý tiếng Việt; gõ tắt và tự viết hoa vẫn chạy                                                        |

Đổi chế độ ở menu bộ gõ, hàng _Chế độ gõ_ trong cửa sổ cài đặt, phím tắt xoay vòng, hoặc theo
từng app qua **Luật theo app**.

- **Flow** cần một backend bơm phím hoạt động (kernel `/dev/uinput` mặc định, hoặc giao thức
  virtual-keyboard của compositor trên Hyprland/wlroots); không có thì **Forward Backspace**
  gánh thay và engine tự xin lại backend sau một khoảng chờ.
- **Forward Backspace** không cần gì ngoài IBus: phần xoá đi bằng sự kiện phím gửi tiếp, nên
  chạy được trên GNOME Wayland (nơi cấm bơm phím) và app XIM.
- Config hay luật còn giữ giá trị cũ tự chuyển khi nạp: `nonpreedit` → `surrounding`,
  `turbo`/`select`/`smoothforward` → `flow`, `forwardselect` và chính tả cũ `forwardkeyevent`
  → `forwardbackspace`, `defaulttyping` → `off`, `auto` → `preedit`. File cũ vẫn nạp, không có
  gì bị ghi đè ngầm.

### Hiện/ẩn chế độ gõ

Trang *Chung* của cửa sổ cài đặt có một công tắc cho từng chế độ (_Chế độ hiển thị_). Chế độ
bị ẩn không chọn được ở đâu cả: rút khỏi hàng chế độ, bảng chọn luật theo app, menu panel và
phím tắt xoay vòng, và engine không áp dụng nó làm chế độ chung hay chế độ luật. Cả năm chế độ
mặc định đều hiện. Danh sách lưu trong `config.toml`:

```toml
visible_modes = ["preedit", "surrounding", "flow", "forwardbackspace", "off"]
```

File không có khoá này (nâng cấp từ bản cũ) giữ nguyên mọi chế độ đang dùng — chế độ chung và
mọi chế độ luật được nối vào danh sách mặc định, không có gì bị ẩn ngầm.

## Kiểu gõ và bảng mã

**9 kiểu gõ sẵn** — Telex (`tieengs` → `tiếng`), Telex 2, Telex W (`asz` → `a`), Telex + VNI,
Telex + VNI + VIQR, VNI (`tie6ng1`), VNI bàn phím Pháp, VIQR và bố cục Microsoft
(`ti3ng8` → `tiếng`).

**11 bảng mã xuất ra** — Unicode (mặc định), VNI, VIQR, TCVN3, VISCII, VPS, CP1258, BKHCM,
Vietware X, Vietware Full, NFD. Chỉ văn bản gửi vào app được encode; bảng mã legacy đổi cả số
lượng ký tự nên cả từ được viết lại với số lần xoá đúng.

## Tùy chọn gõ

- Dấu kiểu mới `hòa` hay kiểu cũ `hoà` (*Kiểu gõ dấu hiện đại*).
- Gõ dấu tự do (`hoangf` → `hoàng`) — dấu đặt sai vị trí vẫn nhận.
- Tự sửa âm tiết sai (invalid syllable quay về phím gốc, trừ từ còn mở như `đuo`).
- Tự buông từ tiếng nước ngoài (`windowf` giữ nguyên) — từ điển cá nhân không bao giờ bị tự sửa.
- Giữ nguyên phím gõ sau URL/email/đường dẫn (`example.tieeng` không thành `example.tiêng`);
  chỉ URL thật (`https://…`, `www.`, `word://`) mới bật chế độ giữ nguyên, gồm cả query và
  fragment. Thêm ký tự giữ nguyên riêng trong trang *Thử nghiệm*.
- `w` → `ư`, `[` `]` → `ơ` `ư`, `--` → `—`, tự viết hoa sau `. ! ?` và Enter, hai dấu cách
  thành dấu chấm.

## Backspace và xoá dấu

Dropdown *Xóa dấu bằng Backspace* (trang *Dấu & tự động*) quyết định Backspace xử lý từ đang
có dấu thế nào:

| Kiểu | Hành vi | Ví dụ |
| --- | --- | --- |
| **Xóa dấu trước** (mặc định) | một Backspace bỏ cả dấu | `hạ` → `ha` → `h` |
| **Xóa cả dấu và chữ** | xoá ký tự cuối trên màn hình | `việt` → `việ` → `vi` |
| **Unikey (lùi từng phím)** | hoàn tác phím gõ cuối | `hạ` `hã` `hả` `hà` `há` `ha` |

Ở **Flow**, xóa vào phần kết thúc của từ đã hoàn thành là xoá thuần — **một Backspace xoá
đúng một ký tự**, không bơm phím nào. Xoá dấu cách sau `chào ` thì `chào` quay lại nguyên
trạng, **kể cả dấu**, để sửa tiếp; Backspace tiếp theo mới bỏ dấu theo dropdown. Xoá một từ
gõ sai phía trước từ đã xong không còn làm mất chữ của từ đó (`chịu t` + 2 Backspace giữ
nguyên `chịu`).

## Chữ hiện từng phím ở Flow

Luôn bật (không còn công tắc): gõ `ee` thấy ngay `ê` thay vì đợi hết từ; độ trễ chỉnh được ở
hàng _Hiện dấu từng bước_ (mặc định 20 ms). Click chuột hay đổi cửa sổ không mất chữ đang gõ
dở — preedit vào văn bản đúng như đang hiện, kể cả chỗ sửa tắt như `ddc` → `đc`.

## Gõ tắt và từ điển

- **Gõ tắt** (`macros.toml`): mỗi dòng `key=nội dung`, hỗ trợ `$TIME` / `$DATE` và viết hoa
  thông minh. Từ khoá trùng từ tiếng Việt thật chỉ nở khi bật *Gõ tắt nở cả trên từ hợp lệ*
  (trang *Thử nghiệm*).
- **Từ điển cá nhân** (`dictionary.txt`): mỗi dòng một từ, không bao giờ bị tự buông về phím gốc.

## Emoji

- Gõ `:tên:` ra emoji (bật trong trang *Emoji*, mặc định tắt) — 1.940 emoji tìm được bằng cả
  tiếng Việt lẫn tiếng Anh; chỉ dấu `:` kết thúc mới quyết định, nên chữ gõ sau `:` không mất dấu.
- Bảng chọn emoji tại con trỏ qua phím tắt: các lựa chọn gần đây đứng trước, gõ chữ để tìm,
  số để chọn, Enter đưa ký tự vào văn bản.
- Bộ riêng của bạn dạng `tên = "ký tự"` trong `~/.config/ibus-mangrs/emoji.toml`.

## Luật theo app

Một luật ép một chế độ gõ cho một ứng dụng và đè lên chế độ chung; luật nào khớp trước ăn
trước nên xếp luật cụ thể lên trên luật chung. Tên app khớp theo chuỗi con không phân biệt
hoa/thường (`term` ăn cả `xterm`); pattern đặc biệt `none` nhắm những app không bao giờ báo tên.
Làm trong GUI trang *Luật theo app* (chọn app, chọn chế độ), qua phím tắt bảng chọn luật, hoặc
sửa tay trong `~/.config/ibus-mangrs/config.toml` (sửa là áp dụng sau khoảng 1 giây):

```toml
[[app_rules]]
pattern = "chrome"
mode = "preedit"

[[app_rules]]
pattern = "sublime_text"
mode = "surrounding"
```

Giá trị `mode`: `preedit` (mặc định), `surrounding`, `flow`, `forwardbackspace`, `off` — giá
trị cũ vẫn nạp và tự chuyển về giá trị mới. Luật trỏ tới chế độ đang bị ẩn được giữ trong file
nhưng bị bỏ qua cho tới khi chế độ đó hiện lại.

Bộ gõ không mang luật nào sẵn. Engine có sẵn cơ chế bảo vệ cho các client hay nuốt thao tác
xoá (Chrome, Chromium, Edge, Brave, Vivaldi và các app Electron như VS Code, Slack, Discord):
nếu một chế độ có xoá-ghi-lại đang bật ở đó, engine gõ bằng preedit thay vì để chữ bị lặp.
Muốn app đó dùng chế độ khác, tự thêm luật.

Chrome và Firefox trên GNOME Wayland chỉ hiện với IBus như một relay vô danh: muốn đặt luật
cho chúng phải cài kèm GNOME Shell extension (`../data/gnome-shell-extension/README.md`) rồi
đăng xuất đăng nhập lại.

## Phím tắt

Cả bốn khe phím tắt có sẵn (bật/tắt tiếng Việt, xoay vòng chế độ gõ, bảng chọn luật theo app,
bảng emoji) mặc định chưa gán phím — đặt trên trang *Phím tắt*. Ngoài ra **14 chức năng** gán
phím được: đổi kiểu gõ, xoay bảng mã, mở cài đặt ở trang cụ thể, và các công tắc chữ
(`--`, hai dấu cách, tự viết hoa...). `Super+Space` là phím đổi nguồn nhập của GNOME, không
phải phím tắt của MăngRS.

## Menu trên panel và cửa sổ cài đặt

- **Menu trên panel sửa được** (trang *Menu panel*): thư mục (một cấp), chức năng, ba nhóm
  chọn (Bảng mã / Kiểu gõ / Chế độ gõ), đường phân cách, sắp xếp, đổi tên, đặt lại về menu
  mặc định; thiết kế lưu trong `config.toml` và tới panel trong khoảng một giây.
- **Cửa sổ cài đặt GTK4 + libadwaita** (12 trang, tiếng Việt hoặc tiếng Anh — hàng ở _Cài đặt
  khác_ đổi ngôn ngữ): sửa gì cũng áp dụng sau khoảng 1 giây, không cần restart IBus, và toàn
  bộ thiết lập sao lưu ra một tệp JSON duy nhất.
