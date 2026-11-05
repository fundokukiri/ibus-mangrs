//! Page "Chung": typing on/off, input style and the preedit underline —
//! the things users touch every day. The keyboard shortcuts live on their
//! own "Phím tắt" page (see hotkeys.rs).

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let status = adw::PreferencesGroup::builder()
        .title(t("groups.typing.status.title", "Trạng thái"))
        .description(t("groups.typing.status.description", "Bật tiếng Việt"))
        .build();
    status.add(&ui.enabled);
    page.add(&status);

    let style = adw::PreferencesGroup::builder()
        .title(t("groups.typing.style.title", "Cách gõ"))
        .description(t(
            "groups.typing.style.description",
            "Kiểu gõ, bảng mã và chế độ gõ",
        ))
        .build();
    style.add(&ui.method);
    style.add(&ui.charset);
    style.add(&ui.mode);
    page.add(&style);

    let display = adw::PreferencesGroup::builder()
        .title(t("groups.typing.display.title", "Hiển thị"))
        .description(t(
            "groups.typing.display.description",
            "Hình thức của chữ đang gõ",
        ))
        .build();
    display.add(&ui.hide_underline);
    page.add(&display);

    page
}
