//! Page "Gõ tắt": the macro switches, the $TIME/$DATE formats and the
//! shortcut list itself (one editable row per entry).

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> (adw::PreferencesPage, gtk4::Button) {
    let page = adw::PreferencesPage::new();

    let settings = adw::PreferencesGroup::builder()
        .title("Gõ tắt")
        .description("Mở rộng cụm tắt thành câu đầy đủ")
        .build();
    settings.add(&ui.macro_enabled);
    settings.add(&ui.capitalize_macro);
    settings.add(&ui.macro_in_off_mode);
    let time = action_row("Định dạng giờ ($TIME)", "Ví dụ %H:%M");
    time.add_suffix(&ui.time_format);
    settings.add(&time);
    let date = action_row("Định dạng ngày ($DATE)", "Ví dụ %d/%m/%Y");
    date.add_suffix(&ui.date_format);
    settings.add(&date);
    page.add(&settings);

    // The list group is the editor: `fill_macros` adds one row per entry
    // and the add row below appends a pair. The rows live in the shared
    // ListBox; the group wrapping it is page layout.
    let list_group = adw::PreferencesGroup::builder()
        .title("Bảng gõ tắt")
        .description("Mỗi cặp: khóa=Nội dung ($TIME, $DATE)")
        .build();
    list_group.add(&ui.macros_list);
    page.add(&list_group);
    let add_group = adw::PreferencesGroup::new();
    let add_btn = gtk4::Button::with_label("Thêm gõ tắt");
    let add_row = action_row(
        "Thêm cặp mới",
        "Khóa và nội dung trống — điền ngay sau khi thêm",
    );
    add_btn.add_css_class("flat");
    add_row.add_suffix(&add_btn);
    add_group.add(&add_row);
    page.add(&add_group);

    (page, add_btn)
}
