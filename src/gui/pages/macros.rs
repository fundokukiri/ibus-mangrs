//! Page "Gõ tắt": the macro switches, the $TIME/$DATE formats and the
//! raw macro table (one `khóa=nội dung` line per pair — the single
//! editor, a row list would duplicate it).

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
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

    // The raw table, dictionary-style: one `khóa=nội dung` line per pair,
    // saved straight through the funnel on focus-out. Bulk edits —
    // pasting a whole table — happen here.

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

    page
}
