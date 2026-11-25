//! Page "Gỡ lỗi": one screen of tools that only exist while debug mode is on.
//! A toolbar over the log file, the log itself as the main area, the
//! diagnostics grid, and a footer that restarts IBus or copies a report.
//!
//! The switch that turns the log on stays in "Cài đặt nâng cao": this page is
//! the consequence of that switch, not a second place to set it.

use super::*;
use crate::gui::debug_report::{cells, transport_from_log, Facts};
use crate::gui::engine_probe;
use crate::gui::log_decode::{mask_private, parse, Kind, Line, CHIPS};
use crate::gui::log_tail::{size_label, tail, Reader, TAIL_BYTES};

/// Lines the viewer keeps. Beyond this the file is history, and holding more
/// only makes the page heavy.
const VIEW_LINES: usize = 5_000;

/// The lines the copied report carries.
const REPORT_LINES: usize = 50;

/// Page-local styling: a box that reads as a box, and the dot colours taken
/// from the theme so light and dark both work.
const DEBUG_CSS: &str = crate::gui::pages::debug_diag::DIAG_CSS;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
    let page = gtk4::Box::new(Orientation::Vertical, 10);
    page.set_margin_top(12);
    page.set_margin_bottom(12);
    page.set_margin_start(16);
    page.set_margin_end(16);
    let toast_overlay = adw::ToastOverlay::new();
    let path = crate::debug_log::log_path();
    {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(DEBUG_CSS);
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    // The log reader (it only reads what was appended), the chips the user has
    // on, the typed filter, and how many lines arrived while paused.
    let reader = Rc::new(RefCell::new(Reader::new(path.clone(), VIEW_LINES)));
    let pending = Rc::new(Cell::new(0usize));
    let picked: Rc<RefCell<Vec<Kind>>> = Rc::new(RefCell::new(Vec::new()));
    let query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));
    let updating = Rc::new(Cell::new(false));

    // --- Toolbar ----------------------------------------------------------
    let filter = gtk4::SearchEntry::new();
    filter.set_placeholder_text(Some(&t("misc.debug_filter_placeholder", "Lọc dòng")));
    filter.set_hexpand(true);
    filter.set_tooltip_text(Some(&t(
        "tips.debug_filter",
        "Gõ một phần nội dung để lọc, không phân biệt hoa thường.",
    )));
    let follow = gtk4::ToggleButton::new();
    follow.set_icon_name("media-playback-pause-symbolic");
    follow.set_active(true);
    follow.add_css_class("flat");
    set_button_label(
        &follow,
        &t("misc.debug_pause", "Tạm dừng cuộn"),
        &t("misc.debug_pause", "Tạm dừng cuộn"),
    );
    // How many lines arrived while the view is paused.
    let arrived = gtk4::Label::new(None);
    arrived.add_css_class("caption");
    arrived.add_css_class("accent");
    arrived.set_visible(false);
    let open_btn = icon_button(
        "folder-open-symbolic",
        &t("misc.debug_open_folder", "Mở thư mục"),
    );
    let clear_btn = icon_button("user-trash-symbolic", &t("misc.debug_clear_log", "Xoá log"));
    let toolbar = gtk4::Box::new(Orientation::Horizontal, 6);
    toolbar.append(&filter);
    toolbar.append(&follow);
    toolbar.append(&arrived);
    toolbar.append(&open_btn);
    toolbar.append(&clear_btn);
    page.append(&toolbar);

    // --- Chips ------------------------------------------------------------
    let chip_row = gtk4::FlowBox::builder()
        .selection_mode(gtk4::SelectionMode::None)
        .max_children_per_line(6)
        .min_children_per_line(1)
        .row_spacing(4)
        .column_spacing(4)
        .build();
    let mut chips: Vec<(gtk4::ToggleButton, Option<Kind>)> = Vec::new();
    let all_chip = gtk4::ToggleButton::with_label(&t("misc.chip_all", "Tất cả"));
    all_chip.add_css_class("flat");
    all_chip.add_css_class("caption");
    all_chip.set_active(true);
    chip_row.insert(&all_chip, -1);
    chips.push((all_chip, None));
    for kind in CHIPS {
        let chip = gtk4::ToggleButton::with_label(&kind_label(kind));
        chip.add_css_class("flat");
        chip.add_css_class("caption");
        if kind == Kind::Error {
            chip.add_css_class("diag-chip-error");
        }
        chip_row.insert(&chip, -1);
        chips.push((chip, Some(kind)));
    }
    page.append(&chip_row);

    // --- Log view ---------------------------------------------------------
    let view = gtk4::TextView::new();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    view.set_wrap_mode(gtk4::WrapMode::None);
    view.set_left_margin(10);
    view.set_right_margin(10);
    view.set_top_margin(6);
    view.set_bottom_margin(6);
    make_tags(&view.buffer(), &view);
    let scroller = gtk4::ScrolledWindow::builder()
        .min_content_height(110)
        .vexpand(true)
        .child(&view)
        .build();
    page.append(&scroller);
    let auto_scroll = Rc::new(Cell::new(false));
    let render: Rc<dyn Fn()> = {
        let reader = Rc::clone(&reader);
        let picked = Rc::clone(&picked);
        let query = Rc::clone(&query);
        let view = view.clone();
        let scroller = scroller.clone();
        let follow = follow.clone();
        let auto_scroll = Rc::clone(&auto_scroll);
        let chips = chips.clone();
        let open_btn = open_btn.clone();
        let clear_btn = clear_btn.clone();
        let path = path.clone();
        Rc::new(move || {
            let parsed = parse_all(&reader.borrow().lines());
            let mut counts = [0usize; 6];
            for line in &parsed {
                counts[kind_index(line.kind)] += 1;
            }
            let on = picked.borrow().clone();
            let needle = query.borrow().to_lowercase();
            let shown: Vec<&Line> = parsed
                .iter()
                .filter(|line| on.is_empty() || on.contains(&line.kind))
                .filter(|line| needle.is_empty() || line.plain().to_lowercase().contains(&needle))
                .collect();
            draw(&view, &shown);
            // Chip labels carry their own count, so the row doubles as a
            // summary of what the file holds.
            for (chip, kind) in &chips {
                let count = match kind {
                    None => parsed.len(),
                    Some(kind) => counts[kind_index(*kind)],
                };
                let label = match kind {
                    None => t("misc.chip_all", "Tất cả"),
                    Some(kind) => kind_label(*kind),
                };
                chip.set_label(&format!("{label} {count}"));
            }
            if let Some((chip, _)) = chips.iter().find(|(_, k)| k.is_none()) {
                chip.set_active(picked.borrow().is_empty());
            }
            if follow.is_active() {
                auto_scroll.set(true);
                let adj = scroller.vadjustment();
                adj.set_value(adj.upper() - adj.page_size());
                auto_scroll.set(false);
            }
            // Same values, in a tooltip: the page has no room for a row that
            // only says where the file is.
            if let Some(path) = path.as_deref() {
                let size = std::fs::metadata(path)
                    .map(|m| size_label(m.len()))
                    .unwrap_or_else(|_| t("misc.debug_log_absent", "chưa ghi"));
                let tip = format!("{} · {}", size, path.display());
                open_btn.set_tooltip_text(Some(&format!(
                    "{}\n{tip}",
                    t("misc.debug_open_folder", "Mở thư mục")
                )));
                clear_btn.set_tooltip_text(Some(&format!(
                    "{}\n{tip}",
                    t("misc.debug_clear_log", "Xoá log")
                )));
            }
        })
    };
    {
        let render = Rc::clone(&render);
        let query = Rc::clone(&query);
        filter.connect_search_changed(move |entry| {
            *query.borrow_mut() = entry.text().to_string();
            render();
        });
    }
    {
        for (chip, kind) in chips.clone() {
            let render = Rc::clone(&render);
            let picked = Rc::clone(&picked);
            let updating = Rc::clone(&updating);
            let chips_inner = chips.clone();
            chip.connect_toggled(move |chip| {
                if updating.get() {
                    return;
                }
                updating.set(true);
                match kind {
                    None => picked.borrow_mut().clear(),
                    Some(kind) => {
                        let mut on = picked.borrow_mut();
                        if chip.is_active() {
                            if !on.contains(&kind) {
                                on.push(kind);
                            }
                        } else {
                            on.retain(|k| *k != kind);
                        }
                    }
                }
                for (other, k) in &chips_inner {
                    if k.is_none() {
                        other.set_active(picked.borrow().is_empty());
                    }
                }
                updating.set(false);
                render();
            });
        }
    }
    {
        // The pause button follows the state, not the click: scrolling up in
        // the view turns following off, and the button has to say so.
        let follow2 = follow.clone();
        let auto_scroll = Rc::clone(&auto_scroll);
        scroller.vadjustment().connect_value_changed(move |adj| {
            if auto_scroll.get() || !follow2.is_active() {
                return;
            }
            if adj.value() + adj.page_size() < adj.upper() - 4.0 {
                follow2.set_active(false);
            }
        });
    }
    {
        let follow = follow.clone();
        follow.connect_toggled(move |btn| {
            let on = btn.is_active();
            btn.set_icon_name(if on {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            });
        });
    }
    {
        // Open the folder, not the file: the file would launch an editor.
        let path = path.clone();
        open_btn.connect_clicked(move |_| {
            if let Some(dir) = path.as_deref().and_then(|p| p.parent()) {
                let _ = std::process::Command::new("xdg-open").arg(dir).spawn();
            }
        });
    }
    {
        let path = path.clone();
        let render = Rc::clone(&render);
        let reader_clear = Rc::clone(&reader);
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
                let render = Rc::clone(&render);
                let path = path.clone();
                let reader = Rc::clone(&reader_clear);
                dialog.connect_response(Some("delete"), move |_, _| {
                    let _ = std::fs::remove_file(&path);
                    reader.borrow_mut().reset();
                    render();
                });
            }
            dialog.present(btn.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }

    // --- Diagnostics ------------------------------------------------------
    let facts: Rc<dyn Fn() -> Vec<crate::gui::debug_report::Cell>> = {
        let ui = ui.clone();
        let path = path.clone();
        let reader = Rc::clone(&reader);
        Rc::new(move || {
            let config = ui.read();
            let extension = extension_state();
            let (key, fallback) = extension_row_for(extension).subtitle;
            let engine = engine_probe::probe();
            let config_path = Config::path();
            let transport = transport_from_log(&reader.borrow().lines());
            cells(&Facts {
                preferred: config.preferred_backend,
                mode: config.mode,
                extension: (key, fallback, extension == ExtensionState::Running),
                config_path: config_path.as_deref(),
                log_path: path.as_deref(),
                engine: engine.as_ref(),
                transport: transport.as_deref(),
            })
        })
    };
    let (diag, refresh_diag) =
        super::debug_diag::build(ui, &toast_overlay, &page, Rc::clone(&facts));
    page.append(&diag);

    // --- Footer -----------------------------------------------------------
    let restart_btn = gtk4::Button::with_label(&t("misc.restart_button", "Khởi động lại IBus"));
    restart_btn.set_tooltip_text(Some(&t(
        "tips.restart_engine",
        "Bộ gõ ngắt trong giây lát ở mọi ứng dụng, rồi chạy lại. Có hỏi lại trước khi làm.",
    )));
    // Icon and words in one child: `set_label` plus `set_icon_name` keeps only
    // the icon on this GTK version.
    let copy_btn = gtk4::Button::new();
    let copy_child = gtk4::Box::new(Orientation::Horizontal, 6);
    copy_child.append(&gtk4::Image::from_icon_name("edit-copy-symbolic"));
    copy_child.append(&gtk4::Label::new(Some(&t(
        "misc.debug_copy_report",
        "Sao chép báo cáo",
    ))));
    copy_btn.set_child(Some(&copy_child));
    let hide_switch =
        gtk4::CheckButton::with_label(&t("misc.debug_hide_text", "Ẩn nội dung gõ khi sao chép"));
    hide_switch.set_active(true);
    let hide_hint = gtk4::Label::new(Some(&t(
        "misc.debug_hide_hint",
        "Tắt thì báo cáo kèm văn bản bạn đang gõ.",
    )));
    hide_hint.add_css_class("caption");
    hide_hint.add_css_class("dim-label");
    hide_hint.set_wrap(true);
    hide_hint.set_xalign(0.0);
    let footer = gtk4::Box::new(Orientation::Horizontal, 8);
    let footer_text = gtk4::Box::new(Orientation::Vertical, 2);
    footer_text.append(&hide_switch);
    footer_text.append(&hide_hint);
    let spacer = gtk4::Box::new(Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    footer.append(&restart_btn);
    footer.append(&copy_btn);
    footer.append(&spacer);
    footer.append(&footer_text);
    page.append(&footer);

    // Report and restart: wired below so the layout above stays readable.
    {
        let ui = ui.clone();
        let overlay = toast_overlay.clone();
        let path = path.clone();
        let hide_switch_copy = hide_switch.clone();
        copy_btn.connect_clicked(move |btn| {
            let last = report_log_lines(path.as_deref(), hide_switch_copy.is_active());
            let text = crate::gui::debug_report::report(&facts(), &config_lines(&ui), &last);
            btn.clipboard().set_text(&text);
            ui.fire_toast(&overlay, &t("debug_report.copied", "Đã sao chép báo cáo"));
        });
    }
    {
        // One poll reads only what the engine appended. While following, the
        // view jumps to the new lines; while paused, the number waits in the
        // toolbar until the user asks for it.
        let poll: Rc<dyn Fn(bool)> = {
            let reader = Rc::clone(&reader);
            let render = Rc::clone(&render);
            let refresh_diag = Rc::clone(&refresh_diag);
            let follow = follow.clone();
            let arrived = arrived.clone();
            let pending = Rc::clone(&pending);
            Rc::new(move |first: bool| {
                let added = reader.borrow_mut().poll();
                if first {
                    render();
                    refresh_diag();
                    return;
                }
                if added == 0 {
                    return;
                }
                if follow.is_active() {
                    render();
                    pending.set(0);
                    arrived.set_visible(false);
                } else {
                    pending.set(pending.get() + added);
                    arrived.set_text(&format!(
                        "+{} {}",
                        pending.get(),
                        t("misc.debug_new_lines", "dòng mới")
                    ));
                    arrived.set_visible(true);
                }
            })
        };
        {
            let poll = Rc::clone(&poll);
            page.connect_map(move |_| poll(true));
        }
        {
            let poll = Rc::clone(&poll);
            let view = view.clone();
            gtk4::glib::timeout_add_seconds_local(1, move || {
                if view.is_mapped() {
                    poll(false);
                }
                gtk4::glib::ControlFlow::Continue
            });
        }
        // Asking to follow again shows what arrived and clears the count.
        {
            let render = Rc::clone(&render);
            let arrived = arrived.clone();
            let pending = Rc::clone(&pending);
            follow.connect_toggled(move |btn| {
                if btn.is_active() {
                    pending.set(0);
                    arrived.set_visible(false);
                    render();
                }
            });
        }
    }

    // The restart goes through the same script as the maintenance button on
    // "Cài đặt nâng cao": it also brings the ibus-x11 bridge back, which a bare
    // `ibus restart` leaves dead for XIM applications.
    {
        let ui = ui.clone();
        let overlay = toast_overlay.clone();
        let refresh_diag = Rc::clone(&refresh_diag);
        restart_btn.connect_clicked(move |btn| {
            let dialog = adw::AlertDialog::builder()
                .heading(t("debug_restart.heading", "Khởi động lại IBus?"))
                .body(t(
                    "debug_restart.body",
                    "Bộ gõ ngắt trong giây lát ở mọi ứng dụng, chữ đang gõ dở sẽ mất.",
                ))
                .build();
            dialog.add_response("cancel", &t("debug_clear.cancel", "Huỷ"));
            dialog.add_response("restart", &t("misc.restart_button", "Khởi động lại IBus"));
            dialog.set_response_appearance("restart", adw::ResponseAppearance::Suggested);
            dialog.set_default_response(Some("cancel"));
            dialog.set_close_response("cancel");
            {
                let ui = ui.clone();
                let overlay = overlay.clone();
                let refresh_diag = Rc::clone(&refresh_diag);
                let btn = btn.clone();
                dialog.connect_response(Some("restart"), move |_, _| {
                    if let Err(e) = super::other::restart_ibus() {
                        ui.fire_toast(
                            &overlay,
                            &format!("{}: {e}", t("debug_restart.failed", "Không chạy được")),
                        );
                        return;
                    }
                    btn.set_sensitive(false);
                    btn.set_label(&t("debug_restart.running", "Đang khởi động lại…"));
                    ui.fire_toast(
                        &overlay,
                        &t("debug_restart.started", "Đang khởi động lại IBus…"),
                    );
                    // The daemon needs a moment; report what actually happened
                    // instead of assuming the restart worked.
                    let ticks = Rc::new(std::cell::Cell::new(0u32));
                    let ui = ui.clone();
                    let overlay = overlay.clone();
                    let refresh_diag = Rc::clone(&refresh_diag);
                    let btn = btn.clone();
                    gtk4::glib::timeout_add_local(
                        std::time::Duration::from_millis(500),
                        move || {
                            ticks.set(ticks.get() + 1);
                            let back = engine_probe::probe().is_some();
                            if back || ticks.get() >= 20 {
                                btn.set_sensitive(true);
                                btn.set_label(&t("misc.restart_button", "Khởi động lại IBus"));
                                refresh_diag();
                                let text = if back {
                                    t("debug_restart.done", "IBus đã chạy lại")
                                } else {
                                    t("debug_restart.timeout", "Chưa thấy IBus chạy lại")
                                };
                                ui.fire_toast(&overlay, &text);
                                return gtk4::glib::ControlFlow::Break;
                            }
                            gtk4::glib::ControlFlow::Continue
                        },
                    );
                });
            }
            dialog.present(btn.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }

    toast_overlay.set_child(Some(&page));
    toast_overlay
}

/// The log tail the report carries. With hiding on, the two fields that hold
/// what the user typed (`text=` and `tail=`) lose their content: the report
/// travels to issue trackers, and a chat window may be a password field.
fn report_log_lines(path: Option<&std::path::Path>, hide_text: bool) -> Vec<String> {
    let lines = path
        .map(|p| tail(p, REPORT_LINES, TAIL_BYTES))
        .unwrap_or_default();
    if hide_text {
        lines.iter().map(|line| mask_private(line)).collect()
    } else {
        lines
    }
}

/// What `config.toml` held, line by line: the report quotes the file the
/// engine actually loaded rather than what the widgets currently show.
fn config_lines(ui: &Rc<Ui>) -> Vec<String> {
    if let Some(text) = Config::path().and_then(|p| std::fs::read_to_string(p).ok()) {
        return text.lines().map(|l| l.to_string()).collect();
    }
    toml::to_string_pretty(&ui.read())
        .map(|text| text.lines().map(|l| l.to_string()).collect())
        .unwrap_or_default()
}

/// Parse the whole tail once, keeping the gap between neighbouring lines.
fn parse_all(raw: &[String]) -> Vec<Line> {
    let mut out = Vec::with_capacity(raw.len());
    let mut prev = None;
    for line in raw {
        if let Some(parsed) = parse(line, prev) {
            prev = parsed.abs_ms.or(prev);
            out.push(parsed);
        }
    }
    out
}

fn kind_index(kind: Kind) -> usize {
    match kind {
        Kind::Key => 0,
        Kind::Backspace => 1,
        Kind::Insert => 2,
        Kind::Flush => 3,
        Kind::Error => 4,
        Kind::Other => 5,
    }
}

fn kind_label(kind: Kind) -> String {
    match kind {
        Kind::Key => t("misc.chip_key", "Phím"),
        Kind::Backspace => t("misc.chip_backspace", "Backspace"),
        Kind::Insert => t("misc.chip_insert", "Chèn"),
        Kind::Flush => t("misc.chip_flush", "Flush"),
        Kind::Error => t("misc.chip_error", "Lỗi"),
        Kind::Other => t("misc.chip_other", "Khác"),
    }
}

/// Draw the selected lines: relative time, coloured kind, decoded body.
fn draw(view: &gtk4::TextView, lines: &[&Line]) {
    let buffer = view.buffer();
    buffer.set_text("");
    let press = t("misc.key_press", "nhấn");
    let release = t("misc.key_release", "thả");
    let width = CHIPS
        .iter()
        .map(|k| kind_label(*k).len())
        .max()
        .unwrap_or(4);
    let mut iter = buffer.start_iter();
    for line in lines {
        let when = match line.rel_ms {
            Some(ms) => format!("+{ms}ms"),
            None => String::new(),
        };
        buffer.insert_with_tags_by_name(&mut iter, &format!("{when:>8} "), &["log-time"]);
        let label = kind_label(line.kind);
        buffer.insert_with_tags_by_name(
            &mut iter,
            &format!("{label:<width$} "),
            &[tag_for(line.kind), "log-label"],
        );
        buffer.insert(&mut iter, &line.body.text(&press, &release));
        buffer.insert(&mut iter, "\n");
    }
}

fn tag_for(kind: Kind) -> &'static str {
    match kind {
        Kind::Key => "log-key",
        Kind::Backspace => "log-backspace",
        Kind::Insert => "log-insert",
        Kind::Flush => "log-flush",
        Kind::Error => "log-error",
        Kind::Other => "log-other",
    }
}

/// The tag colours come from the theme, so the page follows light and dark.
fn make_tags(buffer: &gtk4::TextBuffer, view: &gtk4::TextView) {
    #[allow(deprecated)]
    let color = |name: &str, alpha: f32| {
        let rgba = view
            .style_context()
            .lookup_color(name)
            .unwrap_or(gtk4::gdk::RGBA::new(0.5, 0.5, 0.5, 1.0));
        gtk4::gdk::RGBA::new(rgba.red(), rgba.green(), rgba.blue(), alpha)
    };
    for (tag, name, alpha) in [
        ("log-key", "accent_color", 1.0),
        ("log-backspace", "warning_color", 1.0),
        ("log-insert", "success_color", 1.0),
        ("log-flush", "view_fg_color", 0.75),
        ("log-error", "error_color", 1.0),
        ("log-other", "view_fg_color", 0.6),
        ("log-time", "view_fg_color", 0.45),
        ("log-label", "view_fg_color", 1.0),
    ] {
        if let Some(created) = buffer.create_tag(Some(tag), &[]) {
            created.set_foreground_rgba(Some(&color(name, alpha)));
        }
    }
}

/// An icon-only toolbar button: tooltip and accessible label carry the words
/// the button does not show.
fn icon_button(icon: &str, label: &str) -> gtk4::Button {
    let btn = gtk4::Button::from_icon_name(icon);
    btn.add_css_class("flat");
    set_button_label(&btn, label, label);
    btn
}

fn set_button_label<W: IsA<gtk4::Widget> + IsA<gtk4::Accessible>>(btn: &W, label: &str, tip: &str) {
    btn.set_tooltip_text(Some(tip));
    btn.update_property(&[gtk4::accessible::Property::Label(label)]);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw() -> Vec<String> {
        vec![
            "1795619563084 key keyval=0x69 state=0x0".to_string(),
            "1795619563090 key keyval=0xff08 state=0x40000000".to_string(),
            "1795619563154 timed commit flush (per-key)".to_string(),
            "1795619563158 surrounding cursor=3 tail=\"Gia\" text=\"Gia\"".to_string(),
            "1795619563160 delete-before inject FAILED → protocol=-1".to_string(),
            "1795619563164 mode: preedit".to_string(),
        ]
    }

    #[test]
    fn every_kind_has_its_own_slot_and_label() {
        let mut seen = Vec::new();
        for kind in CHIPS.iter().copied() {
            let index = kind_index(kind);
            assert!(!seen.contains(&index), "two kinds share slot {index}");
            seen.push(index);
            assert!(!kind_label(kind).is_empty());
        }
        assert_eq!(kind_index(Kind::Other), 5, "the other slot stays free");
    }

    #[test]
    fn hiding_keeps_the_typed_text_out_of_the_report() {
        let dir = crate::test_support::TempDir::new("mangrs-report-hide");
        let path = dir.path().join("debug.log");
        std::fs::write(
            &path,
            "1795619563158 surrounding cursor=3 tail=\"Gia\" text=\"Gia\"\n\
             1795619563160 key keyval=0x69 state=0x0\n",
        )
        .expect("write log");

        let hidden = report_log_lines(Some(&path), true);
        let joined = hidden.join("\n");
        assert!(!joined.contains("Gia"), "the typed text is gone: {joined}");
        assert!(joined.contains("cursor=3"), "the shape is kept: {joined}");
        assert!(
            joined.contains("key keyval=0x69"),
            "keys are not private: {joined}"
        );

        let plain = report_log_lines(Some(&path), false);
        assert!(plain.join("\n").contains("Gia"), "off means the text stays");
    }

    #[test]
    fn the_tail_parses_into_one_line_per_raw_line() {
        let parsed = parse_all(&raw());
        assert_eq!(parsed.len(), 6);
        assert_eq!(parsed[1].kind, Kind::Key);
        assert_eq!(parsed[4].kind, Kind::Error);
        assert_eq!(parsed[3].kind, Kind::Insert);
    }
}
