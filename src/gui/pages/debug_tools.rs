//! The right-hand column of the debug page: the typing test, then the
//! diagnostics as preferences groups. A row carries a status dot, a label, the
//! value on its right and, when something is off, a line saying what to do —
//! clicking a row copies its value, and a file row also has a button to open
//! its folder. The groups are the window's own row widgets, so this column
//! reads like every settings tab instead of a hand-drawn list.

use super::*;
use crate::gui::debug_report::{Cell, Section, Status};

pub(super) const TOOLS_CSS: &str = "
.diag-dot-ok { color: @success_color; }
.diag-dot-warn { color: @warning_color; }
.diag-dot-info { color: alpha(@view_fg_color, 0.55); }
/* Each value is something to read and copy: shape it like a value, not like a
   second label. */
.diag-value {
  font-feature-settings: \"tnum\";
  background-color: alpha(@view_fg_color, 0.07);
  border-radius: 6px;
  padding: 1px 6px;
}
/* A dropdown beside three flat icon buttons should not read as the only
   control on the bar. */
.diag-kind button {
  min-height: 22px;
  padding: 2px 8px;
}
/* The count of lines that arrived while the view is paused. */
.log-chip {
  background-color: alpha(@accent_bg_color, 0.22);
  border-radius: 9px;
  padding: 1px 7px;
}
/* The log is one card, like the diagnostics cards beside it. */
.log-pane {
  border-radius: 10px;
  border-width: 1px;
  border-style: solid;
  border-color: alpha(@view_fg_color, 0.1);
  background-color: alpha(@view_fg_color, 0.035);
}
.log-view, .log-view > text {
  background-color: transparent;
}
/* This column answers ten short questions; the window's row height is built for
   settings rows with a subtitle, which leaves too much air here. */
.tools-compact row {
  min-height: 30px;
  padding-top: 1px;
  padding-bottom: 1px;
}
.tools-compact row > box.header {
  min-height: 20px;
  padding-top: 1px;
  padding-bottom: 1px;
}
.tools-compact preferencesgroup > box > box.header {
  margin-top: 2px;
  margin-bottom: 2px;
}
.tools-compact row.diag-warn {
  background-color: alpha(@warning_color, 0.14);
}
";

/// The column's fixed parts, so the page can wire the typing test and re-read
/// the groups without rebuilding the window.
pub(super) struct Tools {
    pub body: gtk4::Widget,
    /// Says whether the engine sees a word being composed.
    pub composing: gtk4::Label,
    pub refresh: Rc<dyn Fn()>,
}

/// Build the column. `facts` is asked again on every refresh, which is what
/// makes "Đọc lại" and a restart show their result.
pub(super) fn build(
    ui: &Rc<Ui>,
    overlay: &adw::ToastOverlay,
    facts: Rc<dyn Fn() -> Vec<Section>>,
) -> Tools {
    let body = gtk4::Box::new(Orientation::Vertical, 10);
    body.add_css_class("tools-compact");

    // --- Typing test ------------------------------------------------------
    let try_group = adw::PreferencesGroup::new();
    try_group.set_title(&t("misc.section_try", "Thử gõ"));
    let card = gtk4::Box::new(Orientation::Vertical, 0);
    card.add_css_class("card");
    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some(&t(
        "misc.debug_try_placeholder",
        "Gõ thử ở đây, log bên trái chạy theo",
    )));
    entry.set_margin_top(6);
    entry.set_margin_bottom(6);
    entry.set_margin_start(10);
    entry.set_margin_end(10);
    entry.set_tooltip_text(Some(&t(
        "tips.debug_try",
        "Gõ vào đây để xem bộ gõ có nhận phím không: dòng mới hiện bên trái nghĩa là phím đã đi qua IBus.",
    )));
    card.append(&entry);
    card.append(&gtk4::Separator::new(Orientation::Horizontal));
    // The buffer keeps its own line under the field, the way the sample draws
    // it: the field is for typing, the strip is for what the engine sees.
    let strip = gtk4::Box::new(Orientation::Horizontal, 8);
    strip.set_margin_top(4);
    strip.set_margin_bottom(4);
    strip.set_margin_start(12);
    strip.set_margin_end(12);
    let buffer_caption = gtk4::Label::new(Some(&t("misc.buffer_label", "buffer:")));
    buffer_caption.add_css_class("caption");
    buffer_caption.add_css_class("dim-label");
    let buffer = gtk4::Label::new(None);
    buffer.add_css_class("caption");
    buffer.add_css_class("monospace");
    buffer.set_xalign(0.0);
    buffer.set_hexpand(true);
    buffer.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    buffer.set_tooltip_text(Some(&t(
        "tips.debug_buffer",
        "Chữ đang có trong ô thử, lấy ngay từ ô chứ không phải từ bộ gõ.",
    )));
    let composing = gtk4::Label::new(None);
    composing.add_css_class("caption");
    composing.add_css_class("dim-label");
    strip.append(&buffer_caption);
    strip.append(&buffer);
    strip.append(&composing);
    card.append(&strip);
    try_group.add(&card);
    body.append(&try_group);
    {
        let buffer = buffer.clone();
        entry.connect_changed(move |entry| buffer.set_markup(&buffer_markup(&entry.text())));
    }
    buffer.set_markup(&buffer_markup(""));

    // --- The groups -------------------------------------------------------
    let groups = gtk4::Box::new(Orientation::Vertical, 6);
    body.append(&groups);

    // The window is rebuilt only when a row actually changed: tearing down and
    // re-creating four groups twice a second made the whole page re-lay out
    // while the user was typing in the box above.
    let shown: Rc<RefCell<Vec<Section>>> = Rc::new(RefCell::new(Vec::new()));
    let refresh: Rc<dyn Fn()> = {
        let groups = groups.clone();
        let overlay = overlay.clone();
        let ui = ui.clone();
        let shown = Rc::clone(&shown);
        Rc::new(move || {
            let next = facts();
            if next == *shown.borrow() {
                return;
            }
            while let Some(child) = groups.first_child() {
                groups.remove(&child);
            }
            for section in &next {
                groups.append(&group_widget(section, &overlay, &ui));
            }
            *shown.borrow_mut() = next;
        })
    };
    refresh();
    Tools {
        body: body.upcast(),
        composing,
        refresh,
    }
}

