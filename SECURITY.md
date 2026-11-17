# Bảo mật

**Tiếng Việt** | [English](docs/SECURITY-en.md)

MăngRS là một input engine: nó thấy mọi phím gõ vào ứng dụng và có thể đưa chữ cùng sự kiện
phím vào cửa sổ đang focus. File này nói rõ nó làm gì với vị trí đó, nó từ chối làm gì, và
ranh giới tin cậy nằm ở đâu.

## Ranh giới tin cậy

Ranh giới là **tài khoản người dùng**. Engine chạy dưới quyền người dùng đang đăng nhập
(được `ibus-daemon` khởi chạy), không bao giờ chạy root và không qua binary setuid. Nó không
cố phòng thủ trước mã chạy cùng user, vì mã đó vốn đã có những kênh mà không input engine
nào chặn được:

- đăng ký một IBus engine rồi chuyển sang nó (`RegisterComponent`, `SetGlobalEngine`) — khi
  đó nó nhận mọi phím gõ và đưa được chữ vào văn bản;
- `XTEST` / `XRecord` trên X11 — bơm phím và ghi phím từ bất kỳ X client nào;
- giao diện văn bản `AT-SPI` trên Wayland — chèn chữ vào ứng dụng GTK;
- sửa `~/.config/ibus-mangrs/*.toml`, mà engine nạp lại trong khoảng một giây.

Mục tiêu thiết kế theo đó: engine không cần, và không xin, một quyền lớn hơn những gì các
kênh trên đã cho một tiến trình thường của tài khoản.

## Engine làm được gì

| Hành động | Cơ chế | Giới hạn |
| --- | --- | --- |
| Bơm sự kiện phím | một trong các backend chọn theo phiên — `/dev/uinput` (thiết bị kernel), XTest (X11), giao thức virtual-keyboard của wlroots (Hyprland/wlroots), hoặc libei qua API trực tiếp của mutter (GNOME) — xem `ARCHITECTURE.md` phần "Injection transports" | mọi backend bị engine khống chế cùng một whitelist: **chỉ Backspace và Delete** — chữ luôn đi qua IBus, không bao giờ qua bơm phím. Trên `/dev/uinput` kernel tự thực thi giới hạn (chỉ đăng ký các keycode đó); trên các backend giao thức, bộ phương thức của trait là lớp thực thi |
| Đưa chữ vào văn bản | IBus `CommitText` tới input context đang focus | chỉ client đang focus |
| Ghi file | `~/.config/ibus-mangrs/`, `~/.cache/ibus-mangrs/`, các file xuất từ cửa sổ cài đặt | không đụng `/usr`, `/etc`, hay home của người khác |

Engine không mở kết nối mạng nào. Log gỡ lỗi (`debug_log = true`) mặc định tắt, được tạo với
quyền `0600`; tier riêng cho nội dung gõ (`debug_log_text`) mới có thể chứa những gì bạn gõ.

## libei trên GNOME

Trên GNOME Wayland, backend EI (Emulated Input) nói chuyện với mutter qua API phiên
`org.gnome.Mutter.RemoteDesktop` của chính compositor; nó được đăng ký nhưng không bao giờ
tự được chọn ở đó cho tới khi có cách nhận phiên không hiện chỉ báo và mutter ngừng segfault
khi EI client biến mất (15/11: phiên do engine giữ làm đèn "đang chia sẻ màn hình" sáng và
hai lần làm sập shell). Đo trên GNOME 51: API đó cấp quyền bơm phím cho bất kỳ client
session-bus nào mà không cần hộp thoại đồng ý — compositor tin các tiến trình của phiên,
đúng ranh giới tin cậy file này đã giả định (engine chạy dưới quyền người dùng đăng nhập,
và một tiến trình như vậy vốn có thể dùng XTEST/XRecord trên X11 hay `/dev/uinput` với ACL
uaccess). Những gì engine thêm trên đó là cùng whitelist với mọi backend khác: chỉ Backspace
và Delete, thực thi bởi bộ phương thức của trait — không lời gọi backend nào gõ được chữ.
Thư viện `libei.so.1` được nạp lúc chạy (dlopen): hệ thống thiếu nó vẫn build và chạy engine
bình thường, backend libei đơn giản không bao giờ kích hoạt.

## Quyền truy cập `/dev/uinput`

