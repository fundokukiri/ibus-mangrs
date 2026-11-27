//! The right-hand column of the debug page: the typing test, then the facts as
//! three blocks — a grid of tiles for the system, the insertion path as two
//! pills, and the two files with a copy button each. Every value can be
//! selected and copied with the mouse; a fact that can be wrong (the extension)
//! is the only one that carries a colour.
//!
//! A fact is shown once and in one place: the rows are picked out of
//! `debug_report::sections` by their `id`, never by their translated label, so
//! the screen and the copied report cannot drift apart.

use super::*;
use crate::gui::debug_report::{Cell, Section, Status};

/// The column's fixed width: wide enough for two tiles side by side, narrow
/// enough to leave the log most of the page.
pub(super) const SIDE_WIDTH: i32 = 292;

pub(super) const TOOLS_CSS: &str = "
.diag-dot-ok { color: @success_color; }
.diag-dot-warn { color: @warning_color; }
.diag-ok { color: @success_color; }
.diag-warn-text { color: @warning_color; }
/* The status square at the top of the page: a tinted tile, never a loose
   glyph. */
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
/* The cards of this column, and the log beside them. */
.diag-card { padding: 8px; }
/* One fact: name over value, a shade lighter than the card it sits on. */
.diag-tile {
  background-color: alpha(@view_fg_color, 0.06);
  border-radius: 8px;
  padding: 4px 8px;
}
/* A file row: the name, and a button for the full path. */
.diag-file {
  background-color: alpha(@view_fg_color, 0.06);
  border-radius: 8px;
  padding: 3px 6px;
}
/* What is being composed right now. */
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
/* Six kind buttons share one row with the pause switch: their padding decides
   whether the page can be narrowed at all. */
.diag-filters button {
  padding: 2px 7px;
  min-height: 22px;
}
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
";

/// The column's fixed parts, so the page can wire the typing test and re-read
/// the facts without rebuilding the window.
pub(super) struct Tools {
    pub body: gtk4::Box,
    /// The typing test. The page puts it under the log, because that is what it
    /// exercises.
    pub try_box: gtk4::Widget,
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
    let body = gtk4::Box::new(Orientation::Vertical, 10);
    body.add_css_class("diag-side");
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
    // The chip hugs its value: stretched across the card it read as a progress
    // bar instead of as the word being composed.
    buffer.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    buffer.set_tooltip_text(Some(&t(
        "tips.debug_buffer",
        "Chữ đang có trong ô thử, lấy ngay từ ô chứ không phải từ bộ gõ.",
    )));
    let composing = gtk4::Label::new(None);
    composing.add_css_class("caption");
    composing.add_css_class("dim-label");
    composing.set_hexpand(true);
    composing.set_halign(gtk4::Align::End);
    strip.append(&keyboard);
    strip.append(&buffer_caption);
    strip.append(&buffer);
    strip.append(&composing);
    try_card.append(&strip);
    {
        let buffer = buffer.clone();
        entry.connect_changed(move |entry| buffer.set_markup(&buffer_markup(&entry.text())));
    }
    buffer.set_markup(&buffer_markup(""));

    // --- Hệ thống ---------------------------------------------------------
    let system_card = card();
    system_card.append(&heading(&t("misc.section_system", "Hệ thống")));
    let grid = gtk4::Grid::builder()
        .column_spacing(8)
        .row_spacing(6)
        .build();
    grid.set_hexpand(true);
    system_card.append(&grid);
    let hints = gtk4::Box::new(Orientation::Vertical, 2);
    system_card.append(&hints);
    body.append(&system_card);

    // --- Cách chèn --------------------------------------------------------
    let insert_card = card();
    insert_card.append(&heading(&t("misc.section_insert", "Cách chèn")));
    let insert_grid = gtk4::Grid::builder()
        .column_spacing(8)
        .row_spacing(6)
        .build();
    insert_grid.set_hexpand(true);
    insert_card.append(&insert_grid);
    body.append(&insert_card);

    // --- Tệp --------------------------------------------------------------
    let files_card = card();
    files_card.append(&heading(&t("misc.section_files", "Tệp")));
    let files_box = gtk4::Box::new(Orientation::Vertical, 4);
    files_card.append(&files_box);
    body.append(&files_card);

