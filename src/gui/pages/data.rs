//! Page "Dữ liệu": the personal dictionary (a plain text area — the word
//! list can grow into the thousands, so one row per word would not scale)
//! and the JSON backup export/import.

use super::*;

pub(crate) fn build(
    ui: &Rc<Ui>,
    export_btn: &gtk4::Button,
    import_btn: &gtk4::Button,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let dictionary = adw::PreferencesGroup::builder()
        .title("Từ điển của tôi")
        .description("Từ trong từ điển không bị khôi phục về chuỗi phím (~/.config/ibus-mangrs/dictionary.txt)")
        .build();
    dictionary.add(&ui.dictionary_enabled);
    dictionary.add(&ui.dictionary_text);
    page.add(&dictionary);

    let backup = adw::PreferencesGroup::builder()
        .title("Sao lưu & khôi phục")
        .description("Một JSON chứa cấu hình, gõ tắt và từ điển — chuyển máy hoặc dự phòng")
        .build();
    let export = action_row("Xuất cấu hình ra JSON", "Lưu ra file để mang đi");
    export.add_suffix(export_btn);
    backup.add(&export);
    let import = action_row("Nhập cấu hình từ JSON", "Đọc lại từ file đã xuất");
    import.add_suffix(import_btn);
    backup.add(&import);
    page.add(&backup);

    page
}