Udev rule `70-uinput.rules` gắn tag `uaccess` cho thiết bị. `systemd-logind` cấp POSIX ACL
cho user của **phiên đang hoạt động** và thu hồi khi phiên kết thúc:

```
KERNEL=="uinput", SUBSYSTEM=="misc", OPTIONS+="static_node=uinput", TAG+="uaccess"
```

Không dùng nhóm nào. Bản cài đặt cũ từng thêm user vào nhóm `input` — nhóm đó cho quyền đọc
mọi `/dev/input/event*` trên máy, tức một keylogger toàn hệ thống, đọc được cả trên màn hình
khoá, và sống sót qua lần đăng xuất. Installer hiện gỡ membership đó và xoá `80-uinput.rules`
còn sót; udev đọc mọi file trong `rules.d` nên một bản copy cũ sẽ giữ quyền nhóm sống sót
ngay cạnh rule mới.

`/dev/uinput` được mở lười và thử lại mỗi hai giây khi chưa mở được, vì logind có thể cấp ACL
một lúc sau khi engine khởi động. Nếu nó không bao giờ khả dụng, kiểu gõ commit-as-you-type
rơi về xoá qua protocol ở app tự báo cáo chữ, còn lại là phím Backspace gửi tiếp qua IBus —
không bao giờ fail âm thầm.

## Rủi ro còn lại (đã chấp nhận)

1. **ACL là khả năng theo user, không theo tiến trình.** Bất kỳ tiến trình cùng tài khoản
   đều mở được `/dev/uinput` và tạo bàn phím ảo riêng, chuột gồm cả trong đó. Cùng lớp phơi
   nhiễm với các kênh IBus và X11 phía trên.
2. **ACL vẫn còn khi phiên bị khoá** (phiên khoá vẫn là phiên active), nên phím bơm có thể
   tới màn hình khoá hoặc ô mật khẩu. Bản thân engine không làm gì khi không có composition
   cần viết lại, nhưng khả năng thì tồn tại.
3. **Bơm phím theo focus.** Các rewrite có pacing trải trong vài millisecond; focus đổi giữa
   chừng thì phần phím còn lại rơi vào cửa sổ mới. Vì vậy engine xác nhận client nó ghi vào
   và bỏ qua rewrite khi không xác nhận được.
4. **Tài khoản bị xâm phạm tức engine bị xâm phạm.** Cấu hình, binary engine và log gỡ lỗi
   đều ghi được bởi user, do thiết kế.
5. **X11 không có cách ly theo client** (`XTEST`, `XRecord`); phần hardening ở đây chỉ có ý
   nghĩa trên phiên Wayland.

## Tự kiểm tra trên máy của bạn

```sh
# 1. không còn membership nhóm input (không in gì, exit code 1)
id -nG | tr ' ' '\n' | grep -x input; echo "exit=$?"

# 2. các bàn phím không đọc được bởi tài khoản
head -c 1 /dev/input/event0          # Permission denied

# 3. ACL của phiên tồn tại cho bạn và cho brltty (người dùng thật của thiết bị)
getfacl -p /dev/uinput | grep -E 'user:'

# 4. thiết bị chỉ đăng ký hai phím (bit 14, 111)
for f in /sys/class/input/event*/device/name; do
    n=$(cat "$f" 2>/dev/null)
    case "$n" in *mangrs*) cat "${f%name}capabilities/key";; esac
done
# mong đợi: 800000000000 4000
```

Một lần khởi động engine bình thường ghi `engine start uinput=true` và không có cảnh báo nào
về nhóm `input` (`debug_log = true`).

## Thu hồi quyền

```sh
# gỡ ACL của phiên và ngừng dùng thiết bị
sudo rm /etc/udev/rules.d/70-uinput.rules
sudo udevadm control --reload-rules && sudo udevadm trigger --name-match=/dev/uinput
ibus restart
```

Các chế độ Preedit và Forward Backspace không đụng vào thiết bị; thiếu nó, Surrounding rơi về
xoá qua protocol hoặc phím Backspace gửi tiếp, còn Flow rơi về Forward Backspace — engine
vẫn gõ được.

## Báo cáo lỗ hổng

Mở một private security advisory trên repository GitHub
(`https://github.com/fundokukiri/ibus-mangrs`) thay vì issue công khai. Kèm phiên bản, distro
và desktop, chế độ đang dùng, và một debug log bật `debug_log = true` (để `debug_log_text`
tắt trừ khi báo cáo cần nội dung gõ).
