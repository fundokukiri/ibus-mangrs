//! Page "Gỡ lỗi": one screen of tools that only exist while debug mode is on.
//! A status bar on top, the log on the left, the typing test and the
//! diagnostics on the right, and a footer that restarts IBus or copies a
//! report. Below `NARROW_WIDTH` the right column drops under the log.
//!
//! The switch that turns the log on stays in "Cài đặt nâng cao": this page is
//! the consequence of that switch, not a second place to set it.

use super::*;
use crate::gui::debug_report::{backend_from_log, sections, Facts, Section};
use crate::gui::engine_probe;
use crate::gui::latency::{self, Stats};
use crate::gui::log_decode::{composing, mask_private, parse, Kind, Line, CHIPS};
use crate::gui::log_tail::{size_label, tail, Reader, TAIL_BYTES};

/// Lines the viewer keeps. Beyond this the file is history, and holding more
/// only makes the page heavy.
const VIEW_LINES: usize = 5_000;

/// The lines the copied report carries.
const REPORT_LINES: usize = 50;

/// Under this width the two columns cannot both be read, so the right one
/// moves below the log.
const NARROW_WIDTH: i32 = 760;

/// The columns split about 1.5 to 1, within a band where both stay readable.
const SHARE_RATIO: f32 = 2.5;
const MIN_RIGHT_WIDTH: i32 = 300;
const MAX_RIGHT_WIDTH: i32 = 560;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
    let path = crate::debug_log::log_path();
    let toast_overlay = adw::ToastOverlay::new();
    {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(super::debug_tools::TOOLS_CSS);
        if let Some(display) = gtk4::gdk::Display::default() {
            gtk4::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
    }

    // The log reader (it only reads what was appended), the kind on, the typed
    // filter, and how many lines arrived while paused.
    let reader = Rc::new(RefCell::new(Reader::new(path.clone(), VIEW_LINES)));
    // The decoded log, kept between polls: re-decoding five thousand lines a
    // second cost about 9 ms, and redrawing all of them about 105 ms.
    let parsed: Rc<RefCell<Vec<Line>>> = Rc::new(RefCell::new(Vec::new()));
    // How many of the shown lines are already in the buffer, and under which
    // filter they were drawn.
    let drawn = Rc::new(Cell::new(0usize));
    let drawn_key: Rc<RefCell<(Option<Kind>, String)>> =
        Rc::new(RefCell::new((None, String::new())));
    let pending = Rc::new(Cell::new(0usize));
    let kind_filter: Rc<RefCell<Option<Kind>>> = Rc::new(RefCell::new(None));
    let query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    // --- Status bar -------------------------------------------------------
    let dot = gtk4::Label::new(Some("●"));
    dot.add_css_class("diag-dot-info");
    let state = gtk4::Label::new(Some(&t("misc.status_unknown", "Chưa rõ engine")));
    state.add_css_class("caption");
    state.add_css_class("diag-group");
    let detail = gtk4::Label::new(None);
    detail.add_css_class("caption");
    detail.add_css_class("dim-label");
    detail.set_xalign(0.0);
    detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let metric_avg = gtk4::Label::new(None);
    let metric_max = gtk4::Label::new(None);
    let metric_log = gtk4::Label::new(None);
    for metric in [&metric_avg, &metric_max, &metric_log] {
        metric.add_css_class("caption");
        metric.add_css_class("monospace");
    }
    let status_spacer = gtk4::Box::new(Orientation::Horizontal, 0);
    status_spacer.set_hexpand(true);
    let status = gtk4::Box::new(Orientation::Horizontal, 8);
    status.append(&dot);
    status.append(&state);
    status.append(&detail);
    status.append(&status_spacer);
    status.append(&metric_avg);
    status.append(&metric_max);
    status.append(&metric_log);

    // --- Left column: toolbar over the log --------------------------------
    let filter = gtk4::SearchEntry::new();
    filter.set_placeholder_text(Some(&t("misc.debug_filter_placeholder", "Lọc dòng")));
    filter.set_hexpand(true);
    filter.set_tooltip_text(Some(&t(
        "tips.debug_filter",
        "Gõ một phần nội dung để lọc, không phân biệt hoa thường.",
    )));
    let names: Vec<String> = std::iter::once(t("misc.chip_all", "Tất cả"))
        .chain(CHIPS.iter().map(|kind| kind_label(*kind)))
        .collect();
    let name_refs: Vec<&str> = names.iter().map(String::as_str).collect();
    let kind_pick = gtk4::DropDown::from_strings(&name_refs);
    kind_pick.add_css_class("diag-kind");
    kind_pick.set_tooltip_text(Some(&t(
        "tips.debug_kind",
        "Chỉ hiện một loại dòng. Dòng không thuộc loại nào chỉ hiện khi chọn Tất cả.",
    )));
    kind_pick.update_property(&[gtk4::accessible::Property::Label(&t(
        "misc.debug_kind",
        "Loại dòng",
    ))]);
    let follow = gtk4::ToggleButton::new();
    follow.set_icon_name("media-playback-pause-symbolic");
    follow.set_active(true);
    follow.add_css_class("flat");
    set_button_label(
        &follow,
        &t("misc.debug_pause", "Tạm dừng cuộn"),
        &t("misc.debug_pause", "Tạm dừng cuộn"),
    );
    let arrived = gtk4::Label::new(None);
    arrived.add_css_class("caption");
    arrived.add_css_class("accent");
    arrived.set_visible(false);
    let clear_btn = icon_button("user-trash-symbolic", &t("misc.debug_clear_log", "Xoá log"));
    let toolbar = gtk4::Box::new(Orientation::Horizontal, 6);
    toolbar.append(&filter);
    toolbar.append(&kind_pick);
    toolbar.append(&follow);
    toolbar.append(&arrived);
    toolbar.append(&clear_btn);

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
    let left = gtk4::Box::new(Orientation::Vertical, 6);
    left.set_hexpand(true);
    left.append(&toolbar);
    left.append(&scroller);

    // --- The facts the rows and the report read ---------------------------
    let facts: Rc<dyn Fn() -> Vec<Section>> = {
        let ui = ui.clone();
        let path = path.clone();
        let reader = Rc::clone(&reader);
        Rc::new(move || {
            let config = ui.read();
            let extension = extension_state();
            let (key, fallback) = extension_row_for(extension).subtitle;
            let engine = engine_probe::probe();
            let log_lines = reader.borrow().recent(200);
            sections(&Facts {
                preferred: config.preferred_backend,
                mode: config.mode,
                extension: (key, fallback, extension == ExtensionState::Running),
                config_path: Config::path().as_deref(),
                log_path: path.as_deref(),
                engine: engine.as_ref(),
                backend: backend_from_log(&log_lines).as_deref(),
            })
        })
    };

    // --- Right column -----------------------------------------------------
    let tools = super::debug_tools::build(ui, &toast_overlay, Rc::clone(&facts));

    // --- The two columns --------------------------------------------------
    let separator = gtk4::Separator::new(Orientation::Vertical);
    let right = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .child(&tools.body)
        .build();
    right.set_width_request(MIN_RIGHT_WIDTH);
    // Without this the column's natural width is its longest label, and the
    // box would hand it half the page instead of the fixed share on the right.
    right.set_propagate_natural_width(false);
    right.set_hexpand(false);
    right.set_vexpand(true);
    let split = gtk4::Box::new(Orientation::Horizontal, 12);
    split.append(&left);
    split.append(&separator);
    split.append(&right);

    // --- Footer -----------------------------------------------------------
    let restart_btn = gtk4::Button::with_label(&t("misc.restart_button", "Khởi động lại IBus"));
    restart_btn.set_tooltip_text(Some(&t(
        "tips.restart_engine",
        "Khởi động lại IBus, bộ gõ sẽ ngắt trong giây lát ở mọi ứng dụng.",
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
    let hide_switch = gtk4::CheckButton::with_label(&t(
        "misc.debug_hide_text",
        "Ẩn nội dung gõ và macro khi sao chép",
    ));
    hide_switch.set_active(true);
    let hide_hint = gtk4::Label::new(Some(&t(
        "misc.debug_hide_hint",
        "Tắt sẽ đưa cả macro và nội dung gõ vào báo cáo.",
    )));
    hide_hint.add_css_class("caption");
    hide_hint.add_css_class("dim-label");
    hide_hint.set_wrap(true);
    hide_hint.set_xalign(0.0);
    let footer = gtk4::Box::new(Orientation::Horizontal, 8);
    let footer_text = gtk4::Box::new(Orientation::Vertical, 2);
    footer_text.append(&hide_switch);
    footer_text.append(&hide_hint);
    let footer_spacer = gtk4::Box::new(Orientation::Horizontal, 0);
    footer_spacer.set_hexpand(true);
    footer.append(&footer_text);
    footer.append(&footer_spacer);
    footer.append(&restart_btn);
    footer.append(&copy_btn);

    // --- The page ---------------------------------------------------------
    let page = gtk4::Box::new(Orientation::Vertical, 10);
    page.set_margin_top(12);
    page.set_margin_bottom(12);
    page.set_margin_start(16);
    page.set_margin_end(16);
    page.set_vexpand(true);
    page.append(&status);
    page.append(&split);
    page.append(&footer);

    // The page never scrolls while the columns fit; the stacked column below
    // `NARROW_WIDTH` is taller than the window, and this keeps it reachable.
    let outer = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .child(&page)
        .build();

    // --- Rendering --------------------------------------------------------
    let render: Rc<dyn Fn()> = {
        let parsed = Rc::clone(&parsed);
        let drawn = Rc::clone(&drawn);
        let drawn_key = Rc::clone(&drawn_key);
        let kind_filter = Rc::clone(&kind_filter);
        let query = Rc::clone(&query);
        let view = view.clone();
        let scroller = scroller.clone();
        let follow = follow.clone();
        let metric_avg = metric_avg.clone();
        let metric_max = metric_max.clone();
        let metric_log = metric_log.clone();
        let composing_label = tools.composing.clone();
        let path = path.clone();
        Rc::new(move || {
            let cache = parsed.borrow();
            let kind = *kind_filter.borrow();
            let needle = query.borrow().to_lowercase();
            let shown: Vec<&Line> = cache
                .iter()
                .filter(|line| kind.is_none_or(|k| line.kind == k))
                .filter(|line| needle.is_empty() || line.plain().to_lowercase().contains(&needle))
                .collect();
            // Appending the handful of new lines keeps this in the microsecond
            // range; anything else (a new filter, a cleared log) starts over.
            let same_filter = *drawn_key.borrow() == (kind, needle.clone());
            if same_filter && shown.len() >= drawn.get() {
                draw_more(&view, &shown[drawn.get()..]);
            } else {
                draw_all(&view, &shown);
                *drawn_key.borrow_mut() = (kind, needle.clone());
            }
            drawn.set(shown.len());
            // The two latency numbers are measured once, from the same walk.
            let stats = latency::measure(&cache);
            let pick = |f: fn(&Stats) -> u64| match stats {
                Some(stats) => format!("{}ms", f(&stats)),
                None => "—".to_string(),
            };
            metric_avg.set_text(&format!(
                "{} {}",
                t("misc.metric_avg", "Trễ TB"),
                pick(|s| s.avg_ms)
            ));
            metric_max.set_text(&format!(
                "{} {}",
                t("misc.metric_max", "Tối đa"),
                pick(|s| s.max_ms)
            ));
            let size = path
                .as_deref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|m| size_label(m.len()))
                .unwrap_or_else(|| "—".to_string());
            metric_log.set_text(&format!("{} {size}", t("misc.metric_log", "Log")));
            composing_label.set_text(&match composing(&cache) {
                Some(true) => t("misc.buffer_composing", "đang soạn"),
                Some(false) => t("misc.buffer_empty", "trống"),
                None => String::new(),
            });
            if follow.is_active() {
                let adj = scroller.vadjustment();
                adj.set_value(adj.upper() - adj.page_size());
            }
        })
    };

    // --- Engine state in the status bar -----------------------------------
    let status_refresh: Rc<dyn Fn()> = {
        let dot = dot.clone();
        let state = state.clone();
        let detail = detail.clone();
        Rc::new(move || match engine_probe::probe() {
            Some(engine) => {
                set_dot(&dot, "diag-dot-ok");
                state.set_text(&t("misc.status_running", "Engine đang chạy"));
                let uptime = engine
                    .started
                    .and_then(|began| std::time::SystemTime::now().duration_since(began).ok())
                    .map(|age| uptime_label(age.as_secs()))
                    .unwrap_or_else(|| "?".to_string());
                detail.set_text(&format!(
                    "PID {} · {uptime} · {}",
                    engine.pid,
                    env!("CARGO_PKG_VERSION")
                ));
            }
            None => {
                set_dot(&dot, "diag-dot-warn");
                state.set_text(&t("misc.status_stopped", "Engine không chạy"));
                detail.set_text(&format!(
                    "{} · {}",
                    t(
                        "misc.status_stopped_hint",
                        "IBus chỉ khởi động engine khi có phím"
                    ),
                    env!("CARGO_PKG_VERSION")
                ));
            }
        })
    };
    status_refresh();

    // --- Wiring -----------------------------------------------------------
    {
        let render = Rc::clone(&render);
        let query = Rc::clone(&query);
        filter.connect_search_changed(move |entry| {
            *query.borrow_mut() = entry.text().to_string();
            render();
        });
    }
    {
        let render = Rc::clone(&render);
        let kind_filter = Rc::clone(&kind_filter);
        kind_pick.connect_selected_notify(move |pick| {
            // Item 0 is "Tất cả"; the rest follow `CHIPS`.
            let index = pick.selected();
            *kind_filter.borrow_mut() = usize::try_from(index)
                .ok()
                .and_then(|i| i.checked_sub(1))
                .and_then(|i| CHIPS.get(i).copied());
            render();
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
        // The pause button follows the state, not the click: scrolling up in
        // the view turns following off, and the button has to say so.
        let follow2 = follow.clone();
        let auto_scroll = Rc::new(Cell::new(false));
        let guard = Rc::clone(&auto_scroll);
        scroller.vadjustment().connect_value_changed(move |adj| {
            if guard.get() || !follow2.is_active() {
                return;
            }
            if adj.value() + adj.page_size() < adj.upper() - 4.0 {
                follow2.set_active(false);
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
    {
        // A narrow window stacks the column instead of squeezing both.
        let split = split.clone();
        let separator = separator.clone();
        let right = right.clone();
        let scroller = scroller.clone();
        let outer_ref = outer.clone();
        let laid_out = Rc::new(Cell::new((false, -1i32)));
        let state = Rc::clone(&laid_out);
        // A tick callback runs on every frame and keeps the window redrawing
        // for as long as the page is open; a plain timer costs a comparison.
        gtk4::glib::timeout_add_local(std::time::Duration::from_millis(200), move || {
            let width = outer_ref.width();
            let narrow = is_narrow(width);
            let share = column_share(width);
            if (narrow, share) != state.get() {
                state.set((narrow, share));
                if narrow {
                    split.set_orientation(Orientation::Vertical);
                    separator.set_visible(false);
                    right.set_width_request(-1);
                    right.set_vexpand(false);
                    // Stacked, the column takes its own height and the page
                    // scrolls, instead of squeezing it into a few pixels.
                    right.set_propagate_natural_height(true);
                    scroller.set_vexpand(false);
                } else {
                    split.set_orientation(Orientation::Horizontal);
                    separator.set_visible(true);
                    right.set_width_request(share);
                    right.set_propagate_natural_height(false);
                    right.set_vexpand(true);
                    scroller.set_vexpand(true);
                }
            }
            gtk4::glib::ControlFlow::Continue
        });
    }

    // --- Report and restart -----------------------------------------------
    {
        let ui_clone = ui.clone();
        let overlay = toast_overlay.clone();
        let path = path.clone();
        let hide_switch_copy = hide_switch.clone();
        let facts = Rc::clone(&facts);
        copy_btn.connect_clicked(move |btn| {
            let hide = hide_switch_copy.is_active();
            let last = report_log_lines(path.as_deref(), hide);
            let mut config = config_lines(&ui_clone);
            config.extend(macro_lines(hide));
            let text = crate::gui::debug_report::report(&facts(), &config, &last);
            btn.clipboard().set_text(&text);
            ui_clone.fire_toast(&overlay, &t("debug_report.copied", "Đã sao chép báo cáo"));
        });
    }
    {
        let ui_clone = ui.clone();
        let overlay = toast_overlay.clone();
        let status_refresh = Rc::clone(&status_refresh);
        let tools_refresh = Rc::clone(&tools.refresh);
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
                let ui = ui_clone.clone();
                let overlay = overlay.clone();
                let status_refresh = Rc::clone(&status_refresh);
                let tools_refresh = Rc::clone(&tools_refresh);
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
                    let ticks = Rc::new(Cell::new(0u32));
                    let ui = ui.clone();
                    let overlay = overlay.clone();
                    let status_refresh = Rc::clone(&status_refresh);
                    let tools_refresh = Rc::clone(&tools_refresh);
                    let btn = btn.clone();
                    gtk4::glib::timeout_add_local(
                        std::time::Duration::from_millis(500),
                        move || {
                            ticks.set(ticks.get() + 1);
                            let back = engine_probe::probe().is_some();
                            if back || ticks.get() >= 20 {
                                btn.set_sensitive(true);
                                btn.set_label(&t("misc.restart_button", "Khởi động lại IBus"));
                                status_refresh();
                                tools_refresh();
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

    // --- Following the file -----------------------------------------------
    {
        // One poll reads only what the engine appended. While following, the
        // view jumps to the new lines; while paused, the number waits in the
        // toolbar until the user asks for it.
        let poll: Rc<dyn Fn(bool)> = {
            let reader = Rc::clone(&reader);
            let parsed = Rc::clone(&parsed);
            let render = Rc::clone(&render);
            let status_refresh = Rc::clone(&status_refresh);
            let tools_refresh = Rc::clone(&tools.refresh);
            let follow = follow.clone();
            let arrived = arrived.clone();
            let pending = Rc::clone(&pending);
            let last_probe = Rc::new(Cell::new(std::time::Instant::now()));
            Rc::new(move |first: bool| {
                let added = reader.borrow_mut().poll();
                extend_tail(&mut parsed.borrow_mut(), &added, VIEW_LINES);
                if first {
                    render();
                    status_refresh();
                    tools_refresh();
                    return;
                }
                if added.is_empty() {
                    return;
                }
                // Typing in the test box is exactly what starts the engine, so
                // a status that said "not running" has to be re-read. Twice a
                // second at most: the probe walks /proc.
                if last_probe.get().elapsed() > std::time::Duration::from_millis(500) {
                    last_probe.set(std::time::Instant::now());
                    status_refresh();
                    tools_refresh();
                }
                if follow.is_active() {
                    render();
                    pending.set(0);
                    arrived.set_visible(false);
                } else {
                    pending.set(pending.get() + added.len());
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

    // One draw before the window appears, so the status bar is never blank.
    render();
    toast_overlay.set_child(Some(&outer));
    toast_overlay
}

/// Below `NARROW_WIDTH` the right column moves under the log.
fn is_narrow(width: i32) -> bool {
    width > 0 && width < NARROW_WIDTH
}

/// The right column's width: a fixed share of the page, kept in a band where
/// the rows still read and the log keeps most of the room.
fn column_share(width: i32) -> i32 {
    ((width as f32 / SHARE_RATIO) as i32).clamp(MIN_RIGHT_WIDTH, MAX_RIGHT_WIDTH)
}

/// "2 giờ 14 phút": an uptime short enough for a status bar.
fn uptime_label(secs: u64) -> String {
    let minutes = secs / 60;
    let hours = minutes / 60;
    let days = hours / 24;
    if days > 0 {
        format!("{days} ngày {}", hours % 24)
    } else if hours > 0 {
        format!("{hours} giờ {}", minutes % 60)
    } else if minutes > 0 {
        format!("{minutes} phút")
    } else {
        format!("{secs} giây")
    }
}

fn kind_label(kind: Kind) -> String {
    match kind {
        Kind::Key => t("misc.chip_key", "Phím"),
        Kind::Insert => t("misc.chip_insert", "Chèn"),
        Kind::Flush => t("misc.chip_flush", "Flush"),
        Kind::Error => t("misc.chip_error", "Lỗi"),
        Kind::Other => t("misc.chip_other", "Khác"),
    }
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

/// The macro table as the report carries it.
fn macro_lines(hide: bool) -> Vec<String> {
    let entries: Vec<(String, String)> = crate::macros::Macros::load()
        .entries()
        .map(|(key, value)| (key.to_string(), value.to_string()))
        .collect();
    macro_block(hide, entries)
}

/// Hiding keeps the count and drops every key and expansion: a shortcut can
/// hold a password.
fn macro_block(hide: bool, entries: Vec<(String, String)>) -> Vec<String> {
    if entries.is_empty() {
        return Vec::new();
    }
    if hide {
        return vec![format!(
            "[macro] {} {}",
            entries.len(),
            t("debug_report.macro_hidden", "mục, đã ẩn nội dung")
        )];
    }
    let mut out = vec![format!("[macro] {}", entries.len())];
    for (key, value) in entries {
        out.push(format!("{key} = {value:?}"));
    }
    out
}

/// Decode what the engine appended and keep the tail at `cap` lines. Line
/// times stay relative to the line before them, so the gap is taken from the
/// cache rather than from the batch.
fn extend_tail(cache: &mut Vec<Line>, fresh: &[String], cap: usize) {
    let mut prev = cache.last().and_then(|line| line.abs_ms);
    for raw in fresh {
        let Some(line) = parse(raw, prev) else {
            continue;
        };
        prev = line.abs_ms.or(prev);
        cache.push(line);
    }
    let over = cache.len().saturating_sub(cap);
    if over > 0 {
        cache.drain(..over);
    }
}

/// Draw every selected line from scratch: a new filter, a new kind, a cleared
/// log.
fn draw_all(view: &gtk4::TextView, lines: &[&Line]) {
    let buffer = view.buffer();
    buffer.set_text("");
    let mut iter = buffer.start_iter();
    write(&buffer, &mut iter, lines);
}

/// Append the lines that arrived since the last draw and drop the oldest ones
/// the cap no longer keeps.
fn draw_more(view: &gtk4::TextView, lines: &[&Line]) {
    if lines.is_empty() {
        return;
    }
    let buffer = view.buffer();
    let mut iter = buffer.end_iter();
    write(&buffer, &mut iter, lines);
    let over = buffer.line_count() - VIEW_LINES as i32;
    if over > 0 {
        let mut start = buffer.start_iter();
        let mut end = buffer.start_iter();
        end.forward_lines(over);
        buffer.delete(&mut start, &mut end);
    }
}

/// One line: relative time, coloured kind, decoded body.
fn write(buffer: &gtk4::TextBuffer, iter: &mut gtk4::TextIter, lines: &[&Line]) {
    let press = t("misc.key_press", "nhấn");
    let release = t("misc.key_release", "thả");
    let width = CHIPS
        .iter()
        .map(|k| kind_label(*k).len())
        .max()
        .unwrap_or(4);
    for line in lines {
        let when = match line.rel_ms {
            Some(ms) => format!("+{ms}ms"),
            None => String::new(),
        };
        buffer.insert_with_tags_by_name(iter, &format!("{when:>8} "), &["log-time"]);
        let label = kind_label(line.kind);
        buffer.insert_with_tags_by_name(
            iter,
            &format!("{label:<width$} "),
            &[tag_for(line.kind), "log-label"],
        );
        buffer.insert(iter, &line.body.text(&press, &release));
        buffer.insert(iter, "\n");
    }
}

fn tag_for(kind: Kind) -> &'static str {
    match kind {
        Kind::Key => "log-key",
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

/// The status dot takes exactly one of the three colours.
fn set_dot(dot: &gtk4::Label, class: &str) {
    for old in ["diag-dot-ok", "diag-dot-warn", "diag-dot-info"] {
        dot.remove_css_class(old);
    }
    dot.add_css_class(class);
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
    fn every_kind_the_list_offers_has_a_label() {
        for kind in CHIPS {
            assert!(!kind_label(kind).is_empty(), "{kind:?}");
        }
        assert_eq!(kind_label(Kind::Key), "Phím");
        assert_eq!(kind_label(Kind::Error), "Lỗi");
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
        let mut parsed = Vec::new();
        extend_tail(&mut parsed, &raw(), VIEW_LINES);
        assert_eq!(parsed.len(), 6);
        assert_eq!(parsed[1].kind, Kind::Key);
        assert_eq!(parsed[4].kind, Kind::Error);
        assert_eq!(parsed[3].kind, Kind::Insert);
    }

    #[test]
    fn the_cache_grows_by_what_arrived_and_keeps_its_cap() {
        let mut cache = Vec::new();
        extend_tail(&mut cache, &raw()[..3], VIEW_LINES);
        assert_eq!(cache.len(), 3);
        // The next batch keeps the gap relative to the last line it saw.
        extend_tail(&mut cache, &raw()[3..], VIEW_LINES);
        assert_eq!(cache.len(), 6);
        assert_eq!(cache[3].rel_ms, Some(4), "4 ms after the line before it");

        let mut small = Vec::new();
        extend_tail(&mut small, &raw(), 2);
        assert_eq!(small.len(), 2, "the cap drops the oldest lines");
        assert_eq!(
            small[1].body.text("nhấn", "thả"),
            "mode: preedit",
            "the newest line survives"
        );

        let mut junk = Vec::new();
        extend_tail(
            &mut junk,
            &["not a log line at all".to_string()],
            VIEW_LINES,
        );
        assert_eq!(junk.len(), 1, "a line without a timestamp still shows");
        assert_eq!(junk[0].rel_ms, None, "it just has no gap to report");
    }

    #[test]
    fn the_two_columns_split_one_and_a_half_to_one() {
        assert_eq!(column_share(0), MIN_RIGHT_WIDTH);
        assert_eq!(column_share(760), 304);
        assert_eq!(column_share(1334), 533);
        assert_eq!(
            column_share(3000),
            MAX_RIGHT_WIDTH,
            "the log keeps the room"
        );
    }

    #[test]
    fn a_narrow_window_stacks_the_columns() {
        assert!(is_narrow(699));
        assert!(
            !is_narrow(NARROW_WIDTH),
            "the threshold itself still splits"
        );
        assert!(!is_narrow(0), "an unallocated window is not narrow");
    }

    #[test]
    fn an_uptime_reads_the_way_a_person_says_it() {
        assert_eq!(uptime_label(5), "5 giây");
        assert_eq!(uptime_label(90), "1 phút");
        assert_eq!(uptime_label(3600 + 14 * 60), "1 giờ 14");
        assert_eq!(uptime_label(86400 * 2 + 3600 * 3), "2 ngày 3");
    }
}
