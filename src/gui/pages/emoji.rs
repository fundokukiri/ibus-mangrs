//! Page "Emoji": shortcode typing and the user's own shortcode table
//! (which overrides or extends the embedded one). The picker shortcut row
//! activates the window's `page-8` action — the Phím tắt page, the 8th
//! entry in content.rs's sidebar order — so no shell handle is needed;
//! the row re-renders on map, because the shortcut may have changed on
//! that page since.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // The switch leads the page alone; a group header would repeat it.
    let settings = adw::PreferencesGroup::new();
    // ":ten:" reads as code: markup on for this row — the text has no &.
    ui.emoji_enabled.set_use_markup(true);
    ui.emoji_enabled.set_subtitle(&t(
        "rows.emoji_enabled.subtitle",
        "Gõ <tt>:ten:</tt> kèm dấu hai chấm sẽ thành emoji khi hết từ",
    ));
    settings.add(&ui.emoji_enabled);

    // The picker shortcut: the same keycap display as the Phím tắt page,
    // bound to the same stored value, plus the go arrow.
    let shortcut_row = adw::ActionRow::builder()
        .use_markup(false)
        .title(t("rows.hotkey_emoji.title", "Phím tắt mở bảng emoji"))
        .subtitle(t(
            "groups.emoji.shortcut.subtitle",
            "Mở menu chọn emoji tại con trỏ. Gán ở trang Phím tắt.",
        ))
        .activatable(true)
        .build();
    let display = ShortcutDisplay::new(&t("rows.hotkey_emoji.title", "Phím tắt mở bảng emoji"));
    shortcut_row.add_suffix(display.widget());
    shortcut_row.add_suffix(&gtk4::Image::from_icon_name("go-next-symbolic"));
    {
        // The accessible description carries the current combo; the row
        // and its display re-render on map, because the shortcut may have
        // been reassigned on the Phím tắt page since.
        let ui = ui.clone();
        let display = display.clone();
        let render = move |row: &adw::ActionRow| {
            let value = ui.emoji_hotkey_value.borrow().clone();
            display.render(&value);
            let keys = if value.is_empty() {
                t("misc.a11y_unassigned", "chưa gán")
            } else {
                accelerator_label(&value).unwrap_or_else(|| value.clone())
            };
            row.update_property(&[gtk4::accessible::Property::Description(&format!(
                "{}, {}",
                t("rows.hotkey_emoji.title", "Phím tắt mở bảng emoji"),
                keys
            ))]);
        };
        render(&shortcut_row);
        let row_for_map = shortcut_row.clone();
        shortcut_row.connect_map(move |_| render(&row_for_map));
    }
    shortcut_row.connect_activated(|row| {
        // page-8: the hotkeys page's window action (content.rs numbers the
        // pages 1-based in their sidebar order). The action exists for
        // every sidebar page; a miss only means that order changed, and
        // there is nothing to recover with.
        let _ = row.activate_action("page-8", None);
    });
    settings.add(&shortcut_row);
    page.add(&settings);

    // The user's shortcode table, macros-style: one `ten=emoji` line per
    // pair, saved through the funnel on focus-out. Bulk edits happen here.
    let raw = adw::PreferencesGroup::builder()
        .title(t("groups.emoji.raw.title", "Bảng emoji riêng"))
        .description(t(
            "groups.emoji.raw.description",
            "Mỗi dòng một cặp <tt>ten=emoji</tt>. Ghi đè hoặc thêm shortcode (chữ thường).",
        ))
        .build();
    // Header suffix: the live entry count.
    let count = gtk4::Label::new(None);
    count.add_css_class("dim-label");
    count.add_css_class("caption");
    count.set_valign(gtk4::Align::Center);
    raw.set_header_suffix(Some(&count));

    // The error line under the editor: informational only, never blocks
    // the save.
    let error_line = gtk4::Label::new(None);
    error_line.add_css_class("error");
    error_line.add_css_class("caption");
    error_line.set_xalign(0.0);
    error_line.set_margin_top(6);
    error_line.set_visible(false);

    let scroll = bounded_card_editor(
        &ui.emoji_text,
        &t("groups.emoji.raw.hint", "cuoi=😄\ntim=❤️\nok=👌"),
        &t("groups.emoji.raw.title", "Bảng emoji riêng"),
    );
    raw.add(&scroll);
    raw.add(&error_line);
    page.add(&raw);

    // With Gõ tắt emoji off the table is dead weight: it disables with
    // the switch, like the macro and dictionary editors.
    ui.emoji_enabled
        .bind_property("active", &scroll, "sensitive")
        .sync_create()
        .build();

    // The count and the error line follow the buffer; the save stays on
    // focus-leave (content.rs), untouched.
    {
        let count_into = count.clone();
        let error_into = error_line.clone();
        ui.emoji_text.buffer().connect_changed(move |buf| {
            let text = buf.text(&buf.start_iter(), &buf.end_iter(), false);
            let (valid, error) = line_stats(text.as_str(), "groups.emoji.raw");
            count_into.set_text(
                &t("groups.emoji.raw.count", "{n} mục").replace("{n}", &valid.to_string()),
            );
            error_into.set_text(error.as_deref().unwrap_or(""));
            error_into.set_visible(error.is_some());
        });
        let (valid, error) = line_stats(&text_of(&ui.emoji_text), "groups.emoji.raw");
        count.set_text(&t("groups.emoji.raw.count", "{n} mục").replace("{n}", &valid.to_string()));
        error_line.set_text(error.as_deref().unwrap_or(""));
        error_line.set_visible(error.is_some());
    }

    page
}
