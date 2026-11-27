//! Page "Gỡ lỗi": one screen of tools that only exist while debug mode is on.
//! A status card on top, the log card on the left, the typing test and the
//! facts on the right, and an action bar across the bottom that restarts IBus
//! or copies a report. Below `NARROW_WIDTH` the right column drops under the
//! log.
//!
//! The switch that turns the log on stays in "Cài đặt nâng cao": this page is
//! the consequence of that switch, not a second place to set it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use super::*;
use crate::gui::debug_report::{backend_from_log, backend_why_from_log, sections, Facts, Section};
use crate::gui::engine_probe;
use crate::gui::latency::{self, SPARK};
use crate::gui::log_decode::{composing, mask_private, parse, Kind, Line};
use crate::gui::log_tail::{size_label, tail, Reader, TAIL_BYTES};

/// Lines the viewer keeps. Beyond this the file is history, and holding more
/// only makes the page heavy.
const VIEW_LINES: usize = 5_000;

/// The lines the copied report carries.
const REPORT_LINES: usize = 100;

/// A wide log is worth more than a wide window: past this the page stops
/// growing and centres itself, like every other tab.
const MAX_PAGE_WIDTH: i32 = 1040;

/// Under this width the two columns cannot both be read, so the right one
/// moves below the log.
const NARROW_WIDTH: i32 = 720;

/// The log kinds the filter bar offers, in the order it draws them: "Tất cả"
/// and then every kind the decoder names, so no line is out of reach. A line
/// that fits none of them shows only under "Tất cả".
const FILTERS: [Option<Kind>; 6] = [
    None,
    Some(Kind::Key),
    Some(Kind::Insert),
    Some(Kind::Delete),
    Some(Kind::Flush),
    Some(Kind::Error),
];

/// How much of the height the page has left the log pane takes on a wide
/// window. The rest is left open, so the pane reads as a window onto the log
/// rather than as a wall of text.
const LOG_SHARE: f64 = 0.8;

/// The log pane's height when the column is stacked: the page scrolls there, and
/// a pane sized to its content would be a few lines tall.
const STACKED_LOG_HEIGHT: i32 = 320;

/// Vertical gaps inside the log column, measured in spacing and margins rather
/// than guessed wide.
const LOG_COLUMN_GAPS: i32 = 24;

/// Tallest bar of the header's sparkline, in pixels.
const SPARK_HEIGHT: i32 = 18;