/// The underlined text after `buffer:`. Pango markup, so the value is escaped.
pub(super) fn buffer_markup(text: &str) -> String {
    if text.is_empty() {
        return "<span alpha=\"55%\">—</span>".to_string();
    }
    format!("<u>{}</u>", gtk4::glib::markup_escape_text(text))
}

fn group_widget(section: &Section, overlay: &adw::ToastOverlay, ui: &Rc<Ui>) -> gtk4::Widget {
    // A caption above the list instead of the group's own heading: the heading
    // is built for a preferences page and costs a row of height for four words.
    let wrap = gtk4::Box::new(Orientation::Vertical, 2);
    let title = gtk4::Label::new(Some(&section.title));
    title.add_css_class("caption");
    title.add_css_class("dim-label");
    title.set_xalign(0.0);
    wrap.append(&title);
    let group = adw::PreferencesGroup::new();
    for cell in section.rows.iter().filter(|cell| cell.screen) {
        group.add(&row_widget(cell, overlay, ui));
    }
    wrap.append(&group);
    // The sentence that explains a warning sits under the card, in the theme's
    // warning colour: inside the row it pushed the row itself apart.
    for cell in section.rows.iter().filter(|cell| cell.screen) {
        if cell.status != Status::Warn {
            continue;
        }
        let Some(hint) = &cell.hint else { continue };
        let line = gtk4::Label::new(Some(hint));
        line.add_css_class("caption");
        line.add_css_class("warning");
        line.set_xalign(0.0);
        line.set_wrap(true);
        line.set_margin_start(12);
        line.set_margin_end(12);
        wrap.append(&line);
    }
    wrap.upcast()
}

fn row_widget(cell: &Cell, overlay: &adw::ToastOverlay, ui: &Rc<Ui>) -> gtk4::Widget {
    let row = adw::ActionRow::new();
    row.set_title(&cell.label);
    row.set_activatable(true);
    if cell.status == Status::Warn {
        row.add_css_class("diag-warn");
    }

    let dot = gtk4::Label::new(Some("●"));
    dot.add_css_class("caption");
    dot.add_css_class(match cell.status {
        Status::Ok => "diag-dot-ok",
        Status::Warn => "diag-dot-warn",
        Status::Info => "diag-dot-info",
    });
    row.add_prefix(&dot);

    let value = gtk4::Label::new(Some(&cell.value));
    value.add_css_class("caption");
    value.add_css_class("monospace");
    value.add_css_class("diag-value");
    value.set_xalign(1.0);
    value.set_single_line_mode(true);
    value.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    value.set_max_width_chars(28);
    row.add_suffix(&value);

    if cell.open_dir.is_some() {
        // The words stay in the tooltip and the accessible label: the button
        // shows an icon, not a sentence.
        let copy = gtk4::Button::from_icon_name("edit-copy-symbolic");
        copy.add_css_class("flat");
        copy.add_css_class("circular");
        copy.set_valign(gtk4::Align::Center);
        copy.set_tooltip_text(Some(&t("misc.debug_copy_path", "Sao chép đường dẫn")));
        copy.update_property(&[gtk4::accessible::Property::Label(&t(
            "misc.debug_copy_path",
            "Sao chép đường dẫn",
        ))]);
        if let Some(path) = cell.hint.clone() {
            let overlay = overlay.clone();
            let ui = ui.clone();
            copy.connect_clicked(move |btn| {
                btn.clipboard().set_text(&path);
                ui.fire_toast(&overlay, &t("misc.diag_copied", "Đã sao chép"));
            });
        }
        row.add_suffix(&copy);
    }

    let tip = cell.hint.as_deref().unwrap_or(&cell.value).to_string();
    row.set_tooltip_text(Some(&tip));
    let accessible = format!("{}: {}. {}", cell.label, cell.value, tip);
    row.update_property(&[gtk4::accessible::Property::Label(&accessible)]);
    {
        // A click copies the value the report would carry: the full path for a
        // file row, because that is what a helper needs.
        let copied = cell.hint.clone().unwrap_or_else(|| cell.value.clone());
        let overlay = overlay.clone();
        let ui = ui.clone();
        row.connect_activated(move |row| {
            row.clipboard().set_text(&copied);
            ui.fire_toast(&overlay, &t("misc.diag_copied", "Đã sao chép"));
        });
    }
    row.upcast()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_buffer_line_is_underlined_and_escaped() {
        assert_eq!(buffer_markup("chào"), "<u>chào</u>");
        // A typed `<` must not turn into markup.
        assert!(buffer_markup("<b>x</b>").contains("&lt;b&gt;"));
        assert!(buffer_markup("").contains("—"), "an empty buffer says so");
    }
}
