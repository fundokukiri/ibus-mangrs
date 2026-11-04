//! Page "Gõ tắt": the macro switches, the $TIME/$DATE formats and the
//! shortcut list itself (one editable row per entry).

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> (adw::PreferencesPage, gtk4::Button) {
    let page = adw::PreferencesPage::new();

    let settings = adw::PreferencesGroup::builder()
        .title(t("groups.macros.settings.title", "Gõ tắt"))
        .description(t(
            "groups.macros.settings.description",
            "Mở rộng cụm tắt thành câu đầy đủ",
        ))
        .build();
    settings.add(&ui.macro_enabled);
    settings.add(&ui.capitalize_macro);
    settings.add(&ui.macro_in_off_mode);
    let time = action_row(
        &t("rows.time_format.title", "Định dạng giờ ($TIME)"),
        &t("rows.time_format.subtitle", "Ví dụ %H:%M"),
    );
    time.add_suffix(&ui.time_format);
    settings.add(&time);
    let date = action_row(
        &t("rows.date_format.title", "Định dạng ngày ($DATE)"),
        &t("rows.date_format.subtitle", "Ví dụ %d/%m/%Y"),
    );
    date.add_suffix(&ui.date_format);
    settings.add(&date);
    page.add(&settings);

    // The list group is the editor: `fill_macros` adds one row per entry
    // and the add row below appends a pair. The rows live in the shared
    // ListBox; the group wrapping it is page layout.
    let list_group = adw::PreferencesGroup::builder()
        .title(t("groups.macros.list.title", "Bảng gõ tắt"))
        .description(t(
            "groups.macros.list.description",
            "Mỗi cặp: khóa=Nội dung ($TIME, $DATE)",
        ))
        .build();
    list_group.add(&ui.macros_list);
    page.add(&list_group);
    let add_group = adw::PreferencesGroup::new();
    let add_btn = gtk4::Button::with_label(&t("groups.macros.add.button", "Thêm gõ tắt"));
    let add_row = action_row(
        &t("groups.macros.add.title", "Thêm cặp mới"),
        &t(
            "groups.macros.add.subtitle",
            "Khóa và nội dung trống — điền ngay sau khi thêm",
        ),
    );
    add_btn.add_css_class("flat");
    add_row.add_suffix(&add_btn);
    add_group.add(&add_row);
    page.add(&add_group);

    // The raw table, dictionary-style: one `khóa=nội dung` line per pair,
    // kept in sync with the rows above (a row edit mirrors here, a text
    // edit rebuilds the rows on focus-out). Bulk edits — pasting a whole
    // table — happen here instead of one row at a time.
    let raw = adw::PreferencesGroup::builder()
        .title(t("groups.macros.raw.title", "Nội dung gõ tắt"))
        .description(t(
            "groups.macros.raw.description",
            "Mỗi dòng một cặp khóa=nội dung ($TIME, $DATE)",
        ))
        .build();
    let scroll = card_text_editor(
        &ui.macros_text,
        &t(
            "groups.macros.raw.placeholder",
            "khóa=nội dung — mỗi dòng một cặp",
        ),
    );
    raw.add(&scroll);
    page.add(&raw);

    (page, add_btn)
}
