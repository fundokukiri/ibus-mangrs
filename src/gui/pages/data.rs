//! Page "Từ điển": the personal dictionary, a plain text area — the word
//! list can grow into the thousands, so one row per word would not scale.
//! (Backup export/import moved to the Tinh chỉnh page.)

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let dictionary = adw::PreferencesGroup::builder()
        .title("Từ điển của tôi")
        .description("Từ trong từ điển không bị khôi phục về chuỗi phím (~/.config/ibus-mangrs/dictionary.txt)")
        .build();
    dictionary.add(&ui.dictionary_enabled);
    dictionary.add(&ui.dictionary_text);
    page.add(&dictionary);

    page
}
