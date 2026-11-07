//! Page "Gõ tắt": the macro switches, the $TIME/$DATE formats and the
//! raw macro table (one `khóa=nội dung` line per pair — the single
//! editor, a row list would duplicate it). The editor is a bounded card
//! (240..420px) with a top-left example hint, a live entry count and an
//! informational error line for lines the parser drops; saving stays
//! focus-leave through the shared funnel.

use super::*;

/// The current time/date as the engine formats it — chrono's strftime, the
/// very call inside `Macros::expand`. A pattern chrono rejects (Item::Error)
/// renders nothing instead of a wrong preview.
pub(crate) fn preview(pattern: &str) -> Option<String> {
    use chrono::format::{strftime::StrftimeItems, Item};
    if StrftimeItems::new(pattern).any(|it| matches!(it, Item::Error)) {
        return None;
    }
    Some(chrono::Local::now().format(pattern).to_string())
}

/// A dim live preview of the pattern typed into the row, updated while
/// typing; empty or invalid patterns leave the label blank.
fn add_preview(row: &adw::EntryRow) {
    let label = gtk4::Label::new(None);
    label.add_css_class("dim-label");
    label.add_css_class("caption");
    label.set_valign(gtk4::Align::Center);
    label.set_margin_end(6);
    row.add_suffix(&label);
    {
        let into = label.clone();
        row.connect_changed(move |row| {
            into.set_text(&preview(row.text().trim()).unwrap_or_default());
        });
        label.set_text(&preview(row.text().trim()).unwrap_or_default());
    }
}

/// The pencil (edit-icon) inside an AdwEntryRow has no API to hide it and
/// CSS cannot remove a widget's paint. The template never toggles it in
/// code, so hiding it once — and again on every map, for safety — sticks;
/// the row stays fully editable without it.
fn hide_edit_icon(row: &adw::EntryRow) {
    fn hide(w: &gtk4::Widget) {
        if let Some(img) = w.downcast_ref::<gtk4::Image>() {
            if img.css_classes().iter().any(|c| c.as_str() == "edit-icon") {
                img.set_visible(false);
            }
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            hide(&c);
            child = c.next_sibling();
        }
    }
    hide(row.upcast_ref());
    row.connect_map(|row| hide(row.upcast_ref()));
}

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
    settings.add(&ui.macro_valid_words);
    decorate_experimental(&ui.macro_valid_words);
    // The old subtitle examples ("Ví dụ %H:%M") live on as the row
    // tooltip: EntryRow has no placeholder in this libadwaita.
    ui.time_format
        .set_tooltip_text(Some(&t("rows.time_format.subtitle", "Ví dụ %H:%M")));
    ui.date_format
        .set_tooltip_text(Some(&t("rows.date_format.subtitle", "Ví dụ %d/%m/%Y")));
    ui.time_format.add_css_class("format-row");
    ui.date_format.add_css_class("format-row");
    hide_edit_icon(&ui.time_format);
    hide_edit_icon(&ui.date_format);
    add_preview(&ui.time_format);
    add_preview(&ui.date_format);
    settings.add(&ui.time_format);
    settings.add(&ui.date_format);
    page.add(&settings);

    // The raw table, dictionary-style: one `khóa=nội dung` line per pair,
    // saved straight through the funnel on focus-out. Bulk edits —
    // pasting a whole table — happen here.
    let raw = adw::PreferencesGroup::builder()
        .title(t("groups.macros.raw.title", "Nội dung gõ tắt"))
        // Markup description: the $TIME/$DATE tokens read as code, like
        // the monospace paths on "Cài đặt khác".
        .description(t(
            "groups.macros.raw.description",
            "Mỗi dòng một cặp khóa=nội dung. Dùng <tt>$TIME</tt> và <tt>$DATE</tt> để chèn giờ và ngày.",
        ))
        .build();

    // Header suffix: the live entry count and the file button, like the
    // dictionary page.
    let count = gtk4::Label::new(None);
    count.add_css_class("dim-label");
    count.add_css_class("caption");
    count.set_valign(gtk4::Align::Center);
    let open_btn = gtk4::Button::with_label(&t("groups.macros.raw.open_button", "Mở tệp"));
    open_btn.add_css_class("flat");
    open_btn.set_valign(gtk4::Align::Center);
    open_btn.set_tooltip_text(Some(&t(
        "groups.macros.raw.open_tooltip",
        "Mở macros.toml bằng ứng dụng mặc định",
    )));
    let header = gtk4::Box::new(Orientation::Horizontal, 12);
    header.append(&count);
    header.append(&open_btn);
    raw.set_header_suffix(Some(&header));
    open_btn.connect_clicked(move |_| {
        if let Some(path) = crate::macros::Macros::path() {
            gtk4::FileLauncher::new(Some(&gtk4::gio::File::for_path(path))).launch(
                None::<&gtk4::Window>,
                None::<&gtk4::gio::Cancellable>,
                |_| {},
            );
        }
    });

    // The error line under the editor: informational only, never blocks
    // the save.
    let error_line = gtk4::Label::new(None);
    error_line.add_css_class("error");
    error_line.add_css_class("caption");
    error_line.set_xalign(0.0);
    error_line.set_margin_top(6);
    error_line.set_visible(false);

    let scroll = bounded_card_editor(
        &ui.macros_text,
        &t(
            "groups.macros.raw.hint",
            "vn=Việt Nam\ntphcm=Thành phố Hồ Chí Minh\ngio=$TIME",
        ),
        &t("groups.macros.raw.title", "Nội dung gõ tắt"),
    );
    raw.add(&scroll);
    raw.add(&error_line);
    page.add(&raw);

    // With Gõ tắt off the table is dead weight: it visibly disables with
    // the switch, like the dictionary editor.
    gate_editor(&ui.macro_enabled, &scroll);

    // The count and the error line follow the buffer; the save stays on
    // focus-leave (content.rs), untouched.
    {
        let count_into = count.clone();
        let error_into = error_line.clone();
        ui.macros_text.buffer().connect_changed(move |buf| {
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            let (valid, error) = line_stats(text.as_str(), "groups.macros.raw");
            count_into.set_text(
                &t("groups.macros.raw.count", "{n} mục").replace("{n}", &valid.to_string()),
            );
            error_into.set_text(error.as_deref().unwrap_or(""));
            error_into.set_visible(error.is_some());
        });
        let (valid, error) = line_stats(&text_of(&ui.macros_text), "groups.macros.raw");
        count.set_text(&t("groups.macros.raw.count", "{n} mục").replace("{n}", &valid.to_string()));
        error_line.set_text(error.as_deref().unwrap_or(""));
        error_line.set_visible(error.is_some());
    }

    page
}
