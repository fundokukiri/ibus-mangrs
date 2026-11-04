//! Page "Giới thiệu": feature highlights, project links and the standard
//! GNOME about dialog. The logo itself comes from the installed hicolor
//! icon (`ibus-mangrs`, the same logo.png install.sh ships) — an embedded
//! copy would only duplicate it.

use super::*;

pub(crate) fn build() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let features = adw::PreferencesGroup::builder()
        .title("Đặc điểm nổi bật")
        .description("Bộ gõ tiếng Việt cho lập trình viên và mọi ứng dụng — Rust + bamboo-core")
        .build();
    features.add(&feature_row(
        "input-keyboard-symbolic",
        "10 chế độ hiển thị",
        "Preedit, Non-preedit, Surrounding + 3 mode uinput + 2 mode ForwardKeyEvent, OFF, Gõ mặc định",
    ));
    features.add(&feature_row(
        "network-workgroup-symbolic",
        "URL thông minh",
        "Từ ngay sau / : @ . _ ~ = # & % ? giữ nguyên, không thêm dấu",
    ));
    features.add(&feature_row(
        "applications-system-symbolic",
        "App Rules theo ứng dụng",
        "Mỗi ứng dụng một chế độ — đổi bằng menu, hotkey hoặc cửa sổ này",
    ));
    features.add(&feature_row(
        "insert-text-symbolic",
        "Gõ tắt & macro",
        "$TIME/$DATE, viết hoa thông minh, không kích hoạt nhầm từ Việt thật",
    ));
    page.add(&features);

    let project = adw::PreferencesGroup::builder()
        .title("Thông tin dự án")
        .build();
    let source = action_row(
        "Mã nguồn GitHub",
        "https://github.com/fundokukiri/ibus-mangrs",
    );
    source.add_suffix(&link_button(
        "Mở",
        "https://github.com/fundokukiri/ibus-mangrs",
    ));
    project.add(&source);
    project.add(&feature_row(
        "avatar-default-symbolic",
        "Tác giả",
        "Phùng Đức Tùng (Fundoku)",
    ));
    let homepage = action_row("Trang chủ", "https://mangrs.fundoku.io.vn");
    homepage.add_suffix(&link_button("Mở", "https://mangrs.fundoku.io.vn"));
    project.add(&homepage);
    project.add(&feature_row(
        "help-about-symbolic",
        "Giấy phép mã nguồn mở",
        "GPL — phiên bản 3, chi tiết trong LICENSE",
    ));
    project.add(&feature_row(
        "emblem-favorite-symbolic",
        "Credits",
        "Lõi gõ bamboo-core (MIT, bên thứ 3)",
    ));
    let detail_row = action_row("Giới thiệu chi tiết", "Hộp thoại about chuẩn GNOME");
    let detail_btn = gtk4::Button::from_icon_name("help-about-symbolic");
    detail_btn.add_css_class("flat");
    detail_row.add_suffix(&detail_btn);
    project.add(&detail_row);
    page.add(&project);

    detail_btn.connect_clicked(move |btn| {
        let dialog = adw::AboutDialog::builder()
            .application_name("MăngRS IBus")
            .application_icon("ibus-mangrs")
            .version(env!("CARGO_PKG_VERSION"))
            .developer_name("Phùng Đức Tùng (Fundoku)")
            .website("https://github.com/fundokukiri/ibus-mangrs")
            .issue_url("https://github.com/fundokukiri/ibus-mangrs/issues")
            .license_type(gtk4::License::Gpl30)
            .build();
        dialog.present(btn.native().as_ref());
    });

    page
}

/// One read-only info row with a leading symbolic icon.
fn feature_row(icon: &str, title: &str, subtitle: &str) -> adw::ActionRow {
    let row = action_row(title, subtitle);
    let image = gtk4::Image::from_icon_name(icon);
    image.add_css_class("dim-label");
    row.add_prefix(&image);
    row
}
