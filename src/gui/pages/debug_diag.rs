//! The diagnostics grid: a dozen small boxes, each one a question about this
//! install with the answer and a colour. Built as a grid the page re-flows by
//! width, because the same twelve boxes have to work at 980px and at 700px.

use super::*;
use crate::gui::debug_report::{counts, Cell, Status};

/// Below this a box is too narrow to read; above it the extra width is better
/// spent on another column.
const MIN_CELL_WIDTH: f32 = 150.0;
const MAX_COLUMNS: i32 = 6;

/// The page's own left and right margins, which are not grid width.
const PAGE_MARGIN: i32 = 32;

pub(super) const DIAG_CSS: &str = "
.diag-cell {
  padding: 7px 10px;
  border-radius: 9px;
  background-color: alpha(@view_fg_color, 0.05);
}
.diag-warn {
  background-color: alpha(@warning_color, 0.16);
}
.diag-dot-ok { color: @success_color; }
.diag-dot-warn { color: @warning_color; }
.diag-dot-info { color: alpha(@view_fg_color, 0.45); }
.diag-summary { color: alpha(@view_fg_color, 0.65); }
";

/// Build the grid and return it with the function that re-reads every box.
pub(super) fn build(
    ui: &Rc<Ui>,
    overlay: &adw::ToastOverlay,
    page: &gtk4::Box,
    facts: Rc<dyn Fn() -> Vec<Cell>>,
) -> (gtk4::Widget, Rc<dyn Fn()>) {
    let summary = gtk4::Label::new(None);
    summary.add_css_class("caption");
    summary.add_css_class("diag-summary");
    let title = gtk4::Label::new(Some(&t("misc.diag_title", "Chẩn đoán")));
    title.add_css_class("heading");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    let header = gtk4::Box::new(Orientation::Horizontal, 8);
    header.append(&title);
    header.append(&summary);

    let grid = gtk4::Grid::builder()
        .column_spacing(8)
        .row_spacing(8)
        .column_homogeneous(true)
        .build();

    let body = gtk4::Box::new(Orientation::Vertical, 6);
    body.append(&header);
    body.append(&grid);

    // Where each box currently sits, so a resize can re-place the same widgets
    // without rebuilding them.
    let placed: Rc<RefCell<Vec<(gtk4::Widget, bool)>>> = Rc::new(RefCell::new(Vec::new()));
    let last_columns = Rc::new(std::cell::Cell::new(0i32));
    let columns_now = {
        let page = page.clone();
        Rc::new(move || columns_for(page.width() - PAGE_MARGIN))
    };
    let layout_now: Rc<dyn Fn()> = {
        let grid = grid.clone();
        let placed = Rc::clone(&placed);
        let columns_now = Rc::clone(&columns_now);
        let last_columns = Rc::clone(&last_columns);
        Rc::new(move || {
            let columns = columns_now();

            reflow(&grid, &placed.borrow(), columns);
            last_columns.set(columns);
        })
    };

    let refresh: Rc<dyn Fn()> = {
        let grid = grid.clone();
        let summary = summary.clone();
        let overlay = overlay.clone();
        let ui = ui.clone();
        let placed = Rc::clone(&placed);
        let layout_now = Rc::clone(&layout_now);
        Rc::new(move || {
            let list = facts();
            while let Some(child) = grid.first_child() {
                grid.remove(&child);
            }
            let mut widgets: Vec<(gtk4::Widget, bool)> = Vec::new();
            for cell in &list {
                let widget = cell_widget(cell, &overlay, &ui);
                widgets.push((widget.upcast(), cell.open_dir.is_some()));
            }
            *placed.borrow_mut() = widgets;
            layout_now();
            let (ok, warn) = counts(&list);
            summary.set_text(&format!(
                "{} {} · {} {}",
                ok,
                t("misc.diag_ok", "ổn"),
                warn,
                t("misc.diag_warn", "cần xem")
            ));
        })
    };
    {
        // The grid has no width of its own before its first allocation, so the
        // column count is re-checked on every frame the page draws. An integer
        // compare per frame is cheaper than any notify plumbing, and it is the
        // only hook GTK offers for "the width changed".
        let columns_now = Rc::clone(&columns_now);
        let last_columns = Rc::clone(&last_columns);
        let layout_now = Rc::clone(&layout_now);
        page.add_tick_callback(move |_, _| {
            let columns = columns_now();
            if columns != last_columns.get() {
                layout_now();
            }
            gtk4::glib::ControlFlow::Continue
        });
    }
    refresh();
    (body.upcast(), refresh)
}

/// How many boxes fit side by side at this width.
fn columns_for(width: i32) -> i32 {
    if width <= 0 {
        return MAX_COLUMNS;
    }
    ((width as f32 / MIN_CELL_WIDTH).floor() as i32).clamp(2, MAX_COLUMNS)
}

