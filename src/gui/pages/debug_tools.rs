//! The right-hand column of the debug page: the typing test, four rows of facts
//! about this install, and the two files the report carries. Every card is the
//! window's own `card` style, so the column reads like the settings tabs beside
//! it; only the icon squares, the buffer chip and the sparkline need rules of
//! their own.
//!
//! A fact is shown once and in one place: the rows are picked out of
//! `debug_report::sections` by their `id`, never by their translated label, so
//! the screen and the copied report cannot drift apart.

use super::*;
use crate::gui::debug_report::{Cell, Section, Status};

/// The column's fixed width: wide enough for two values side by side, narrow
/// enough to leave the log most of the page.
pub(super) const SIDE_WIDTH: i32 = 292;

pub(super) const TOOLS_CSS: &str = "
.diag-dot-ok { color: @success_color; }
.diag-dot-warn { color: @warning_color; }
.diag-dot-info { color: alpha(@view_fg_color, 0.55); }
/* The status square at the top of the page and the 28 px one in front of each
   fact: a tinted tile, never a loose glyph. */
.diag-head-icon {
  min-width: 34px;
  min-height: 34px;
  border-radius: 11px;
  background-color: alpha(@success_color, 0.16);
  color: @success_color;
}
.diag-head-icon-off {
  background-color: alpha(@error_color, 0.16);
  color: @error_color;
}
.diag-icon {
  min-width: 28px;
  min-height: 28px;
  border-radius: 8px;
  background-color: alpha(@view_fg_color, 0.06);
  color: alpha(@view_fg_color, 0.7);
}
.diag-icon-ok {
  background-color: alpha(@success_color, 0.16);
  color: @success_color;
}
.diag-icon-warn {
  background-color: alpha(@warning_color, 0.18);
  color: @warning_color;
}
.diag-ok { color: @success_color; font-weight: bold; }
.diag-warn-text { color: @warning_color; font-weight: bold; }
/* The cards of this column, and the log beside them. */
.diag-card { padding: 12px; }
/* What is being composed right now: a value, so give it a shape. */
.buffer-chip {
  background-color: alpha(@accent_bg_color, 0.18);
  border-radius: 6px;
  padding: 1px 6px;
}
/* Eight delays, oldest first: the header's sparkline. */
.diag-bar {
  background-color: alpha(@accent_bg_color, 0.7);
  border-radius: 2px;
  min-width: 4px;
}
.diag-value { font-feature-settings: \"tnum\"; }
/* The log's own frame: a rounded pane of its own, the one the page had before
   the rebuild. */
.log-pane {
  border-radius: 10px;
  border-width: 1px;
  border-style: solid;
  border-color: alpha(@view_fg_color, 0.1);
  background-color: alpha(@view_fg_color, 0.035);
}
.log-view {
  font-size: 12px;
}
.log-view, .log-view > text {
  background-color: transparent;
}
.log-chip {
  background-color: alpha(@accent_bg_color, 0.22);
  border-radius: 9px;
  padding: 1px 7px;
}
";

/// The column's fixed parts, so the page can wire the typing test and re-read
/// the rows without rebuilding the window.
pub(super) struct Tools {
    pub body: gtk4::Widget,
    /// Says whether the engine sees a word being composed.
    pub composing: gtk4::Label,
    pub refresh: Rc<dyn Fn()>,
}

/// Build the column. `facts` is asked again on every refresh, which is what
/// makes a restart show its result.
pub(super) fn build(
    ui: &Rc<Ui>,
    overlay: &adw::ToastOverlay,
    facts: Rc<dyn Fn() -> Vec<Section>>,
) -> Tools {
    let body = gtk4::Box::new(Orientation::Vertical, 14);
    body.set_width_request(SIDE_WIDTH);

    // --- Thử gõ -----------------------------------------------------------
    let try_card = card();
    try_card.append(&heading(&t("misc.section_try", "Thử gõ")));
    let entry = gtk4::Entry::new();
    entry.set_placeholder_text(Some(&t(
        "misc.debug_try_placeholder",
        "Gõ thử ở đây, log bên trái chạy theo",
    )));
    entry.set_size_request(-1, 36);
    entry.add_css_class("diag-entry");
    entry.set_tooltip_text(Some(&t(
        "tips.debug_try",
        "Gõ vào đây để xem bộ gõ có nhận phím không: dòng mới hiện bên trái nghĩa là phím đã đi qua IBus.",
    )));
    try_card.append(&entry);

    let strip = gtk4::Box::new(Orientation::Horizontal, 6);
    let keyboard = gtk4::Image::from_icon_name("input-keyboard-symbolic");
    keyboard.add_css_class("dim-label");
    let buffer_caption = gtk4::Label::new(Some(&t("misc.buffer_label", "buffer")));
    buffer_caption.add_css_class("caption");
    buffer_caption.add_css_class("dim-label");
    // The chip keeps its own line: the field is for typing, the chip is for what
    // the engine sees.
    let buffer = gtk4::Label::new(None);
    buffer.add_css_class("caption");
    buffer.add_css_class("monospace");
    buffer.add_css_class("buffer-chip");
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
    strip.append(&keyboard);
    strip.append(&buffer_caption);
    strip.append(&buffer);
    strip.append(&composing);
    try_card.append(&strip);
    body.append(&try_card);
    {
        let buffer = buffer.clone();
        entry.connect_changed(move |entry| buffer.set_markup(&buffer_markup(&entry.text())));
    }
    buffer.set_markup(&buffer_markup(""));

    // --- The facts --------------------------------------------------------
    let info_card = card();
    body.append(&info_card);
    let files_row = gtk4::Box::new(Orientation::Horizontal, 14);
    body.append(&files_row);

    // The cards are rebuilt only when a fact actually changed: tearing them
    // down twice a second made the whole page re-lay out while the user was
    // typing in the box above.
    let shown: Rc<RefCell<Vec<Section>>> = Rc::new(RefCell::new(Vec::new()));
    let refresh: Rc<dyn Fn()> = {
        let info_card = info_card.clone();
        let files_row = files_row.clone();
        let overlay = overlay.clone();
        let ui = ui.clone();
        let shown = Rc::clone(&shown);
        Rc::new(move || {
            let next = facts();
            if next == *shown.borrow() {
                return;
            }
            while let Some(child) = info_card.first_child() {
                info_card.remove(&child);
            }
            while let Some(child) = files_row.first_child() {
                files_row.remove(&child);
            }
            fill_facts(&info_card, &files_row, &next, &overlay, &ui);
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

/// A card of the column: the window's own card style with this column's tighter
/// insets.
fn card() -> gtk4::Box {
    let card = gtk4::Box::new(Orientation::Vertical, 8);
    card.add_css_class("card");
    card.add_css_class("diag-card");
    card
}

/// A card's own heading: small, upper case, muted, sentence case in the source.
fn heading(text: &str) -> gtk4::Label {
    let label = gtk4::Label::new(Some(&text.to_uppercase()));
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label
}

/// The underlined text after `buffer`. Pango markup, so the value is escaped.
pub(super) fn buffer_markup(text: &str) -> String {
    if text.is_empty() {
        return "<span alpha=\"55%\">—</span>".to_string();
    }
    format!("<u>{}</u>", gtk4::glib::markup_escape_text(text))
}

/// The fact with this id, wherever the report put it.
fn cell<'a>(sections: &'a [Section], id: &str) -> Option<&'a Cell> {
    sections
        .iter()
        .flat_map(|section| section.rows.iter())
        .find(|row| row.id == id)
}

/// The configured backend and the one the log names, joined the way the row
/// reads: `auto → uinput`.
fn backend_cell(sections: &[Section]) -> Option<Cell> {
    let configured = cell(sections, "backend")?;
    let actual = cell(sections, "actual")?;
    let mut joined = configured.clone();
    joined.value = format!("{} → {}", configured.value, actual.value);
    // Both rows carry the same warning when they disagree: the pair is the
    // evidence, and one of them alone says nothing.
    if actual.status == Status::Warn {
        joined.status = Status::Warn;
        joined.hint.clone_from(&actual.hint);
    }
    Some(joined)
}

/// Fill both cards from one reading of the facts.
fn fill_facts(
    info_card: &gtk4::Box,
    files_row: &gtk4::Box,
    sections: &[Section],
    overlay: &adw::ToastOverlay,
    ui: &Rc<Ui>,
) {
    let extension = cell(sections, "extension");
    let icon = match extension.map(|row| row.status) {
        Some(Status::Ok) => "emblem-ok-symbolic",
        Some(Status::Warn) => "dialog-warning-symbolic",
        _ => "dialog-information-symbolic",
    };
    let rows: [(&str, Option<&Cell>, Option<Cell>); 4] = [
        (icon, extension, cell(sections, "ibus").cloned()),
        (
            "computer-symbolic",
            cell(sections, "os"),
            cell(sections, "kernel").cloned(),
        ),
        (
            "video-display-symbolic",
            cell(sections, "desktop"),
            cell(sections, "session").cloned(),
        ),
        (
            "preferences-other-symbolic",
            cell(sections, "mode"),
            backend_cell(sections),
        ),
    ];
    for (icon, left, right) in rows {
        let Some(left) = left else { continue };
        info_card.append(&info_row(icon, left, right.as_ref()));
    }
    // What to do about a warning sits under the card, in the theme's warning
    // colour: inside the row it pushed the two values apart.
    for hint in warning_hints(sections) {
        let line = gtk4::Label::new(Some(&hint));
        line.add_css_class("caption");
        line.add_css_class("warning");
        line.set_xalign(0.0);
        line.set_wrap(true);
        info_card.append(&line);
    }
    for id in ["config", "log"] {
        if let Some(row) = cell(sections, id) {
            files_row.append(&file_card(row, overlay, ui));
        }
    }
}

/// The sentences behind the warnings this column shows, once each.
fn warning_hints(sections: &[Section]) -> Vec<String> {
    let shown = ["extension", "backend", "actual"];
    let mut out: Vec<String> = Vec::new();
    for id in shown {
        let Some(row) = cell(sections, id) else {
            continue;
        };
        if row.status != Status::Warn {
            continue;
        }
        let Some(hint) = &row.hint else { continue };
        if !out.contains(hint) {
            out.push(hint.clone());
        }
    }
    out
}

/// One fact: a tinted icon square and two values, each a muted name over a
/// monospace value that can be selected and copied with the mouse.
fn info_row(icon: &str, left: &Cell, right: Option<&Cell>) -> gtk4::Widget {
    let row = gtk4::Box::new(Orientation::Horizontal, 10);
    let tile = gtk4::Image::from_icon_name(icon);
    tile.add_css_class("diag-icon");
    if left.id == "extension" {
        if let Some(class) = status_class(left.status) {
            tile.add_css_class(class);
        }
    }
    let holder = gtk4::Box::new(Orientation::Horizontal, 0);
    holder.set_valign(gtk4::Align::Center);
    holder.append(&tile);
    row.append(&holder);
    row.append(&value_cell(left));
    if let Some(right) = right {
        row.append(&value_cell(right));
    }
    row.upcast()
}

/// The colour a status is drawn in, for the one row that has a state.
fn status_class(status: Status) -> Option<&'static str> {
    match status {
        Status::Ok => Some("diag-icon-ok"),
        Status::Warn => Some("diag-icon-warn"),
        Status::Info => None,
    }
}

fn value_cell(cell: &Cell) -> gtk4::Widget {
    let box_ = gtk4::Box::new(Orientation::Vertical, 1);
    box_.set_hexpand(true);
    let name = gtk4::Label::new(Some(&cell.label));
    name.add_css_class("caption");
    name.add_css_class("dim-label");
    name.set_xalign(0.0);
    // Wrapped, not cut: the column is 292 px wide, and two values share a row.
    name.set_wrap(true);
    let value = gtk4::Label::new(Some(&cell.value));
    value.add_css_class("caption");
    value.add_css_class("monospace");
    value.add_css_class("diag-value");
    value.set_xalign(0.0);
    value.set_selectable(true);
    value.set_wrap(true);
    value.set_tooltip_text(Some(&cell.value));
    // Only the extension has a state worth a colour: every other fact is either
    // this install or this machine, and nothing about it can be wrong.
    if cell.id == "extension" {
        if let Some(class) = match cell.status {
            Status::Ok => Some("diag-ok"),
            Status::Warn => Some("diag-warn-text"),
            Status::Info => None,
        } {
            value.add_css_class(class);
        }
    }
    let accessible = match &cell.hint {
        Some(hint) => format!("{}: {}. {hint}", cell.label, cell.value),
        None => format!("{}: {}", cell.label, cell.value),
    };
    box_.update_property(&[gtk4::accessible::Property::Label(&accessible)]);
    box_.append(&name);
    box_.append(&value);
    box_.upcast()
}

/// A file the report carries: its name, and a button for the full path.
fn file_card(cell: &Cell, overlay: &adw::ToastOverlay, ui: &Rc<Ui>) -> gtk4::Widget {
    let card = card();
    card.set_hexpand(true);
    let top = gtk4::Box::new(Orientation::Horizontal, 8);
    let icon = gtk4::Image::from_icon_name("text-x-generic-symbolic");
    icon.add_css_class("dim-label");
    top.append(&icon);
    let name = gtk4::Label::new(Some(&cell.value));
    name.add_css_class("caption");
    name.add_css_class("monospace");
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    top.append(&name);
    card.append(&top);
    let bottom = gtk4::Box::new(Orientation::Horizontal, 6);
    let caption = gtk4::Label::new(Some(&cell.label));
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    caption.set_xalign(0.0);
    caption.set_hexpand(true);
    bottom.append(&caption);
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
    bottom.append(&copy);
    card.append(&bottom);
    card.upcast()
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

    #[test]
    fn the_backend_row_joins_what_was_asked_for_and_what_ran() {
        let dir = crate::debug_log::TempLogDir::new("diag-backend");
        let path = dir.dir().join("config.toml");
        let facts = crate::gui::debug_report::Facts {
            machine: &crate::gui::machine::Machine {
                os: "Arch Linux".into(),
                kernel: "7.3".into(),
                desktop: "GNOME".into(),
                ibus: "1.5.35".into(),
            },
            preferred: crate::config::PreferredBackendConfig::Auto,
            mode: crate::config::ModeConfig::default(),
            extension: ("misc.extension_running", "Đang chạy", true),
            config_path: Some(&path),
            log_path: None,
            engine: None,
            backend: Some("uinput"),
            backend_why: None,
        };
        let sections = crate::gui::debug_report::sections(&facts);
        let joined = backend_cell(&sections).expect("the backend row");
        assert!(joined.value.contains('→'), "{}", joined.value);
        assert!(joined.value.ends_with("uinput"), "{}", joined.value);
        // Every id the column reads exists in the report.
        for id in [
            "extension",
            "ibus",
            "os",
            "kernel",
            "desktop",
            "session",
            "mode",
            "config",
            "log",
        ] {
            assert!(cell(&sections, id).is_some(), "no `{id}` row");
        }
    }
}
