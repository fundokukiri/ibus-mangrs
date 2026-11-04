//! Page "Giới thiệu": the logo hero, project links and the standard GNOME
//! about dialog. The hero uses the embedded logo (the same logo.png that
//! install.sh ships into hicolor); the dialog shows the installed icon.

use super::*;

pub(crate) fn build() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let app_name = t("about.name", "MăngRS IBus");
    let source_url = t(
        "about.source_url",
        "https://github.com/fundokukiri/ibus-mangrs",
    );
    let homepage_url = t("about.homepage_url", "https://mangrs.fundoku.io.vn");

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
    let name = gtk4::Label::new(Some(&app_name));
    name.add_css_class("title-1");
    name.set_halign(gtk4::Align::Center);
    hero.append(&name);
    let version = gtk4::Label::new(Some(&format!(
        "Phiên bản {} • {}",
        env!("CARGO_PKG_VERSION"),
        t("about.tagline", "Rust + bamboo-core")
    )));
    version.add_css_class("dim-label");
    version.set_halign(gtk4::Align::Center);
    hero.append(&version);
    hero_group.add(&hero);
    page.add(&hero_group);

    let project = adw::PreferencesGroup::builder()
        .title(t("about.group_title", "Thông tin dự án"))
        .build();
    let source = feature_row(
        "text-x-generic-symbolic",
        &t("about.source_title", "Mã nguồn GitHub"),
        &source_url,
    );
    source.add_suffix(&link_button(&source_url));
    project.add(&source);
    let author = feature_row(
        "avatar-default-symbolic",
        &t("about.author_title", "Tác giả"),
        &t("about.author_value", "Phùng Đức Tùng (Fundoku)"),
    );
    author.add_suffix(&link_button(&t(
        "about.author_url",
        "https://fundoku.io.vn",
    )));
    project.add(&author);
    let homepage = feature_row(
        "web-browser-symbolic",
        &t("about.homepage_title", "Trang chủ"),
        &homepage_url,
    );
    homepage.add_suffix(&link_button(&homepage_url));
    project.add(&homepage);
    project.add(&feature_row(
        "application-certificate-symbolic",
        &t("about.license_title", "Giấy phép mã nguồn mở"),
        &t(
            "about.license_value",
            "GPL — phiên bản 3, chi tiết trong LICENSE",
        ),
    ));
    project.add(&feature_row(
        "package-x-generic-symbolic",
        &t("about.credits_title", "Credits"),
        &t("about.credits_value", "Lõi gõ bamboo-core (MIT, bên thứ 3)"),
    ));
    let detail_row = action_row(
        &t("about.detail_title", "Giới thiệu chi tiết"),
        &t("about.detail_subtitle", "Hộp thoại about chuẩn GNOME"),
    );
    let detail_btn = gtk4::Button::from_icon_name("help-about-symbolic");
    detail_btn.add_css_class("flat");
    detail_row.add_suffix(&detail_btn);
    project.add(&detail_row);
    page.add(&project);

    detail_btn.connect_clicked(move |btn| {
        let dialog = adw::AboutDialog::builder()
            .application_name(&app_name)
            .application_icon("ibus-mangrs")
            .version(env!("CARGO_PKG_VERSION"))
            .developer_name(t("about.author_value", "Phùng Đức Tùng (Fundoku)"))
            .website(&source_url)
            .issue_url(t(
                "about.issue_url",
                "https://github.com/fundokukiri/ibus-mangrs/issues",
            ))
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
