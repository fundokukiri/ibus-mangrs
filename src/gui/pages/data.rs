//! Page "Từ điển": the personal dictionary — one switch plus the word list
//! editor. The word list can grow into the thousands, so one row per word
//! would not scale; it stays a text file shown in a card.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let dictionary = adw::PreferencesGroup::builder()
        .title(t("groups.data.dictionary.title", "Từ điển của tôi"))
        .description(t(
            "groups.data.dictionary.description",
            "Từ trong từ điển không bị khôi phục về chuỗi phím.",
        ))
        .build();
    dictionary.add(&ui.dictionary_enabled);
    page.add(&dictionary);

    // The word list is one text file in a card; the page scroll takes
    // over beyond 240 px (see card_text_editor for the shared shell).
    let scroll = card_text_editor(
        &ui.dictionary_text,
        &t(
            "groups.data.words.placeholder",
            "Mỗi dòng một từ — danh sách đang trống",
        ),
    );

    let words = adw::PreferencesGroup::builder()
        .title(t("groups.data.words.title", "Danh sách từ"))
        .build();
    let open_btn = gtk4::Button::with_label(&t("groups.data.words.open_button", "Mở tệp"));
    open_btn.add_css_class("flat");
    open_btn.set_tooltip_text(Some(&t(
        "groups.data.words.open_tooltip",
        "Mở dictionary.txt bằng ứng dụng mặc định",
    )));
    words.set_header_suffix(Some(&open_btn));
    words.add(&scroll);
    page.add(&words);

    // The switch gates the editor: a dead text area reads as a bug, so it
    // visibly disables with the dictionary off.
    ui.dictionary_enabled
        .bind_property("active", &scroll, "sensitive")
        .sync_create()
        .build();
    // Bulk edits (paste a word list) happen in a real editor, not this card.
    open_btn.connect_clicked(move |_| {
        if let Some(path) = crate::macros::Dictionary::path() {
            gtk4::FileLauncher::new(Some(&gtk4::gio::File::for_path(path))).launch(
                None::<&gtk4::Window>,
                None::<&gtk4::gio::Cancellable>,
                |_| {},
            );
        }
    });

    page
}
