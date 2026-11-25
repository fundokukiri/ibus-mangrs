//! The right-hand column of the debug page: the typing test, then the
//! diagnostics as preferences groups. A row carries a status dot, a label, the
//! value on its right and, when something is off, a line saying what to do —
//! clicking a row copies its value, and a file row also has a button to open
//! its folder. The groups are the window's own row widgets, so this column
//! reads like every settings tab instead of a hand-drawn list.

use super::widgets::{action_row, row_subtitle};
use super::*;
use crate::gui::debug_report::{Cell, Section, Status};

pub(super) const TOOLS_CSS: &str = "
.diag-dot-ok { color: @success_color; }
.diag-dot-warn { color: @warning_color; }
.diag-dot-info { color: alpha(@view_fg_color, 0.45); }
.diag-value { font-feature-settings: \"tnum\"; }
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
    let body = gtk4::Box::new(Orientation::Vertical, 18);

    // --- Typing test ------------------------------------------------------
    let try_group = adw::PreferencesGroup::new();
    try_group.set_title(&t("misc.section_try", "Thử gõ"));
    let entry_row = adw::EntryRow::new();
    entry_row.set_title(&t("misc.debug_try_label", "Gõ thử"));
    entry_row.set_tooltip_text(Some(&t(
        "tips.debug_try",
        "Gõ vào đây để xem bộ gõ có nhận phím không: dòng mới hiện bên trái nghĩa là phím đã đi qua IBus.",
    )));
    try_group.add(&entry_row);

    let buffer_row = action_row(&t("misc.buffer_label", "buffer:"), "");
    let buffer = gtk4::Label::new(None);
    buffer.add_css_class("caption");
    buffer.add_css_class("monospace");
    buffer.set_xalign(1.0);
    buffer.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let composing = gtk4::Label::new(None);
    composing.add_css_class("caption");
    composing.add_css_class("dim-label");
    buffer_row.add_suffix(&buffer);
    buffer_row.add_suffix(&composing);
    try_group.add(&buffer_row);
    body.append(&try_group);
    {
        let buffer = buffer.clone();
        entry_row.connect_changed(move |entry| buffer.set_markup(&buffer_markup(&entry.text())));
    }
    buffer.set_markup(&buffer_markup(""));

    // --- The groups -------------------------------------------------------
    let groups = gtk4::Box::new(Orientation::Vertical, 18);
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
    let group = adw::PreferencesGroup::new();
    group.set_title(&section.title);
    for cell in &section.rows {
        group.add(&row_widget(cell, overlay, ui));
    }
    group.upcast()
}

fn row_widget(cell: &Cell, overlay: &adw::ToastOverlay, ui: &Rc<Ui>) -> gtk4::Widget {
    let row = adw::ActionRow::new();
    row.set_title(&cell.label);
    row.set_activatable(true);
    if cell.status == Status::Warn {
        if let Some(hint) = &cell.hint {
            row.set_subtitle(hint);
            row.set_subtitle_lines(3);
            if let Some(line) = row_subtitle(&row) {
                line.add_css_class("warning");
                line.add_css_class("caption");
            }
        }
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
        // shows a folder, not a sentence.
        let open = gtk4::Button::from_icon_name("folder-open-symbolic");
        open.add_css_class("flat");
        open.add_css_class("circular");
        open.set_valign(gtk4::Align::Center);
        open.set_tooltip_text(Some(&t("misc.debug_open_folder", "Mở thư mục chứa tệp")));
        open.update_property(&[gtk4::accessible::Property::Label(&t(
            "misc.debug_open_folder",
            "Mở thư mục chứa tệp",
        ))]);
        if let Some(dir) = cell.open_dir.clone() {
            open.connect_clicked(move |_| {
                let _ = std::process::Command::new("xdg-open").arg(&dir).spawn();
            });
        }
        row.add_suffix(&open);
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