/// Where each box goes, as `(column, row, span)`: a path box takes two columns
/// and starts a new row rather than wrapping into half a box. Pure data, so the
/// layout can be checked without a display.
fn plan(wide: &[bool], columns: i32) -> Vec<(i32, i32, i32)> {
    let mut out = Vec::with_capacity(wide.len());
    let mut row = 0;
    let mut col = 0;
    for wide in wide {
        let span = if *wide { 2 } else { 1 };
        if col + span > columns {
            row += 1;
            col = 0;
        }
        out.push((col, row, span));
        col += span;
        if col >= columns {
            row += 1;
            col = 0;
        }
    }
    out
}

/// Put the boxes back into the grid at this column count. Returns the number of
/// rows used.
fn reflow(grid: &gtk4::Grid, placed: &[(gtk4::Widget, bool)], columns: i32) -> i32 {
    let wide: Vec<bool> = placed.iter().map(|(_, wide)| *wide).collect();
    let layout = plan(&wide, columns);
    for ((widget, _), (col, row, span)) in placed.iter().zip(layout.iter()) {
        // `attach` on a child the grid already holds keeps its old cell, so a
        // re-flow has to take it out first.
        if widget.parent().is_some_and(|parent| parent == *grid) {
            grid.remove(widget);
        }
        grid.attach(widget, *col, *row, *span, 1);
    }
    layout.iter().map(|(_, row, _)| row + 1).max().unwrap_or(0)
}

fn cell_widget(cell: &Cell, overlay: &adw::ToastOverlay, ui: &Rc<Ui>) -> gtk4::Button {
    let btn = gtk4::Button::new();
    btn.add_css_class("flat");
    btn.add_css_class("diag-cell");
    if cell.status == Status::Warn {
        btn.add_css_class("diag-warn");
    }
    let body = gtk4::Box::new(Orientation::Vertical, 2);
    let head = gtk4::Box::new(Orientation::Horizontal, 6);
    let dot = gtk4::Label::new(Some("●"));
    dot.add_css_class(match cell.status {
        Status::Ok => "diag-dot-ok",
        Status::Warn => "diag-dot-warn",
        Status::Info => "diag-dot-info",
    });
    let label = gtk4::Label::new(Some(&cell.label));
    label.add_css_class("caption");
    label.add_css_class("dim-label");
    label.set_xalign(0.0);
    label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    head.append(&dot);
    head.append(&label);
    let value = gtk4::Label::new(Some(&cell.value));
    value.set_xalign(0.0);
    value.set_hexpand(true);
    value.set_single_line_mode(true);
    value.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
    if cell.open_dir.is_some() {
        value.add_css_class("monospace");
        value.add_css_class("caption");
    }
    body.append(&head);
    body.append(&value);
    btn.set_child(Some(&body));
    btn.set_tooltip_text(Some(cell.hint.as_deref().unwrap_or(&cell.value)));
    let accessible = format!("{}: {}", cell.label, cell.value);
    btn.update_property(&[gtk4::accessible::Property::Label(&accessible)]);
    {
        let cell = cell.clone();
        let overlay = overlay.clone();
        let ui = ui.clone();
        btn.connect_clicked(move |btn| match &cell.open_dir {
            // A path box opens the folder that holds the file: the file itself
            // would launch an editor the user did not ask for.
            Some(dir) => {
                let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
            }
            None => {
                btn.clipboard().set_text(&cell.value);
                ui.fire_toast(&overlay, &t("misc.diag_copied", "Đã sao chép"));
            }
        });
    }
    btn
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_column_count_follows_the_width() {
        assert_eq!(columns_for(0), MAX_COLUMNS);
        assert_eq!(columns_for(300), 2, "a narrow window keeps two boxes");
        assert_eq!(columns_for(470), 3);
        assert_eq!(columns_for(760), 5);
        assert_eq!(columns_for(4000), MAX_COLUMNS);
    }

    /// A wide box never ends a row half full: it moves down instead.
    #[test]
    fn a_path_box_wraps_to_the_next_row_instead_of_splitting() {
        // Three one-column boxes fill row 0; the two-column box then starts
        // row 1 rather than being squeezed into the last free column.
        let layout = plan(&[false, false, false, true], 3);
        assert_eq!(layout[3], (0, 1, 2));
        assert_eq!(
            plan(&[false, true, false], 3),
            vec![(0, 0, 1), (1, 0, 2), (0, 1, 1)]
        );
    }

    #[test]
    fn a_healthy_install_counts_as_ok_and_a_warning_as_needing_a_look() {
        let ok = Cell {
            label: "Engine".to_string(),
            value: "đang chạy".to_string(),
            status: Status::Ok,
            hint: None,
            open_dir: None,
        };
        let warn = Cell {
            status: Status::Warn,
            ..ok.clone()
        };
        let info = Cell {
            status: Status::Info,
            ..ok.clone()
        };
        assert_eq!(counts(&[ok, warn, info]), (1, 1));
    }
}
