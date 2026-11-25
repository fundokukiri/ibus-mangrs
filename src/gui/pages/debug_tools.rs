//! The right-hand column of the debug page: the typing test, then the
//! diagnostics as titled groups of rows. A row carries a status dot, a label,
//! a value and, when it needs one, a line saying what to do — clicking it
//! copies the value, and a file row also has a button to open its folder.

use super::*;
use crate::gui::debug_report::{Cell, Section, Status};

pub(super) const TOOLS_CSS: &str = "
.diag-cell {
  padding: 5px 8px;
  border-radius: 8px;
}
.diag-cell:hover {
  background-color: alpha(@view_fg_color, 0.06);
}
.diag-warn {
  background-color: alpha(@warning_color, 0.16);
}
.diag-dot-ok { color: @success_color; }
.diag-dot-warn { color: @warning_color; }
.diag-dot-info { color: alpha(@view_fg_color, 0.45); }
.diag-group { font-weight: 700; }
.diag-hint { color: alpha(@view_fg_color, 0.8); }
.diag-summary { color: alpha(@view_fg_color, 0.65); }
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
    let body = gtk4::Box::new(Orientation::Vertical, 14);

    // --- Typing test ------------------------------------------------------
    let try_group = gtk4::Box::new(Orientation::Vertical, 5);
    try_group.append(&group_title(&t("misc.section_try", "Thử gõ")));
    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some(&t(
        "misc.debug_try_placeholder",
        "Gõ thử ở đây, log bên trái sẽ chạy",
    )));
    entry.set_tooltip_text(Some(&t(
        "tips.debug_try",
        "Gõ vào ô này để xem bộ gõ có nhận phím không: dòng mới xuất hiện bên trái nghĩa là phím đã đi qua IBus.",
    )));
    try_group.append(&entry);
    let buffer_line = gtk4::Box::new(Orientation::Horizontal, 6);
    let buffer_caption = gtk4::Label::new(Some(&t("misc.buffer_label", "buffer:")));
    buffer_caption.add_css_class("caption");
    buffer_caption.add_css_class("dim-label");
    let buffer = gtk4::Label::new(None);
    buffer.add_css_class("caption");
    buffer.set_xalign(0.0);
    buffer.set_hexpand(true);
    buffer.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let composing = gtk4::Label::new(None);
    composing.add_css_class("caption");
    composing.add_css_class("dim-label");
    buffer_line.append(&buffer_caption);
    buffer_line.append(&buffer);
    buffer_line.append(&composing);
    try_group.append(&buffer_line);
    body.append(&try_group);
    {
        let buffer = buffer.clone();
        entry.connect_changed(move |entry| buffer.set_markup(&buffer_markup(&entry.text())));
    }
    buffer.set_markup(&buffer_markup(""));

    // --- The groups -------------------------------------------------------
    let groups = gtk4::Box::new(Orientation::Vertical, 12);
    body.append(&groups);

    let refresh: Rc<dyn Fn()> = {
        let groups = groups.clone();
        let overlay = overlay.clone();
        let ui = ui.clone();
        Rc::new(move || {
            while let Some(child) = groups.first_child() {
                groups.remove(&child);
            }
            for section in facts() {
                groups.append(&group_widget(&section, &overlay, &ui));
            }
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

fn group_title(title: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(title));
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label.add_css_class("diag-group");
    label.set_xalign(0.0);
    label
}

fn group_widget(section: &Section, overlay: &adw::ToastOverlay, ui: &Rc<Ui>) -> gtk4::Widget {
    let group = gtk4::Box::new(Orientation::Vertical, 2);
    group.append(&group_title(&section.title));
    for row in &section.rows {
        group.append(&row_widget(row, overlay, ui));
    }
    group.upcast()
}

fn row_widget(cell: &Cell, overlay: &adw::ToastOverlay, ui: &Rc<Ui>) -> gtk4::Widget {
    let holder = gtk4::Box::new(Orientation::Vertical, 1);
    let btn = gtk4::Button::new();
    btn.add_css_class("flat");
    btn.add_css_class("diag-cell");
    if cell.status == Status::Warn {
        btn.add_css_class("diag-warn");
    }
    let row = gtk4::Box::new(Orientation::Horizontal, 6);
    let dot = gtk4::Label::new(Some("●"));
    dot.add_css_class("caption");
    dot.add_css_class(match cell.status {
        Status::Ok => "diag-dot-ok",
        Status::Warn => "diag-dot-warn",
        Status::Info => "diag-dot-info",
    });
    let label = gtk4::Label::new(Some(&cell.label));
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    let value = gtk4::Label::new(Some(&cell.value));
    value.set_xalign(1.0);
    value.set_hexpand(true);
    value.set_single_line_mode(true);
    value.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    row.append(&dot);
    row.append(&label);
    row.append(&value);
    if cell.open_dir.is_some() {
        // The words stay in the tooltip and the accessible label: the button
        // shows a folder, not a sentence.
        let open = gtk4::Button::from_icon_name("folder-open-symbolic");
        open.add_css_class("flat");
        open.add_css_class("circular");
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
        row.append(&open);
    }
    btn.set_child(Some(&row));
    let tip = cell.hint.as_deref().unwrap_or(&cell.value).to_string();
    btn.set_tooltip_text(Some(&tip));
    let accessible = format!("{}: {}. {}", cell.label, cell.value, tip);
    btn.update_property(&[gtk4::accessible::Property::Label(&accessible)]);
    {
        // A click copies the value the report would carry: the full path for a
        // file row, because that is what a helper needs.
        let copied = cell.hint.clone().unwrap_or_else(|| cell.value.clone());
        let overlay = overlay.clone();
        let ui = ui.clone();
        btn.connect_clicked(move |btn| {
            btn.clipboard().set_text(&copied);
            ui.fire_toast(&overlay, &t("misc.diag_copied", "Đã sao chép"));
        });
    }
    holder.append(&btn);
    if cell.status == Status::Warn {
        if let Some(hint) = &cell.hint {
            let line = gtk4::Label::new(Some(hint));
            line.add_css_class("caption");
            line.add_css_class("diag-hint");
            line.set_xalign(0.0);
            line.set_wrap(true);
            line.set_margin_start(8);
            line.set_margin_end(8);
            holder.append(&line);
        }
    }
    holder.upcast()
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
