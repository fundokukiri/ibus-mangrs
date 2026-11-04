//! Page "Giới thiệu": the logo hero, project links and the standard GNOME
//! about dialog. The hero uses the embedded logo (the same logo.png that
//! install.sh ships into hicolor); the dialog shows the installed icon.

use super::*;

pub(crate) fn build() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // The hero lives in a title-less group so the page keeps its centered
    // column; the boxed look around it matches the GNOME About layout.
    let hero_group = adw::PreferencesGroup::new();
    let hero = gtk4::Box::new(Orientation::Vertical, 6);
    hero.set_halign(gtk4::Align::Center);
    hero.set_margin_top(18);
    hero.set_margin_bottom(6);
    const LOGO: &[u8] = include_bytes!("../../../data/logo.png");
    if let Ok(texture) = gtk4::gdk::Texture::from_bytes(&gtk4::glib::Bytes::from_static(LOGO)) {
        let image = gtk4::Image::from_paintable(Some(&texture));
        image.set_pixel_size(128);
        hero.append(&image);
    }
    let name = gtk4::Label::new(Some("MăngRS IBus"));
    name.add_css_class("title-1");
    name.set_halign(gtk4::Align::Center);
    hero.append(&name);
    let version = gtk4::Label::new(Some(concat!(
        "Phiên bản ",
        env!("CARGO_PKG_VERSION"),
        " • Rust + bamboo-core"
    )));
    version.add_css_class("dim-label");
    version.set_halign(gtk4::Align::Center);
    hero.append(&version);
    hero_group.add(&hero);
    page.add(&hero_group);

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
