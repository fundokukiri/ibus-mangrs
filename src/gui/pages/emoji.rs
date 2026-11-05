//! Page "Emoji": shortcode typing and the user's own shortcode table
//! (which overrides or extends the embedded one). The picker hotkey row
//! lives on "Phím tắt" with the other engine shortcuts, one row each.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let settings = adw::PreferencesGroup::builder()
        .title(t("groups.emoji.settings.title", "Emoji"))
        .description(t(
            "groups.emoji.settings.description",
            "Gõ :ten: kèm hai dấu hai chấm sẽ thành emoji khi hết từ",
        ))
        .build();
    settings.add(&ui.emoji_enabled);
    page.add(&settings);

    // The user's shortcode table, macros-style: one `ten=emoji` line per
    // pair, saved through the funnel on focus-out. Bulk edits happen here.
    let raw = adw::PreferencesGroup::builder()
        .title(t("groups.emoji.raw.title", "Bảng emoji riêng"))
        .description(t(
            "groups.emoji.raw.description",
            "Mỗi dòng một cặp ten=emoji — ghi đè hoặc thêm shortcode (chữ thường)",
        ))
        .build();
    let scroll = card_text_editor(
        &ui.emoji_text,
        &t(
            "groups.emoji.raw.placeholder",
            "ten=emoji — mỗi dòng một cặp",
        ),
    );
    raw.add(&scroll);
    page.add(&raw);

    // Pointer to the picker hotkey row: data, not decoration (the hotkeys
    // page keeps every engine shortcut row).
    let note = adw::PreferencesGroup::new();
    let label = gtk4::Label::builder()
        .wrap(true)
        .xalign(0.0)
        .label(t(
            "groups.emoji.note.text",
            "Phím tắt mở bảng chọn emoji gán ở trang Phím tắt.",
        ))
        .build();
    label.add_css_class("dim-label");
    note.add(&label);
    page.add(&note);

    page
}
