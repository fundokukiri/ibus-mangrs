//! Page "Gỡ lỗi": the tools that only exist while debug mode is on — the log
//! file (size, folder, clear), a filtered tail viewer that follows the file,
//! and a diagnostics block whose report can be pasted into an issue.
//!
//! The switch that turns the log on stays in "Cài đặt nâng cao": this page is
//! the consequence of that switch, not a second place to set it.

use super::*;
use crate::gui::debug_report::{
    diagnostic_info, engine_rows, non_default_config, report, transport_from_log,
};
use crate::gui::engine_probe;
use crate::gui::log_tail::{highlight, size_label, tail, visible_lines, TAIL_BYTES};

/// Lines the viewer holds. More than anyone reads in one screen, few enough
/// that the per-second re-read stays invisible.
const VIEW_LINES: usize = 500;

/// Lines the copied report carries: enough context around the failure, short
/// enough to paste into an issue without trimming.
const REPORT_LINES: usize = 50;

/// Size plus mtime of the log as last read: the poller re-renders only when
/// the file moved, so an idle page costs one `stat` a second and nothing else.
type Stamp = Option<(u64, Option<std::time::SystemTime>)>;

/// The preset filters. The log is plain `<ms> <text>` with no categories, so a
/// chip is only the set of words its family writes — and the engine writes
/// both Vietnamese and English words, hence the pairs.
fn preset_terms() -> Vec<(String, Vec<String>)> {
    vec![
        (
            t("misc.debug_chip_tone", "Dấu"),
            vec!["dấu".to_string(), "tone".to_string()],
        ),
        (
            t("misc.debug_chip_backspace", "Backspace"),
            vec!["backspace".to_string(), "delete".to_string()],
        ),
        (
            t("misc.debug_chip_mode", "Chế độ"),
            vec![
                "preedit".to_string(),
                "surround".to_string(),
                "flow".to_string(),
                "forward".to_string(),
            ],
        ),
        (
            t("misc.debug_chip_transport", "Cách chèn"),
            vec![
                "uinput".to_string(),
                "xtest".to_string(),
                "libei".to_string(),
                "wlr".to_string(),
            ],
        ),
        (t("misc.debug_chip_error", "Lỗi"), error_terms()),
    ]
}