/// The facts that read another process, kept together so one worker thread
/// updates them all.
#[derive(Clone)]
struct Slow {
    engine: Option<engine_probe::EngineStatus>,
    extension: ExtensionState,
    machine: crate::gui::machine::Machine,
}

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

    // The log reader (it only reads what was appended), the decoded lines, and
    // which filter they were drawn under.
    let reader = Rc::new(RefCell::new(Reader::new(path.clone(), VIEW_LINES)));
    // The decoded log, kept between polls: re-decoding five thousand lines a
    // second cost about 9 ms, and redrawing all of them about 105 ms.
    let parsed: Rc<RefCell<Vec<Line>>> = Rc::new(RefCell::new(Vec::new()));
    // How many of the shown lines are already in the buffer, and under which
    // filter they were drawn.
    let drawn = Rc::new(Cell::new(0usize));
    let drawn_key: Rc<RefCell<(Option<Kind>, String)>> =
        Rc::new(RefCell::new((None, String::new())));
    let kind_filter: Rc<RefCell<Option<Kind>>> = Rc::new(RefCell::new(None));
    let query: Rc<RefCell<String>> = Rc::new(RefCell::new(String::new()));

    // --- Status card ------------------------------------------------------
    let head_tile = gtk4::Image::from_icon_name("utilities-system-monitor-symbolic");
    head_tile.add_css_class("diag-head-icon");
    let tile_holder = gtk4::Box::new(Orientation::Horizontal, 0);
    tile_holder.set_valign(gtk4::Align::Center);
    tile_holder.append(&head_tile);
    let state = gtk4::Label::new(None);
    state.add_css_class("heading");
    state.set_xalign(0.0);
    let detail = gtk4::Label::new(None);
    detail.add_css_class("caption");
    detail.add_css_class("dim-label");
    detail.set_xalign(0.0);
    detail.set_ellipsize(gtk4::pango::EllipsizeMode::End);
    let state_text = gtk4::Box::new(Orientation::Vertical, 1);
    state_text.set_valign(gtk4::Align::Center);
    state_text.set_hexpand(true);
    state_text.append(&state);
    state_text.append(&detail);

    let latency_caption = gtk4::Label::new(Some(&t("misc.metric_pair", "Trễ TB / tối đa")));
    latency_caption.add_css_class("caption");
    latency_caption.add_css_class("dim-label");
    latency_caption.set_xalign(1.0);
    let latency_value = gtk4::Label::new(None);
    latency_value.add_css_class("monospace");
    latency_value.add_css_class("diag-value");
    latency_value.set_xalign(1.0);
    let latency_text = gtk4::Box::new(Orientation::Vertical, 1);
    latency_text.append(&latency_caption);
    latency_text.append(&latency_value);
    // Eight bars, oldest first: only the engine's own log says how long a
    // keystroke took, so this is a picture of the same numbers beside it.
    let spark = gtk4::Box::new(Orientation::Horizontal, 2);
    spark.set_valign(gtk4::Align::End);
    let mut bars = Vec::with_capacity(SPARK);
    for _ in 0..SPARK {
        let bar = gtk4::Box::new(Orientation::Vertical, 0);
        bar.add_css_class("diag-bar");
        bar.set_valign(gtk4::Align::End);
        bar.set_size_request(4, 2);
        spark.append(&bar);
        bars.push(bar);
    }
    spark.set_visible(false);
    let latency_box = gtk4::Box::new(Orientation::Horizontal, 10);
    latency_box.set_valign(gtk4::Align::Center);
    latency_box.append(&latency_text);
    latency_box.append(&spark);

    let head = gtk4::Box::new(Orientation::Horizontal, 12);
    head.add_css_class("card");
    head.add_css_class("diag-card");
    head.append(&tile_holder);
    head.append(&state_text);
    head.append(&latency_box);

    // --- The log card -----------------------------------------------------
    let filter_entry = gtk4::SearchEntry::new();
    filter_entry.set_placeholder_text(Some(&t("misc.debug_filter_placeholder", "Lọc dòng")));
    filter_entry.set_hexpand(true);
    filter_entry.set_tooltip_text(Some(&t(
        "tips.debug_filter",
        "Gõ một phần nội dung để lọc, không phân biệt hoa thường.",
    )));
    let clear_btn = icon_button("user-trash-symbolic", &t("misc.debug_clear_log", "Xoá log"));
    // Follows the newest line, and stops when the reader wants the old ones:
    // reading a stack while the pane jumps is what this switch is for.
    let follow = gtk4::ToggleButton::new();
    follow.set_icon_name("media-playback-pause-symbolic");
    follow.set_active(true);
    follow.add_css_class("flat");
    set_button_label(
        &follow,
        &t("misc.debug_follow", "Dừng cuộn"),
        &t("misc.debug_follow", "Dừng cuộn"),
    );
    let toolbar = gtk4::Box::new(Orientation::Horizontal, 8);
    toolbar.set_margin_top(10);
    toolbar.set_margin_start(10);
    toolbar.set_margin_end(10);
    toolbar.set_margin_bottom(8);
    toolbar.append(&filter_entry);
    toolbar.append(&follow);
    toolbar.append(&clear_btn);

    // One linked group of toggles instead of a dropdown: the kinds are few, and
    // a dropdown hid both the choices and the kind in force.
    let segmented = gtk4::Box::new(Orientation::Horizontal, 0);
    segmented.add_css_class("linked");
    segmented.add_css_class("diag-filters");
    segmented.set_valign(gtk4::Align::Center);
    let mut buttons: Vec<(Option<Kind>, gtk4::ToggleButton)> = Vec::with_capacity(FILTERS.len());
    for kind in FILTERS {
        let text = match kind {
            Some(kind) => kind_label(kind),
            None => t("misc.chip_all", "Tất cả"),
        };
        let btn = gtk4::ToggleButton::with_label(&text);
        btn.set_tooltip_text(Some(&t(
            "tips.debug_kind",
            "Chỉ hiện một loại dòng. Dòng không thuộc loại nào chỉ hiện khi chọn Tất cả.",
        )));
        if let Some((_, first)) = buttons.first() {
            btn.set_group(Some(first));
        }
        segmented.append(&btn);
        buttons.push((kind, btn));
    }
    if let Some((_, first)) = buttons.first() {
        first.set_active(true);
    }

    // Stops the ENGINE writing, not the pane: whoever is chasing a bug keeps
    // the lines already collected, and the file stops growing (or can be
    // cleared without the next keystroke filling it again).
    let log_pause = gtk4::Switch::new();
    log_pause.set_active(crate::config::Config::load().debug_log_paused);
    log_pause.set_valign(gtk4::Align::Center);
    log_pause.set_tooltip_text(Some(&t(
        "tips.debug_log_pause",
        "Tạm dừng việc ghi log, engine không thêm dòng nào cho tới khi bạn bật lại.",
    )));
    log_pause.update_property(&[gtk4::accessible::Property::Label(&t(
        "misc.debug_log_pause",
        "Tạm dừng ghi log",
    ))]);
    let pause_caption = gtk4::Label::new(Some(&t("misc.debug_pause_short", "Tạm dừng")));
    pause_caption.add_css_class("caption");
    pause_caption.add_css_class("dim-label");
    let pause_group = gtk4::Box::new(Orientation::Horizontal, 6);
    pause_group.append(&pause_caption);
    pause_group.append(&log_pause);
    // Two short rows, always: side by side the kind bar and the switch decided
    // the whole page's minimum width, and the page could never be narrowed.
    let filter_row = gtk4::Box::new(Orientation::Vertical, 6);
    filter_row.set_margin_start(10);
    filter_row.set_margin_end(10);
    filter_row.set_margin_bottom(8);
    pause_group.set_halign(gtk4::Align::End);
    filter_row.append(&segmented);
    filter_row.append(&pause_group);

    let view = gtk4::TextView::new();
    view.set_editable(false);
    view.set_cursor_visible(false);
    view.set_monospace(true);
    // Wrapped, not clipped: a line cut mid-word at the pane's edge reads as a
    // broken window, and that value is what the reader opened the page for.
    view.set_wrap_mode(gtk4::WrapMode::WordChar);
    view.add_css_class("log-view");
    view.set_left_margin(6);
    view.set_right_margin(6);
    view.set_top_margin(2);
    view.set_bottom_margin(2);
    make_tags(&view.buffer(), &view);
    let scroller = gtk4::ScrolledWindow::builder()
        .min_content_height(110)
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vexpand(true)
        .child(&view)
        .build();
    scroller.set_margin_bottom(8);
    scroller.set_margin_start(4);
    scroller.set_margin_end(4);

    // Nothing to show is a state of its own: a blank pane reads as a broken
    // page, and a paused one has to say so.
    let empty = adw::StatusPage::builder()
        .icon_name("utilities-terminal-symbolic")
        .title(t("misc.debug_empty_title", "Chưa có log"))
        .build();
    empty.set_vexpand(true);
    let stack = gtk4::Stack::new();
    stack.set_vexpand(true);
    stack.add_named(&scroller, Some("log"));
    stack.add_named(&empty, Some("empty"));

    // The whole log — search, filter, lines — sits in one frame, and that frame
    // is the rounded pane the page used before it was rebuilt, not an Adwaita
    // card: a card here read as a settings card with a terminal dropped in it.
    let log_column = gtk4::Box::new(Orientation::Vertical, 0);
    log_column.add_css_class("log-pane");
    log_column.add_css_class("diag-log");
    log_column.set_hexpand(true);
    log_column.set_vexpand(true);
    log_column.append(&toolbar);
    log_column.append(&filter_row);
    log_column.append(&stack);

    // --- The facts the rows and the report read ---------------------------
    // Two of them come from other processes (`ibus engine`, `gsettings get`)
    // and take tens of milliseconds each: they are read on a worker thread, so
    // none of that ever blocks a keystroke. The rows read the last answer.
    let slow = Arc::new(Mutex::new(Slow {
        engine: engine_probe::probe(),
        extension: extension_state(),
        machine: crate::gui::machine::collect(),
    }));
    let busy = Arc::new(AtomicBool::new(false));
    let read_slow: Rc<dyn Fn() -> Slow> = {
        let slow = Arc::clone(&slow);
        Rc::new(move || {
            slow.lock()
                .map(|slot| slot.clone())
                .unwrap_or_else(|poisoned| poisoned.into_inner().clone())
        })
    };
    let refresh_slow: Rc<dyn Fn()> = {
        let slow = Arc::clone(&slow);
        let busy = Arc::clone(&busy);
        Rc::new(move || {
            if busy.swap(true, Ordering::Relaxed) {
                return;
            }
            let slot = Arc::clone(&slow);
            let busy = Arc::clone(&busy);
            std::thread::spawn(move || {
                let fresh = Slow {
                    engine: engine_probe::probe(),
                    extension: extension_state(),
                    // Read once per refresh: what the machine is does not
                    // change while the page is open.
                    machine: match slot.lock() {
                        Ok(held) => held.machine.clone(),
                        Err(poisoned) => poisoned.into_inner().machine.clone(),
                    },
                };
                if let Ok(mut held) = slot.lock() {
                    *held = fresh;
                }
                busy.store(false, Ordering::Relaxed);
            });
        })
    };

    let facts: Rc<dyn Fn() -> Vec<Section>> = {
        let ui = ui.clone();
        let path = path.clone();
        let reader = Rc::clone(&reader);
        let read_slow = Rc::clone(&read_slow);
        Rc::new(move || {
            let config = ui.read();
            let Slow {
                engine,
                extension,
                machine,
            } = read_slow();
            let (key, fallback) = extension_row_for(extension).subtitle;
            let log_lines = reader.borrow().recent(200);
            let backend = backend_from_log(&log_lines);
            let backend_why = backend_why_from_log(&log_lines);
            sections(&Facts {
                preferred: config.preferred_backend,
                mode: config.mode,
                extension: (key, fallback, extension == ExtensionState::Running),
                config_path: Config::path().as_deref(),
                log_path: path.as_deref(),
                engine: engine.as_ref(),
                backend: backend.as_deref(),
                backend_why: backend_why.as_deref(),
                machine: &machine,
            })
        })
    };

    // --- Right column -----------------------------------------------------
    let tools = super::debug_tools::build(ui, &toast_overlay, Rc::clone(&facts));
    // The typing test sits under the log it produces: what you type in it is the
    // next line in the pane above.
    log_column.append(&tools.try_box);
    let side = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .child(&tools.body)
        .build();
    // Without this the column's natural width is its longest label, and the box
    // would hand it half the page.
    side.set_propagate_natural_width(false);
    // The facts take whatever height the actions leave: they are the part that
    // may need to scroll.
    side.set_vexpand(true);

    let body = gtk4::Box::new(Orientation::Horizontal, 14);
    body.add_css_class("diag-body");
    body.append(&log_column);

    // --- Actions ------------------------------------------------------------
    let restart_btn =
        super::widgets::action_button(None, &t("misc.restart_button", "Khởi động lại IBus"));
    restart_btn.set_tooltip_text(Some(&t(
        "tips.restart_engine",
        "Khởi động lại IBus, bộ gõ sẽ ngắt trong giây lát ở mọi ứng dụng.",
    )));
    let copy_btn = super::widgets::action_button(
        Some("edit-copy-symbolic"),
        &t("misc.debug_copy_report", "Sao chép báo cáo"),
    );
    copy_btn.add_css_class("suggested-action");
    for btn in [&restart_btn, &copy_btn] {
        btn.set_size_request(-1, 30);
    }
    // Remembered like the pause switch: the window opens the way it was left,
    // and the default stays on.
    let hide_switch = gtk4::Switch::new();
    hide_switch.set_active(crate::config::Config::load().debug_hide_text);
    hide_switch.set_valign(gtk4::Align::Center);
    hide_switch.set_tooltip_text(Some(&t(
        "tips.debug_hide",
        "Bật thì báo cáo thay chữ đã gõ bằng *** và macro chỉ còn số lượng; tắt thì báo cáo mang cả hai.",
    )));
    hide_switch.update_property(&[gtk4::accessible::Property::Label(&t(
        "misc.debug_hide_text",
        "Ẩn nội dung gõ và macro khi sao chép",
    ))]);
    let hide_label = gtk4::Label::new(Some(&t(
        "misc.debug_hide_text",
        "Ẩn nội dung gõ và macro khi sao chép",
    )));
    hide_label.set_xalign(0.0);
    hide_label.set_hexpand(true);
    hide_label.add_css_class("caption");
    // Wrapped: on one line this label decides the column's width, and the log
    // pays for it.
    hide_label.set_wrap(true);
    // A wrapped label still asks for its whole text in its natural width, which
    // would widen the column past the log's share.
    hide_label.set_max_width_chars(22);
    let hide_group = gtk4::Box::new(Orientation::Horizontal, 10);
    hide_group.set_valign(gtk4::Align::Center);
    hide_group.set_hexpand(true);
    hide_group.append(&hide_label);
    hide_group.append(&hide_switch);
    // One row each, full width, in a card of their own at the foot of the facts:
    // three controls of different kinds stacked read faster than two rows of
    // buttons of different widths.
    for btn in [&restart_btn, &copy_btn] {
        btn.set_hexpand(true);
    }
    let actions_card = gtk4::Box::new(Orientation::Vertical, 6);
    actions_card.add_css_class("card");
    actions_card.add_css_class("diag-card");
    actions_card.append(&hide_group);
    actions_card.append(&restart_btn);
    actions_card.append(&copy_btn);
    // The facts scroll if they must; the actions never do. A button half under
    // the window's edge is worse than three facts one scroll away.
    let side_column = gtk4::Box::new(Orientation::Vertical, 10);
    side_column.set_width_request(super::debug_tools::SIDE_WIDTH);
    side_column.set_hexpand(false);
    // Not expanding: the column is exactly as tall as the page's tallest column,
    // so its actions bottom ends level with the typing test instead of dropping
    // to the window's edge.
    side_column.set_vexpand(false);
    side_column.append(&side);
    side_column.append(&actions_card);
    body.append(&side_column);

    // --- The page ---------------------------------------------------------
    let page = gtk4::Box::new(Orientation::Vertical, 10);
    page.add_css_class("diag-page");
    // The same breathing room every other tab gets: their preferences page
    // centres a column and keeps it off the window edges, and a page that runs
    // edge to edge reads as a different application.
    page.set_margin_top(14);
    page.set_margin_bottom(14);
    page.set_margin_start(24);
    page.set_margin_end(24);
    // Top-aligned and as tall as its content: the height the log gave up stays
    // below the whole page, never between the log and the typing test.
    page.set_valign(gtk4::Align::Start);
    page.append(&head);
    page.append(&body);

    // The page never scrolls while the two columns fit; the stacked column
    // below `NARROW_WIDTH` is taller than the window, and this keeps it
    // reachable.
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(MAX_PAGE_WIDTH);
    clamp.set_tightening_threshold(MAX_PAGE_WIDTH);
    clamp.set_child(Some(&page));
    let outer = gtk4::ScrolledWindow::builder()
        .hscrollbar_policy(gtk4::PolicyType::Never)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .child(&clamp)
        .vexpand(true)
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
        let stack = stack.clone();
        let empty = empty.clone();
        let log_pause = log_pause.clone();
        let latency_value = latency_value.clone();
        let spark = spark.clone();
        let bars = bars.clone();
        let composing_label = tools.composing.clone();
        let clear_btn = clear_btn.clone();
        let follow = follow.clone();
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
            // An empty log is a state, not a blank card.
            if shown.is_empty() {
                if log_pause.is_active() {
                    empty.set_title(&t("misc.debug_paused_title", "Đã tạm dừng"));
                    empty.set_description(Some(&t(
                        "misc.debug_paused_body",
                        "Bật lại công tắc để engine ghi tiếp.",
                    )));
                } else {
                    empty.set_title(&t("misc.debug_empty_title", "Chưa có log"));
                    empty.set_description(Some(&t(
                        "misc.debug_empty_body",
                        "Gõ vào ô Thử gõ bên phải để engine ghi dòng đầu tiên.",
                    )));
                }
                stack.set_visible_child_name("empty");
            } else {
                stack.set_visible_child_name("log");
                if follow.is_active() {
                    let adj = scroller.vadjustment();
                    adj.set_value(adj.upper() - adj.page_size());
                }
            }
            // The two latency numbers and the bars behind them come from the
            // same walk over the log.
            let stats = latency::measure(&cache);
            let pair = match &stats {
                Some(stats) => format!("{} ms / {} ms", stats.avg_ms, stats.max_ms),
                None => "—".to_string(),
            };
            set_if_changed(&latency_value, &pair);
            let recent = stats.map(|stats| stats.recent).unwrap_or_default();
            let top = recent.iter().copied().max().unwrap_or(1).max(1);
            for (index, bar) in bars.iter().enumerate() {
                match recent.get(index) {
                    Some(ms) => {
                        let height = ((ms * SPARK_HEIGHT as u64) / top).max(2) as i32;
                        bar.set_size_request(4, height.min(SPARK_HEIGHT));
                        bar.set_visible(true);
                    }
                    None => bar.set_visible(false),
                }
            }
            spark.set_visible(!recent.is_empty());
            let composing_text = match composing(&cache) {
                Some(true) => t("misc.buffer_composing", "đang soạn"),
                Some(false) => t("misc.buffer_empty", "rỗng"),
                None => String::new(),
            };
            set_if_changed(&composing_label, &composing_text);
            // How big the file has grown: the one number the card has no room
            // for, on the button that deletes it.
            let size = path
                .as_deref()
                .and_then(|p| std::fs::metadata(p).ok())
                .map(|meta| size_label(meta.len()))
                .unwrap_or_else(|| "—".to_string());
            clear_btn.set_tooltip_text(Some(&format!(
                "{} ({size})",
                t("misc.debug_clear_log", "Xoá log")
            )));
        })
    };

    // --- Engine state in the status card ----------------------------------
    let status_refresh: Rc<dyn Fn()> = {
        let head_tile = head_tile.clone();
        let state = state.clone();
        let detail = detail.clone();
        let read_slow = Rc::clone(&read_slow);
        Rc::new(move || match read_slow().engine {
            Some(engine) => {
                set_head_state(&head_tile, true);
                set_if_changed(&state, &t("misc.status_running", "Engine đang chạy"));
                let uptime = engine
                    .started
                    .and_then(|began| std::time::SystemTime::now().duration_since(began).ok())
                    .map(|age| uptime_label(age.as_secs()))
                    .unwrap_or_else(|| "?".to_string());
                // With the version row gone from the column, the sentence that
                // tells the user to restart IBus lives here.
                let stale = if engine.stale {
                    format!(
                        " · {}",
                        t("misc.status_stale", "bản cài mới hơn, khởi động lại IBus")
                    )
                } else {
                    String::new()
                };
                set_if_changed(
                    &detail,
                    &format!(
                        "PID {} · {uptime} · {}{stale}",
                        engine.pid,
                        env!("CARGO_PKG_VERSION")
                    ),
                );
            }
            None => {
                set_head_state(&head_tile, false);
                set_if_changed(&state, &t("misc.status_stopped", "Engine không chạy"));
                set_if_changed(
                    &detail,
                    &format!(
                        "{} · {}",
                        t(
                            "misc.status_stopped_hint",
                            "IBus chỉ khởi động engine khi có phím"
                        ),
                        env!("CARGO_PKG_VERSION")
                    ),
                );
            }
        })
    };
    status_refresh();

    // --- Wiring -----------------------------------------------------------
    {
        // The icon is the only thing that says whether the pane is following:
        // a flat toggle alone changes no colour.
        follow.connect_toggled(|btn| {
            btn.set_icon_name(if btn.is_active() {
                "media-playback-pause-symbolic"
            } else {
                "media-playback-start-symbolic"
            });
        });
    }
    {
        let render = Rc::clone(&render);
        let query = Rc::clone(&query);
        filter_entry.connect_search_changed(move |entry| {
            *query.borrow_mut() = entry.text().to_string();
            render();
        });
    }
    for (kind, btn) in &buttons {
        let render = Rc::clone(&render);
        let kind_filter = Rc::clone(&kind_filter);
        let kind = *kind;
        btn.connect_toggled(move |btn| {
            // A radio group fires on the button that loses too: only the one
            // that turned on says which kind is in force.
            if !btn.is_active() {
                return;
            }
            *kind_filter.borrow_mut() = kind;
            render();
        });
    }
    {
        let render = Rc::clone(&render);
        log_pause.connect_active_notify(move |sw| {
            let mut config = crate::config::Config::load();
            config.debug_log_paused = sw.is_active();
            let _ = config.save();
            render();
        });
    }
    {
        // Re-pin when the text view grows: at the moment render() appends, the
        // adjustment still reports the old height, so one scroll inside
        // render() lands above the newest line and the log looks frozen. The
        // scroll switch has the last word: a reader who stopped the pane keeps
        // the lines they were reading.
        let adj = scroller.vadjustment();
        let follow = follow.clone();
        adj.connect_changed(move |adj| {
            if follow.is_active() {
                adj.set_value(adj.upper() - adj.page_size());
            }
        });
    }
    {
        let render = Rc::clone(&render);
        hide_switch.connect_active_notify(move |sw| {
            let mut config = crate::config::Config::load();
            config.debug_hide_text = sw.is_active();
            let _ = config.save();
            render();
        });
    }
    {
        let path = path.clone();
        let render = Rc::clone(&render);
        let reader_clear = Rc::clone(&reader);
        let parsed_clear = Rc::clone(&parsed);
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
                let parsed = Rc::clone(&parsed_clear);
                dialog.connect_response(Some("delete"), move |_, _| {
                    clear_log(&path, &reader, &parsed, &render);
                });
            }
            dialog.present(btn.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }
    {
        // A narrow window stacks the column instead of squeezing both, and the
        // log keeps four fifths of the height the page has left: on a maximised
        // window a pane that fills every pixel buries the typing test under the
        // fold and reads as an unbroken wall.
        let body = body.clone();
        let side = side.clone();
        let side_column = side_column.clone();
        let log_column = log_column.clone();
        let stack_ref = stack.clone();
        let toolbar_ref = toolbar.clone();

        let try_box_ref = tools.try_box.clone();
        let filter_row_ref = filter_row.clone();
        let page_ref = page.clone();
        // Measured against the window, never against the page: the page grows
        // with the pane, so asking the page would feed the pane its own height.
        let viewport = outer.clone();
        let head_ref = head.clone();
        let laid_out = Rc::new(Cell::new(false));
        let state = Rc::clone(&laid_out);
        // A tick callback runs on every frame and keeps the window redrawing
        // for as long as the page is open; a plain timer costs a comparison and
        // is quick enough that the columns land where they belong as the window
        // opens.
        gtk4::glib::timeout_add_local(std::time::Duration::from_millis(60), move || {
            let narrow = is_narrow(page_ref.width());
            if narrow != state.get() {
                state.set(narrow);
                if narrow {
                    body.set_orientation(Orientation::Vertical);
                    side_column.set_width_request(-1);
                    side_column.set_vexpand(false);
                    side.set_vexpand(false);
                    // Stacked, the column takes its own height and the page
                    // scrolls, instead of squeezing it into a few pixels.
                    side.set_propagate_natural_height(true);
                    log_column.set_vexpand(false);
                    // Stacked, the log takes a fixed slice and the page scrolls.
                    stack_ref.set_vexpand(false);
                    stack_ref.set_height_request(STACKED_LOG_HEIGHT);
                } else {
                    body.set_orientation(Orientation::Horizontal);
                    side_column.set_width_request(super::debug_tools::SIDE_WIDTH);
                    side_column.set_vexpand(false);
                    side.set_propagate_natural_height(false);
                    side.set_vexpand(true);
                    log_column.set_vexpand(false);
                    stack_ref.set_vexpand(false);
                }
            }
            if !narrow {
                let used = toolbar_ref.height()
                    + filter_row_ref.height()
                    + try_box_ref.height()
                    + LOG_COLUMN_GAPS;
                let room = viewport.height().max(page_ref.height())
                    - page_ref.margin_top()
                    - page_ref.margin_bottom()
                    - head_ref.height()
                    - LOG_COLUMN_GAPS;
                let wanted = (((room - used).max(80) as f64) * LOG_SHARE) as i32;
                if (stack_ref.height_request() - wanted).abs() > 6 {
                    stack_ref.set_height_request(wanted);
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
        // One poll reads only what the engine appended, and the view keeps the
        // newest line in sight.
        let poll: Rc<dyn Fn(bool)> = {
            let reader = Rc::clone(&reader);
            let parsed = Rc::clone(&parsed);
            let render = Rc::clone(&render);
            let status_refresh = Rc::clone(&status_refresh);
            let tools_refresh = Rc::clone(&tools.refresh);
            let last_probe = Rc::new(Cell::new(std::time::Instant::now()));
            let refresh_slow = Rc::clone(&refresh_slow);
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
                render();
                // Typing in the test box is exactly what starts the engine, so
                // a status that said "not running" has to be re-read. Twice a
                // second at most: the probe walks /proc.
                if last_probe.get().elapsed() > std::time::Duration::from_millis(900) {
                    last_probe.set(std::time::Instant::now());
                    // The subprocesses run on the worker; this only applies
                    // what it last answered, and the cards rebuild only if a
                    // value changed.
                    refresh_slow();
                    status_refresh();
                    tools_refresh();
                }
            })
        };
        {
            let poll = Rc::clone(&poll);
            page.connect_map(move |_| poll(true));
        }
        {
            let poll = Rc::clone(&poll);
            // The STACK, never the text view: an empty or paused log shows the
            // status page instead, which leaves the view unmapped — and a poll
            // gated on the view stopped there, so the first line written after a
            // clear (or after a filter matched nothing) never reached the pane.
            let stack = stack.clone();
            // Four times a second: a poll is a stat plus the bytes that
            // arrived, and a whole second of lag reads as a frozen log.
            gtk4::glib::timeout_add_local(std::time::Duration::from_millis(400), move || {
                if stack.is_mapped() {
                    poll(false);
                }
                gtk4::glib::ControlFlow::Continue
            });
        }
    }

    // One draw before the window appears, so the status card is never blank.
    render();
    toast_overlay.set_child(Some(&outer));
    toast_overlay
}

/// Below `NARROW_WIDTH` the right column moves under the log.
fn is_narrow(width: i32) -> bool {
    width > 0 && width < NARROW_WIDTH
}

/// The status square says running or not with its colour and its icon, the same
/// way the word beside it does.
fn set_head_state(tile: &gtk4::Image, running: bool) {
    tile.set_icon_name(Some(if running {
        "utilities-system-monitor-symbolic"
    } else {
        "dialog-error-symbolic"
    }));
    if running {
        tile.remove_css_class("diag-head-icon-off");
    } else {
        tile.add_css_class("diag-head-icon-off");
    }
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
        Kind::Delete => t("misc.chip_delete", "Xoá"),
        Kind::Flush => t("misc.chip_flush", "Bộ đệm"),
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

/// The settings a maintainer can act on. A typing bug is explained by the
/// method, the modes and their switches, the timings and the transport — not by
/// which keys the user picked for shortcuts, which page the window was left on,
/// or whether the log is paused. Everything outside this list is left out of the
/// report; app rules stay, because a rule decides how one app is typed in.
const REPORT_CONFIG_KEYS: &[&str] = &[
    "enabled",
    "method",
    "mode",
    "charset",
    "modern_tone_style",
    "free_tone_marking",
    "auto_correct",
    "extended_finals",
    "extended_onsets",
    "keep_composed_words",
    "keep_repeats_raw",
    "suggest_words",
    "dodge_inline_autocomplete",
    "autocomplete_fix",
    "preferred_backend",
    "backspace_tone",
    "auto_restore",
    "bypass_url",
    "w2u",
    "bracket",
    "em_dash",
    "auto_capitalize",
    "double_space_period",
    "macro_enabled",
    "capitalize_macro",
    "macro_in_off_mode",
    "macro_expand_valid_words",
    "abbreviations",
    "dictionary_enabled",
    "emoji_enabled",
    "hide_underline",
    "backspace_interval_ms",
    "post_delay_ms",
    "unicode_channel",
    "echo_stale_enabled",
    "echo_stale_ms",
    "idle_commit_enabled",
    "idle_commit_ms",
    "flow_per_key_delay_ms",
    "flow_backspace_restore_delay_ms",
    "experimental_features",
    "x11_delete_without_report",
    "preedit_for_xim",
    "nav_key_while_preedit",
    "app_rules",
];

/// What `config.toml` held, minus everything that is already the default: the
/// report quotes the file the engine actually loaded rather than what the
/// widgets show, and a pasted dump of sixty unchanged keys buries the three
/// that explain the bug.
fn config_lines(ui: &Rc<Ui>) -> Vec<String> {
    let text = Config::path()
        .and_then(|p| std::fs::read_to_string(p).ok())
        .or_else(|| toml::to_string_pretty(&ui.read()).ok())
        .unwrap_or_default();
    let defaults = toml::to_string_pretty(&Config::default()).unwrap_or_default();
    reportable_config_lines(&text, &defaults)
}

/// The kept keys, parsed as TOML so a section (the app rules) travels whole
/// instead of losing its inner lines to a key filter.
fn reportable_config_lines(text: &str, defaults: &str) -> Vec<String> {
    let Ok(parsed) = text.parse::<toml::Table>() else {
        // A hand-edited file that no longer parses is still worth quoting.
        return changed_config_lines(text, defaults);
    };
    let default: toml::Table = defaults.parse().unwrap_or_default();
    let mut kept = toml::Table::new();
    for (key, value) in parsed {
        if !REPORT_CONFIG_KEYS.contains(&key.as_str()) {
            continue;
        }
        if default.get(&key) == Some(&value) {
            continue;
        }
        kept.insert(key, value);
    }
    toml::to_string_pretty(&kept)
        .map(|text| text.lines().map(str::to_string).collect())
        .unwrap_or_default()
}

/// The lines of `text` whose key is missing from `defaults` or holds a different
/// value. Comments and blank lines go: they say nothing about this install.
fn changed_config_lines(text: &str, defaults: &str) -> Vec<String> {
    let by_key: std::collections::HashMap<&str, &str> = defaults
        .lines()
        .filter_map(|line| line.split_once(" = "))
        .map(|(key, value)| (key.trim(), value.trim()))
        .collect();
    text.lines()
        .filter(|line| {
            let line = line.trim();
            !line.is_empty() && !line.starts_with('#')
        })
        .filter(|line| match line.split_once(" = ") {
            Some((key, value)) => by_key.get(key.trim()) != Some(&value.trim()),
            None => true,
        })
        .map(|line| line.trim_end().to_string())
        .collect()
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

/// The clear button's work: the file goes, and so does the cache the pane draws
/// from. Emptying the file alone left the lines on screen, because the pane
/// never reads the file twice.
fn clear_log(
    path: &std::path::Path,
    reader: &Rc<RefCell<crate::gui::log_tail::Reader>>,
    parsed: &Rc<RefCell<Vec<Line>>>,
    render: &Rc<dyn Fn()>,
) {
    let _ = std::fs::remove_file(path);
    reader.borrow_mut().reset();
    parsed.borrow_mut().clear();
    render();
}

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

/// One line: a coloured band for the kind, the gap since the line before, then
/// the decoded body. The band is one mono glyph rather than 3 px of padding: a
/// text view has no per-line box, and the glyph keeps every line's height.
fn write(buffer: &gtk4::TextBuffer, iter: &mut gtk4::TextIter, lines: &[&Line]) {
    let press = t("misc.key_press", "nhấn");
    let release = t("misc.key_release", "thả");
    for line in lines {
        buffer.insert_with_tags_by_name(iter, "▌", &[tag_for(line.kind)]);
        let when = match line.rel_ms {
            Some(ms) => format!("+{ms}ms"),
            None => String::new(),
        };
        buffer.insert_with_tags_by_name(iter, &format!("{when:>8} "), &["log-time"]);
        buffer.insert(iter, &line.body.text(&press, &release));
        buffer.insert(iter, "\n");
    }
}

fn tag_for(kind: Kind) -> &'static str {
    match kind {
        Kind::Key => "log-key",
        Kind::Insert => "log-insert",
        Kind::Delete => "log-delete",
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
        ("log-delete", "warning_color", 1.0),
        ("log-flush", "view_fg_color", 0.6),
        ("log-error", "error_color", 1.0),
        ("log-other", "view_fg_color", 0.45),
        ("log-time", "view_fg_color", 0.45),
    ] {
        if let Some(created) = buffer.create_tag(Some(tag), &[]) {
            created.set_foreground_rgba(Some(&color(name, alpha)));
        }
    }
}

/// A label whose text did not change keeps its layout: writing the same string
/// twice still costs a relayout of everything around it.
fn set_if_changed(label: &gtk4::Label, text: &str) {
    if label.text() != text {
        label.set_text(text);
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
            "1795619563154 buf flush=per_key".to_string(),
            "1795619563158 st cur=3 len=3 comp=0".to_string(),
            "1795619563160 del ok=0 mode=protocol why=inject_failed n=1".to_string(),
            "1795619563164 ctx mode=preedit".to_string(),
        ]
    }

    #[test]
    fn every_kind_the_filter_bar_offers_has_a_label() {
        for kind in [
            Kind::Key,
            Kind::Insert,
            Kind::Delete,
            Kind::Flush,
            Kind::Error,
            Kind::Other,
        ] {
            assert!(!kind_label(kind).is_empty(), "{kind:?}");
        }
        assert_eq!(kind_label(Kind::Key), "Phím");
        assert_eq!(kind_label(Kind::Error), "Lỗi");
    }

    #[test]
    fn the_filter_bar_opens_on_every_kind() {
        assert_eq!(FILTERS.len(), 6);
        assert_eq!(
            FILTERS[0], None,
            "the first toggle is every kind, and it is the one turned on"
        );
        assert!(FILTERS[1..].iter().all(|kind| kind.is_some()));
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
    fn clearing_the_log_empties_what_the_pane_draws_from() {
        let dir = crate::debug_log::TempLogDir::new("debug-clear");
        let path = dir.dir().join("debug.log");
        std::fs::write(&path, "1795619563000 ctx start uinput=1\n").expect("a log");
        let reader = Rc::new(RefCell::new(crate::gui::log_tail::Reader::new(
            Some(path.clone()),
            VIEW_LINES,
        )));
        reader.borrow_mut().poll();
        let parsed = Rc::new(RefCell::new(vec![parse(
            "1795619563000 ctx start uinput=1",
            None,
        )
        .expect("a line")]));
        let rendered = Rc::new(Cell::new(0));
        let count = Rc::clone(&rendered);
        let render: Rc<dyn Fn()> = Rc::new(move || count.set(count.get() + 1));
        clear_log(&path, &reader, &parsed, &render);
        assert!(!path.exists(), "the file is gone");
        assert!(parsed.borrow().is_empty(), "the cache is gone");
        assert_eq!(rendered.get(), 1, "the pane redraws once");
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
            "ctx mode=preedit",
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
    fn the_report_carries_only_the_settings_that_matter_and_differ() {
        let defaults = "method = \"telex\"\nlanguage = \"vi\"\ncycle_mode_hotkey = \"\"\n";
        let file = "method = \"vni\"\nlanguage = \"en\"\ncycle_mode_hotkey = \"a\"\n\
                    mode = \"surrounding\"\n[[app_rules]]\npattern = \"firefox\"\n";
        let lines = reportable_config_lines(file, defaults);
        let joined = lines.join("\n");
        assert!(
            joined.contains("method"),
            "a changed method travels: {joined}"
        );
        assert!(joined.contains("mode"), "so does a changed mode: {joined}");
        assert!(
            joined.contains("app_rules") && joined.contains("firefox"),
            "an app rule travels whole: {joined}"
        );
        assert!(
            !joined.contains("language") && !joined.contains("cycle_mode_hotkey"),
            "the interface language and the user's own keys stay home: {joined}"
        );
    }

    #[test]
    fn the_report_carries_only_the_settings_that_differ() {
        let defaults = "mode = \"preedit\"\nlanguage = \"vi\"\n";
        let file = "# a comment\n\nmode = \"preedit\"\nlanguage = \"en\"\nspell_check = true\n";
        let kept = changed_config_lines(file, defaults);
        assert_eq!(
            kept,
            vec![
                "language = \"en\"".to_string(),
                "spell_check = true".to_string()
            ],
            "the unchanged key and the comment are dropped: {kept:?}"
        );
    }

    #[test]
    fn a_narrow_window_stacks_the_columns() {
        assert!(is_narrow(NARROW_WIDTH - 1));
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
