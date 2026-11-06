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

/// The macro line stats, by the parser's own rule (`macros_from_text`): a
/// pair needs a `=` and non-empty key and value. The error line only
/// reports non-blank lines without a `=` — the other drops stay silent,
/// exactly as they always were.
pub(crate) fn line_stats(text: &str) -> (usize, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let valid = lines
        .iter()
        .filter(|l| {
            l.split_once('=')
                .is_some_and(|(k, v)| !k.trim().is_empty() && !v.trim().is_empty())
        })
        .count();
    let bad: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && l.split_once('=').is_none())
        .map(|(i, _)| i + 1)
        .collect();
    let error = match bad.as_slice() {
        [] => None,
        [n] => Some(
            t("groups.macros.raw.error_one", "Dòng {n} thiếu dấu =").replace("{n}", &n.to_string()),
        ),
        [first, ..] => Some(
            t(
                "groups.macros.raw.error_many",
                "{k} dòng thiếu dấu =, bắt đầu từ dòng {n}",
            )
            .replace("{k}", &bad.len().to_string())
            .replace("{n}", &first.to_string()),
        ),
    };
    (valid, error)
}

/// The editor shell: like the dictionary's card editor but bounded
/// (240..420px, the page never becomes one giant block) and the empty
/// hint sits top-left with three example lines. The TextView drops its
/// own `.view` background so the card fill shows through unchanged.
fn editor_card(ui: &Rc<Ui>, hint_visible: &Rc<RefCell<bool>>) -> gtk4::ScrolledWindow {
    ui.macros_text.remove_css_class("view");
    ui.macros_text.set_left_margin(16);
    ui.macros_text.set_right_margin(16);
    ui.macros_text.set_top_margin(12);
    ui.macros_text.set_bottom_margin(12);
    ui.macros_text
        .update_property(&[gtk4::accessible::Property::Label(&t(
            "groups.macros.raw.title",
            "Nội dung gõ tắt",
        ))]);
    // The example lines, top-left over the text area: decorative, so they
    // take neither clicks nor focus.
    let hint = gtk4::Label::new(Some(&t(
        "groups.macros.raw.hint",
        "vn=Việt Nam\ntphcm=Thành phố Hồ Chí Minh\ngio=$TIME",
    )));
    hint.add_css_class("dim-label");
    hint.add_css_class("monospace");
    hint.set_xalign(0.0);
    hint.set_valign(gtk4::Align::Start);
    hint.set_halign(gtk4::Align::Start);
    hint.set_margin_start(16);
    hint.set_margin_top(12);
    hint.set_can_target(false);
    hint.set_focusable(false);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(&ui.macros_text));
    overlay.add_overlay(&hint);
    let scroll = gtk4::ScrolledWindow::new();
    scroll.add_css_class("card");
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_min_content_height(240);
    scroll.set_max_content_height(420);
    scroll.set_propagate_natural_height(true);
    scroll.set_child(Some(&overlay));
    // The hint follows the buffer: visible only while it is empty.
    {
        let hint_into = hint.clone();
        let empty_flag = hint_visible.clone();
        ui.macros_text.buffer().connect_changed(move |buf| {
            let empty = buf.char_count() == 0;
            hint_into.set_visible(empty);
            *empty_flag.borrow_mut() = empty;
        });
        let empty = ui.macros_text.buffer().char_count() == 0;
        hint.set_visible(empty);
        *hint_visible.borrow_mut() = empty;
    }
    scroll
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

    let hint_visible = Rc::new(RefCell::new(true));
    let scroll = editor_card(ui, &hint_visible);
    raw.add(&scroll);
    raw.add(&error_line);
    page.add(&raw);

    // With Gõ tắt off the table is dead weight: it visibly disables with
    // the switch, like the dictionary editor.
    ui.macro_enabled
        .bind_property("active", &scroll, "sensitive")
        .sync_create()
        .build();

    // The count and the error line follow the buffer; the save stays on
    // focus-leave (content.rs), untouched.
    {
        let count_into = count.clone();
        let error_into = error_line.clone();
        ui.macros_text.buffer().connect_changed(move |buf| {
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            let (valid, error) = line_stats(text.as_str());
            count_into.set_text(
                &t("groups.macros.raw.count", "{n} mục").replace("{n}", &valid.to_string()),
            );
            error_into.set_text(error.as_deref().unwrap_or(""));
            error_into.set_visible(error.is_some());
        });
        let (valid, error) = line_stats(&text_of(&ui.macros_text));
        count.set_text(&t("groups.macros.raw.count", "{n} mục").replace("{n}", &valid.to_string()));
        error_line.set_text(error.as_deref().unwrap_or(""));
        error_line.set_visible(error.is_some());
    }

    page
}
