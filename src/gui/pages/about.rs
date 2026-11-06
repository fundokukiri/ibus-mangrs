//! Page "Giới thiệu": the logo hero, project links and the standard GNOME
//! about dialog. The hero uses the embedded logo (the same logo.png that
//! install.sh ships into hicolor); the dialog shows the installed icon.
//! Link rows open the URL themselves (the flat link button is gone); the
//! non-link rows carry no suffix and never look clickable.

use super::*;

/// The first icon the theme actually has, in preference order — the
/// page's prefix/suffix set must never render a missing-icon glyph.
fn themed_icon(names: &[&str]) -> String {
    match gtk4::gdk::Display::default() {
        Some(d) => {
            let theme = gtk4::IconTheme::for_display(&d);
            names
                .iter()
                .find(|n| theme.has_icon(n))
                .map(|n| n.to_string())
                .unwrap_or_else(|| names[names.len() - 1].to_string())
        }
        // No display in headless tests: the first name compiles fine and
        // is only ever asked for on a real display.
        None => names[0].to_string(),
    }
}

/// One read-only info row with a leading dim symbolic icon.
fn feature_row(icon: &str, title: &str, subtitle: &str) -> adw::ActionRow {
    let row = action_row(title, subtitle);
    let image = gtk4::Image::from_icon_name(icon);
    image.add_css_class("dim-label");
    row.add_prefix(&image);
    row
}

/// One link row: activatable, opens the URL in the default browser; the
/// suffix is a dim external-link glyph, the tooltip the URL itself, and
/// the accessible description names it an external link.
fn link_row(prefix: &str, title: &str, subtitle: &str, url: &str) -> adw::ActionRow {
    let row = action_row(title, subtitle);
    row.set_activatable(true);
    let prefix_image = gtk4::Image::from_icon_name(prefix);
    prefix_image.add_css_class("dim-label");
    row.add_prefix(&prefix_image);
    let image = gtk4::Image::from_icon_name(&themed_icon(&[
        "adw-external-link-symbolic",
        "external-link-symbolic",
        "go-next-symbolic",
    ]));
    image.add_css_class("dim-label");
    row.add_suffix(&image);
    row.set_tooltip_text(Some(url));
    row.update_property(&[gtk4::accessible::Property::Description(&format!(
        "{title}, liên kết ngoài"
    ))]);
    let url = url.to_string();
    row.connect_activated(move |row| {
        gtk4::UriLauncher::new(&url).launch(
            row.root().and_downcast::<gtk4::Window>().as_ref(),
            None::<&gtk4::gio::Cancellable>,
            |_| {},
        );
    });
    row
}

pub(crate) fn build() -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let app_name = t("about.name", "MăngRS IBus");
    let source_url = t(
        "about.source_url",
        "https://github.com/fundokukiri/ibus-mangrs",
    );
    let author_url = t("about.author_url", "https://fundoku.io.vn");
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
        image.set_pixel_size(176);
        hero.append(&image);
    }
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
    project.add(&link_row(
        &themed_icon(&["utilities-terminal-symbolic", "text-x-generic-symbolic"]),
        &t("about.source_title", "Mã nguồn GitHub"),
        &source_url,
        &source_url,
    ));
    project.add(&link_row(
        &themed_icon(&["avatar-default-symbolic"]),
        &t("about.author_title", "Tác giả"),
        &t("about.author_value", "Phùng Đức Tùng (Fundoku)"),
        &author_url,
    ));
    project.add(&link_row(
        &themed_icon(&["web-browser-symbolic", "network-workgroup-symbolic"]),
        &t("about.homepage_title", "Trang chủ"),
        &homepage_url,
        &homepage_url,
    ));
    project.add(&feature_row(
        "application-certificate-symbolic",
        &t("about.license_title", "Giấy phép mã nguồn mở"),
        &t(
            "about.license_value",
            "GPL — phiên bản 3, chi tiết trong LICENSE",
        ),
    ));
    project.add(&feature_row(
        "emote-love-symbolic",
        &t("about.credits_title", "Ghi công"),
        &t(
            "about.credits_value",
            "Lõi gõ bamboo-core (MIT, bên thứ ba)",
        ),
    ));
    let detail_row = action_row(
        &t("about.detail_title", "Giới thiệu chi tiết"),
        &t("about.detail_subtitle", "Hộp thoại about chuẩn GNOME"),
    );
    let detail_icon = gtk4::Image::from_icon_name("dialog-information-symbolic");
    detail_icon.add_css_class("dim-label");
    detail_row.add_prefix(&detail_icon);
    let go_next = gtk4::Image::from_icon_name("go-next-symbolic");
    go_next.add_css_class("dim-label");
    detail_row.add_suffix(&go_next);
    detail_row.set_activatable(true);
    project.add(&detail_row);
    page.add(&project);

    detail_row.connect_activated(move |row| {
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
        dialog.present(row.native().as_ref());
    });

    page
}