    // The blocks are rebuilt only when a fact actually changed: tearing them
    // down twice a second made the whole page re-lay out while the user was
    // typing in the box above.
    let shown: Rc<RefCell<Vec<Section>>> = Rc::new(RefCell::new(Vec::new()));
    let refresh: Rc<dyn Fn()> = {
        let grid = grid.clone();
        let hints = hints.clone();
        let insert_grid = insert_grid.clone();
        let files_box = files_box.clone();
        let overlay = overlay.clone();
        let ui = ui.clone();
        let shown = Rc::clone(&shown);
        Rc::new(move || {
            let next = facts();
            if next == *shown.borrow() {
                return;
            }
            fill_blocks(
                &grid,
                &hints,
                &insert_grid,
                &files_box,
                &next,
                &overlay,
                &ui,
            );
            *shown.borrow_mut() = next;
        })
    };
    refresh();
    Tools {
        body,
        try_box: try_card.upcast(),
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

/// A card's own heading: small, upper case, muted.
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

/// The configured backend and the one the log names, joined the way the pill
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

/// The grid's reading order: the local facts first, then the machine, then the
/// session they were typed in.
const TILES: [&str; 6] = ["ibus", "extension", "os", "kernel", "desktop", "session"];

/// Fill the three blocks from one reading of the facts.
fn fill_blocks(
    grid: &gtk4::Grid,
    hints: &gtk4::Box,
    insert_grid: &gtk4::Grid,
    files_box: &gtk4::Box,
    sections: &[Section],
    overlay: &adw::ToastOverlay,
    ui: &Rc<Ui>,
) {
    while let Some(child) = grid.first_child() {
        grid.remove(&child);
    }
    while let Some(child) = hints.first_child() {
        hints.remove(&child);
    }
    while let Some(child) = insert_grid.first_child() {
        insert_grid.remove(&child);
    }
    while let Some(child) = files_box.first_child() {
        files_box.remove(&child);
    }

    for (index, id) in TILES.iter().enumerate() {
        let Some(row) = cell(sections, id) else {
            continue;
        };
        let index = index as i32;
        grid.attach(&tile(row), index % 2, index / 2, 1, 1);
    }
    // What to do about a warning sits under the grid, in the theme's warning
    // colour: inside a tile it pushed the value out of line.
    for hint in warning_hints(sections) {
        let line = gtk4::Label::new(Some(&hint));
        line.add_css_class("caption");
        line.add_css_class("warning");
        line.set_xalign(0.0);
        line.set_wrap(true);
        hints.append(&line);
    }
    // The same tiles as the block above: the insertion path is two facts, not
    // two settings.
    if let Some(mode) = cell(sections, "mode") {
        insert_grid.attach(&tile(mode), 0, 0, 1, 1);
    }
    if let Some(backend) = backend_cell(sections) {
        insert_grid.attach(&tile(&backend), 1, 0, 1, 1);
    }
    for id in ["config", "log"] {
        let Some(row) = cell(sections, id) else {
            continue;
        };
        files_box.append(&file_row(row, overlay, ui));
    }
}

/// The sentences behind the warnings this column shows, once each.
fn warning_hints(sections: &[Section]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for id in ["extension", "backend", "actual"] {
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

/// One fact: its name over a monospace value the mouse can select. The
/// extension is the only one with a state, so it is the only one with a dot and
/// a colour.
fn tile(cell: &Cell) -> gtk4::Widget {
    let tile = gtk4::Box::new(Orientation::Vertical, 1);
    tile.add_css_class("diag-tile");
    tile.set_hexpand(true);
    let name = gtk4::Label::new(Some(&cell.label));
    name.add_css_class("caption");
    name.add_css_class("dim-label");
    name.set_xalign(0.0);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    tile.append(&name);
    let line = gtk4::Box::new(Orientation::Horizontal, 5);
    if let Some(class) = dot_class(cell) {
        let dot = gtk4::Label::new(Some("●"));
        dot.add_css_class("caption");
        dot.add_css_class(class);
        line.append(&dot);
    }
    let value = gtk4::Label::new(Some(&cell.value));
    value.add_css_class("caption");
    value.add_css_class("monospace");
    value.add_css_class("diag-value");
    value.set_xalign(0.0);
    value.set_selectable(true);
    // Wrapped, not clipped: a tile is 128 px wide, and "Gạch chân (preedit)"
    // would otherwise end in an ellipsis that says nothing.
    value.set_wrap(true);
    value.set_tooltip_text(Some(cell.hint.as_deref().unwrap_or(&cell.value)));
    if cell.id == "extension" {
        value.add_css_class(match cell.status {
            Status::Ok => "diag-ok",
            Status::Warn => "diag-warn-text",
            Status::Info => "diag-ok",
        });
    }
    line.append(&value);
    tile.append(&line);
    let accessible = format!("{}: {}", cell.label, cell.value);
    tile.update_property(&[gtk4::accessible::Property::Label(&accessible)]);
    tile.upcast()
}

/// Which facts carry a dot. The extension's dot says whether it runs; the kernel
/// and the session carry the same green, on the owner's reading of the two facts
/// that are simply fine.
fn dot_class(cell: &Cell) -> Option<&'static str> {
    match cell.id {
        "extension" => Some(match cell.status {
            Status::Ok => "diag-dot-ok",
            Status::Warn => "diag-dot-warn",
            Status::Info => "diag-dot-info",
        }),
        "kernel" | "session" => Some("diag-dot-ok"),
        _ => None,
    }
}

/// A file the report carries: its name, and a button for the full path.
fn file_row(cell: &Cell, overlay: &adw::ToastOverlay, ui: &Rc<Ui>) -> gtk4::Widget {
    let row = gtk4::Box::new(Orientation::Horizontal, 8);
    row.add_css_class("diag-file");
    row.set_valign(gtk4::Align::Center);
    let caption = gtk4::Label::new(Some(&cell.label));
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    caption.set_xalign(0.0);
    row.append(&caption);
    let name = gtk4::Label::new(Some(&cell.value));
    name.add_css_class("caption");
    name.add_css_class("monospace");
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    name.set_tooltip_text(Some(cell.hint.as_deref().unwrap_or(&cell.value)));
    row.append(&name);
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
    row.append(&copy);
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

    #[test]
    fn the_backend_pill_joins_what_was_asked_for_and_what_ran() {
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
        // Every id the column draws exists in the report.
        for id in TILES.iter().chain(["mode", "config", "log"].iter()) {
            assert!(cell(&sections, id).is_some(), "no `{id}` row");
        }
    }
}
