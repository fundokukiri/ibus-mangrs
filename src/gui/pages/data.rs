//! Page "Từ điển": the personal dictionary — one switch plus the word list
//! editor. The word list can grow into the thousands, so one row per word
//! would not scale; it stays a text file shown in the shared bounded card,
//! which disables with the switch and dims its text (the theme would leave
//! a TextView at full opacity).

use super::*;

/// The page's only CSS: insensitive editor content must read as dead, but
/// the theme keeps TextView text at full opacity.
const DATA_CSS: &str = "
/* insensitive editor content dims; the theme skips TextViews */
.dictionary-editor textview:disabled {
  opacity: 0.45;
}
";

/// One icon+label button: the label is detached from the stock button and
/// re-parented into an icon+label box (a button child can only be set
/// once).
fn open_button() -> gtk4::Button {
    let btn = gtk4::Button::with_label(&t("groups.data.words.open_button", "Mở tệp"));
    let text = btn
        .child()
        .and_downcast::<gtk4::Label>()
        .expect("with_label");
    btn.set_child(None::<&gtk4::Widget>);
    let content = gtk4::Box::new(Orientation::Horizontal, 6);
    content.append(&gtk4::Image::from_icon_name("document-open-symbolic"));
    content.append(&text);
    btn.set_child(Some(&content));
    btn.set_valign(gtk4::Align::Center);
    btn.set_tooltip_text(Some(&t(
        "groups.data.words.open_tooltip",
        "Mở tệp từ điển trong trình soạn thảo",
    )));
    btn
}

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let provider = gtk4::CssProvider::new();
    provider.load_from_data(DATA_CSS);
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let dictionary = adw::PreferencesGroup::builder()
        .title(t("groups.data.dictionary.title", "Từ điển của tôi"))
        .description(t(
            "groups.data.dictionary.description",
            "Từ trong từ điển không bị khôi phục về chuỗi phím.",
        ))
        .build();
    dictionary.add(&ui.dictionary_enabled);
    page.add(&dictionary);

    // The word list is one text file in the shared bounded card; bulk
    // edits (pasting a whole list) happen here or in a real editor.
    let scroll = bounded_card_editor(
        &ui.dictionary_text,
        &t("groups.data.words.hint", "github\nlinux\nrust"),
        &t("groups.data.words.title", "Danh sách từ"),
    );
    // The dim-on-disabled rule is scoped to this editor.
    scroll.add_css_class("dictionary-editor");

    // Header suffix: the live word count (the parser's own rule — trimmed,
    // non-empty, no `#` comment lines; hidden at zero) and the file
    // button, vertically centered with the title.
    let count = gtk4::Label::new(None);
    count.add_css_class("dim-label");
    count.add_css_class("caption");
    count.set_valign(gtk4::Align::Center);
    let open_btn = open_button();
    let header = gtk4::Box::new(Orientation::Horizontal, 12);
    header.append(&count);
    header.append(&open_btn);
    let words = adw::PreferencesGroup::builder()
        .title(t("groups.data.words.title", "Danh sách từ"))
        .description(t("groups.data.words.description", "Mỗi dòng một từ."))
        .build();
    words.set_header_suffix(Some(&header));
    words.add(&scroll);
    page.add(&words);

    // The switch gates the editor: a dead text area reads as a bug, so it
    // visibly disables with the dictionary off (the text is kept).
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

    // The count follows the buffer; the save stays on focus-leave
    // (content.rs), untouched.
    {
        let count_into = count.clone();
        ui.dictionary_text.buffer().connect_changed(move |buf| {
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            let words = text
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty() && !l.starts_with('#'))
                .count();
            let label = t("groups.data.words.count", "{n} từ").replace("{n}", &words.to_string());
            count_into.set_text(&label);
            count_into.set_visible(words > 0);
        });
        let words = text_of(&ui.dictionary_text)
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty() && !l.starts_with('#'))
            .count();
        count.set_text(&t("groups.data.words.count", "{n} từ").replace("{n}", &words.to_string()));
        count.set_visible(words > 0);
    }

    page
}