/// The words a failure writes. Shared with the red summary row, which counts
/// them whatever filter is on: a failure is worth a mention even while the
/// user is looking at something else.
fn error_terms() -> Vec<String> {
    ["error", "panic", "failed", "denied", "refused"]
        .iter()
        .map(|w| w.to_string())
        .collect()
}

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
    let page = adw::PreferencesPage::new();
    let toast_overlay = adw::ToastOverlay::new();
    let path = crate::debug_log::log_path();
    // Current filter: the typed text is one term, a chip is a whole family.
    let terms: Rc<RefCell<Vec<String>>> = Rc::new(RefCell::new(Vec::new()));
    // Set while the page scrolls itself, so its own scroll is not read as the
    // user scrolling away from the tail.
    let scrolling_self = Rc::new(Cell::new(false));

    // --- Ghi log ----------------------------------------------------------
    let log_group = adw::PreferencesGroup::builder()
        .title(t("groups.debug.log.title", "Tệp log"))
        .build();
    let log_row = action_row(
        &t("rows.debug_log_file.title", "Nhật ký gỡ lỗi"),
        &log_subtitle(path.as_deref()),
    );
    let open_btn = action_button(
        Some("folder-open-symbolic"),
        &t("misc.debug_open_folder", "Mở thư mục"),
    );
    let clear_btn = action_button(
        Some("user-trash-symbolic"),
        &t("misc.debug_clear_log", "Xoá log…"),
    );
    log_row.add_suffix(&open_btn);
    log_row.add_suffix(&clear_btn);
    log_group.add(&log_row);
    page.add(&log_group);
    {
        let path = path.clone();
        open_btn.connect_clicked(move |_| {
            let Some(dir) = path.as_deref().and_then(|p| p.parent()) else {
                return;
            };
            // The folder only: opening the log itself would launch an editor.
            let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
        });
    }
    {
        let path = path.clone();
        let row = log_row.clone();
        clear_btn.connect_clicked(move |btn| {
            let Some(path) = path.clone() else {
                return;
            };
            let dialog = adw::AlertDialog::builder()
                .heading(t("debug_clear.heading", "Xoá nhật ký gỡ lỗi?"))
                .body(t(
                    "debug_clear.body",
                    "Tệp log bị xoá hẳn. Engine ghi lại từ đầu nếu chế độ gỡ lỗi vẫn bật.",
                ))
                .build();
            dialog.add_response("cancel", &t("debug_clear.cancel", "Huỷ"));
            dialog.set_default_response(Some("cancel"));
            dialog.set_close_response("cancel");
            dialog.add_response("delete", &t("debug_clear.delete", "Xoá"));
            dialog.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
            {
                let path = path.clone();
                let row = row.clone();
                dialog.connect_response(Some("delete"), move |_, _| {
                    let _ = std::fs::remove_file(&path);
                    row.set_subtitle(&log_subtitle(Some(&path)));
                });
            }
            dialog.present(btn.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }

    // --- Xem log ----------------------------------------------------------
    let viewer_group = adw::PreferencesGroup::builder()
        .title(t("groups.debug.viewer.title", "Xem log"))
        .build();
    let filter = gtk4::SearchEntry::new();
    filter.set_placeholder_text(Some(&t("misc.debug_filter_placeholder", "Lọc dòng…")));
    filter.set_hexpand(true);
    filter.set_tooltip_text(Some(&t(
        "tips.debug_filter",
        "Gõ một từ để lọc, hoặc chọn nút bên cạnh để lọc cả nhóm dòng.",
    )));
    let follow = gtk4::CheckButton::with_label(&t("misc.debug_follow", "Theo dõi"));
    follow.set_active(true);
    follow.set_tooltip_text(Some(&t(
        "tips.debug_follow",
        "Tự cuộn xuống cuối khi log có dòng mới. Cuộn lên thì tự tạm dừng.",
    )));
    let tools = gtk4::Box::new(Orientation::Horizontal, 12);
    tools.set_margin_top(6);
    tools.set_margin_bottom(6);
    tools.set_margin_start(12);
    tools.set_margin_end(12);
    tools.append(&filter);
    tools.append(&follow);
    viewer_group.add(&tools);

    let chips = gtk4::Box::new(Orientation::Horizontal, 6);
    chips.set_margin_bottom(6);
    chips.set_margin_start(12);
    chips.set_margin_end(12);
    let mut chip_buttons: Vec<(gtk4::ToggleButton, Vec<String>)> = Vec::new();
    for (index, (label, terms_of)) in preset_terms().into_iter().enumerate() {
        let chip = gtk4::ToggleButton::with_label(&label);
        chip.add_css_class("flat");
        chip.add_css_class("caption");
        // Grouped toggles: one family at a time, like a segmented control.
        if index > 0 {
            if let Some((first, _)) = chip_buttons.first() {
                chip.set_group(Some(first));
            }
        }
        chips.append(&chip);
        chip_buttons.push((chip, terms_of));
    }
    viewer_group.add(&chips);

    let error_row = action_row(&t("rows.debug_errors.title", "Lỗi trong log"), "");
    // The icon carries the meaning: a theme is free to ignore the class.
    error_row.add_prefix(&gtk4::Image::from_icon_name("dialog-error-symbolic"));
    error_row.add_css_class("error");
    error_row.set_visible(false);
    viewer_group.add(&error_row);

    let view = gtk4::TextView::new();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_wrap_mode(gtk4::WrapMode::None);
    view.set_left_margin(12);
    view.set_right_margin(12);
    view.set_top_margin(8);
    view.set_bottom_margin(8);
    let scroller = gtk4::ScrolledWindow::builder()
        .min_content_height(260)
        .vexpand(true)
        .child(&view)
        .build();
    viewer_group.add(&scroller);
    page.add(&viewer_group);

    // The viewer re-reads only while it is on screen: a hidden page must not
    // touch the file once a second for a window the user is not looking at.
    let last_seen: Rc<RefCell<Stamp>> = Rc::new(RefCell::new(None));
    let render: Rc<dyn Fn()> = {
        let view = view.clone();
        let scroller = scroller.clone();
        let follow = follow.clone();
        let terms = Rc::clone(&terms);
        let scrolling_self = Rc::clone(&scrolling_self);
        let path = path.clone();
        let error_row = error_row.clone();
        Rc::new(move || {
            let lines = path
                .as_deref()
                .map(|p| tail(p, VIEW_LINES, TAIL_BYTES))
                .unwrap_or_default();
            let current = terms.borrow().clone();
            let shown = visible_lines(&lines, &current);
            view.buffer().set_text(&shown.join("\n"));
            highlight(&view.buffer(), &current);
            // The red row counts what the user is looking for when they have
            // not said what that is: failures always earn a mention.
            let failures = visible_lines(
                &lines,
                &preset_terms().pop().map(|(_, t)| t).unwrap_or_default(),
            );
            error_row.set_visible(!failures.is_empty());
            if !failures.is_empty() {
                error_row.set_subtitle(&format!(
                    "{} {}",
                    failures.len(),
                    t("rows.debug_errors.unit", "dòng")
                ));
            }
            if follow.is_active() {
                scrolling_self.set(true);
                let adj = scroller.vadjustment();
                adj.set_value(adj.upper() - adj.page_size());
                scrolling_self.set(false);
            }
        })
    };
    {
        // A chip replaces the typed filter (they would otherwise AND into
        // nothing); typing again clears the chips.
        let updating = Rc::new(Cell::new(false));
        {
            let render = render.clone();
            let terms = Rc::clone(&terms);
            let filter = filter.clone();
            let chip_buttons: Vec<(gtk4::ToggleButton, Vec<String>)> = chip_buttons.clone();
            let updating = Rc::clone(&updating);
            filter.connect_search_changed(move |entry| {
                if updating.get() {
                    return;
                }
                updating.set(true);
                for (chip, _) in &chip_buttons {
                    chip.set_active(false);
                }
                updating.set(false);
                let text = entry.text().to_string();
                *terms.borrow_mut() = if text.trim().is_empty() {
                    Vec::new()
                } else {
                    vec![text]
                };
                render();
            });
        }
        for (chip, family) in chip_buttons.clone() {
            let render = render.clone();
            let terms = Rc::clone(&terms);
            let filter = filter.clone();
            let updating = Rc::clone(&updating);
            chip.connect_toggled(move |chip| {
                if updating.get() {
                    return;
                }
                updating.set(true);
                if chip.is_active() {
                    filter.set_text("");
                }
                updating.set(false);
                *terms.borrow_mut() = if chip.is_active() {
                    family.clone()
                } else {
                    Vec::new()
                };
                render();
            });
        }
    }
    {
        // Scrolling up while following means "let me read this": stop
        // following rather than yank the view back down.
        let follow = follow.clone();
        let scrolling_self = Rc::clone(&scrolling_self);
        scroller.vadjustment().connect_value_changed(move |adj| {
            if scrolling_self.get() || !follow.is_active() {
                return;
            }
            if adj.value() + adj.page_size() < adj.upper() - 4.0 {
                follow.set_active(false);
            }
        });
    }
    {
        let render = Rc::clone(&render);
        let seen = Rc::clone(&last_seen);
        let view = view.clone();
        let path = path.clone();
        gtk4::glib::timeout_add_seconds_local(1, move || {
            if !view.is_mapped() {
                return gtk4::glib::ControlFlow::Continue;
            }
            let stamp = path
                .as_deref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| (m.len(), m.modified().ok()));
            if stamp.is_some() && *seen.borrow() != stamp {
                render();
                *seen.borrow_mut() = stamp;
            }
            gtk4::glib::ControlFlow::Continue
        });
    }
    {
        // Ctrl+F jumps to the filter, F5 re-reads now instead of next second.
        let keys = gtk4::EventControllerKey::new();
        keys.set_propagation_phase(gtk4::PropagationPhase::Capture);
        let filter = filter.clone();
        let render = Rc::clone(&render);
        keys.connect_key_pressed(move |_, key, _, state| {
            let ctrl = state.contains(gtk4::gdk::ModifierType::CONTROL_MASK);
            match key {
                gtk4::gdk::Key::f if ctrl => {
                    filter.grab_focus();
                    gtk4::glib::Propagation::Stop
                }
                gtk4::gdk::Key::F5 => {
                    render();
                    gtk4::glib::Propagation::Stop
                }
                _ => gtk4::glib::Propagation::Proceed,
            }
        });
        page.add_controller(keys);
    }

    // --- Chẩn đoán --------------------------------------------------------
    let diag_group = adw::PreferencesGroup::builder()
        .title(t("groups.debug.diag.title", "Chẩn đoán"))
        .build();
    let engine_slot: Vec<adw::ActionRow> = (0..5)
        .map(|_| action_row(&t("rows.debug_engine.title", "Engine"), ""))
        .collect();
    for row in &engine_slot {
        diag_group.add(row);
    }
    let transport_row = action_row(&t("rows.debug_transport.title", "Cách chèn thực tế"), "?");
    diag_group.add(&transport_row);
    for (label, value) in diagnostic_info(ui, path.as_deref()) {
        diag_group.add(&action_row(&label, &value));
    }
    let copy_row = action_row(
        &t("rows.debug_report.title", "Báo cáo"),
        &t(
            "rows.debug_report.subtitle",
            "Sao chép thông tin và log cuối",
        ),
    );
    let refresh_btn = action_button(
        Some("view-refresh-symbolic"),
        &t("misc.debug_refresh", "Đọc lại"),
    );
    let copy_btn = action_button(
        Some("edit-copy-symbolic"),
        &t("misc.debug_copy_report", "Sao chép"),
    );
    copy_row.add_suffix(&refresh_btn);
    copy_row.add_suffix(&copy_btn);
    diag_group.add(&copy_row);
    page.add(&diag_group);

    // The engine rows are read from outside the engine (its process and IBus),
    // so they are refreshed on every visit and on demand — never polled.
    let refresh_engine: Rc<dyn Fn()> = {
        let rows = engine_slot.clone();
        let transport_row = transport_row.clone();
        let path = path.clone();
        Rc::new(move || {
            let status = engine_probe::probe();
            for (row, (label, value)) in rows.iter().zip(engine_rows(status.as_ref())) {
                row.set_title(&label);
                row.set_subtitle(&value);
            }
            let lines = path
                .as_deref()
                .map(|p| tail(p, VIEW_LINES, TAIL_BYTES))
                .unwrap_or_default();
            transport_row
                .set_subtitle(&transport_from_log(&lines).unwrap_or_else(|| "?".to_string()));
        })
    };
    refresh_engine();
    {
        let refresh = Rc::clone(&refresh_engine);
        page.connect_map(move |_| refresh());
    }
    {
        let refresh = Rc::clone(&refresh_engine);
        refresh_btn.connect_clicked(move |_| refresh());
    }
    {
        let overlay = toast_overlay.clone();
        let ui = ui.clone();
        copy_btn.connect_clicked(move |btn| {
            let lines = path
                .as_deref()
                .map(|p| tail(p, REPORT_LINES, TAIL_BYTES))
                .unwrap_or_default();
            let config = non_default_config(&ui.read());
            let text = report(&diagnostic_info(&ui, path.as_deref()), &config, &lines);
            btn.clipboard().set_text(&text);
            ui.fire_toast(&overlay, &t("debug_report.copied", "Đã sao chép báo cáo"));
        });
    }

    toast_overlay.set_child(Some(&page));
    toast_overlay
}

/// "<size> · <path>" — what the log costs and where it is. A log that does not
/// exist yet says so instead of showing a zero.
fn log_subtitle(path: Option<&std::path::Path>) -> String {
    let Some(path) = path else {
        return t("misc.debug_log_missing", "Không có đường dẫn log").to_string();
    };
    match std::fs::metadata(path) {
        Ok(meta) => format!("{} · {}", size_label(meta.len()), path.display()),
        Err(_) => format!(
            "{} · {}",
            t("misc.debug_log_absent", "chưa ghi"),
            path.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    #[test]
    fn the_log_subtitle_reports_size_then_absence() {
        let dir = TempDir::new("mangrs-debug-subtitle");
        let path = dir.path().join("debug.log");
        assert!(log_subtitle(Some(&path)).contains("chưa ghi"));
        std::fs::write(&path, vec![b'x'; 2000]).expect("write log");
        let shown = log_subtitle(Some(&path));
        assert!(shown.starts_with("2 KB · "), "{shown}");
    }

    #[test]
    fn every_preset_chip_carries_words() {
        for (label, terms) in preset_terms() {
            assert!(!terms.is_empty(), "chip {label} filters nothing");
            assert!(
                terms.iter().all(|t| !t.trim().is_empty()),
                "chip {label} has a blank word"
            );
        }
    }
}
