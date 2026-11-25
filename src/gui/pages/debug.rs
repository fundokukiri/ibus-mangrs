//! Page "Gỡ lỗi": the tools that only exist while debug mode is on — the log
//! file (size, folder, clear), a filtered tail viewer that follows the file,
//! and a diagnostics block whose report can be pasted into an issue.
//!
//! The switch that turns the log on stays in "Cài đặt nâng cao": this page is
//! the consequence of that switch, not a second place to set it.

use super::*;
use crate::gui::log_tail::{size_label, tail, TAIL_BYTES};

/// Size plus mtime of the log as last read: the poller re-renders only when
/// the file moved, so an idle page costs one `stat` a second and nothing else.
type Stamp = Option<(u64, Option<std::time::SystemTime>)>;

/// Lines the viewer holds. More than anyone reads in one screen, few enough
/// that the per-second re-read stays invisible.
const VIEW_LINES: usize = 500;

/// Lines the copied report carries: enough context around the failure, short
/// enough to paste into an issue without trimming.
const REPORT_LINES: usize = 50;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
    let page = adw::PreferencesPage::new();
    let toast_overlay = adw::ToastOverlay::new();
    let path = crate::debug_log::log_path();

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
    let follow = gtk4::CheckButton::with_label(&t("misc.debug_follow", "Theo dõi"));
    follow.set_active(true);
    let tools = gtk4::Box::new(Orientation::Horizontal, 12);
    tools.set_margin_top(6);
    tools.set_margin_bottom(6);
    tools.set_margin_start(12);
    tools.set_margin_end(12);
    tools.append(&filter);
    tools.append(&follow);
    viewer_group.add(&tools);

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
        let filter = filter.clone();
        let path = path.clone();
        Rc::new(move || {
            let lines = path
                .as_deref()
                .map(|p| tail(p, VIEW_LINES, TAIL_BYTES))
                .unwrap_or_default();
            let shown = visible_lines(&lines, &filter.text());
            view.buffer().set_text(&shown.join("\n"));
            if follow.is_active() {
                let adj = scroller.vadjustment();
                adj.set_value(adj.upper() - adj.page_size());
            }
        })
    };
    {
        let render = render.clone();
        filter.connect_search_changed(move |_| render());
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

    // --- Chẩn đoán --------------------------------------------------------
    let diag_group = adw::PreferencesGroup::builder()
        .title(t("groups.debug.diag.title", "Chẩn đoán"))
        .build();
    let info = diagnostic_info(ui, path.as_deref());
    for (label, value) in &info {
        diag_group.add(&action_row(label, value));
    }
    let copy_row = action_row(
        &t("rows.debug_report.title", "Báo cáo"),
        &t(
            "rows.debug_report.subtitle",
            "Sao chép thông tin trên và 50 dòng log cuối để dán vào issue",
        ),
    );
    let copy_btn = action_button(
        Some("edit-copy-symbolic"),
        &t("misc.debug_copy_report", "Sao chép"),
    );
    copy_row.add_suffix(&copy_btn);
    diag_group.add(&copy_row);
    page.add(&diag_group);
    {
        let overlay = toast_overlay.clone();
        let ui = ui.clone();
        copy_btn.connect_clicked(move |btn| {
            let lines = path
                .as_deref()
                .map(|p| tail(p, REPORT_LINES, TAIL_BYTES))
                .unwrap_or_default();
            let text = report(&diagnostic_info(&ui, path.as_deref()), &lines);
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

/// The lines the viewer shows: every line for a blank query, otherwise the
/// ones holding it, case-insensitively. A user searching "tone" should not
/// have to match the log's own capitalisation.
pub(super) fn visible_lines(lines: &[String], query: &str) -> Vec<String> {
    let query = query.trim().to_lowercase();
    lines
        .iter()
        .filter(|line| query.is_empty() || line.to_lowercase().contains(&query))
        .cloned()
        .collect()
}

/// What the page knows without asking anyone: the build, the session, the
/// typing mode, the extension and the two paths. Rows and report share it so
/// the copied text cannot drift from the screen.
pub(super) fn diagnostic_info(ui: &Rc<Ui>, log: Option<&std::path::Path>) -> Vec<(String, String)> {
    let config = ui.read();
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".to_string());
    vec![
        (
            t("rows.debug_info.version", "Phiên bản"),
            env!("CARGO_PKG_VERSION").to_string(),
        ),
        (t("rows.debug_info.session", "Phiên"), session),
        (
            t("rows.debug_info.mode", "Chế độ gõ"),
            mode_label(config.mode).to_string(),
        ),
        (
            t("rows.debug_info.backend", "Cách chèn"),
            backend_name(config.preferred_backend).to_string(),
        ),
        (t("rows.debug_info.extension", "Extension GNOME"), {
            let (key, fallback) = extension_row_for(extension_state()).subtitle;
            t(key, fallback).to_string()
        }),
        (
            t("rows.debug_info.config", "Cấu hình"),
            Config::path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "?".to_string()),
        ),
        (
            t("rows.debug_info.log", "Nhật ký"),
            log.map(|p| p.display().to_string())
                .unwrap_or_else(|| "?".to_string()),
        ),
    ]
}

/// The saved backend as the config file spells it — machine names, not the
/// translated combo labels: a report is read next to `config.toml`.
fn backend_name(backend: crate::config::PreferredBackendConfig) -> &'static str {
    use crate::config::PreferredBackendConfig as B;
    match backend {
        B::Auto => "auto",
        B::Uinput => "uinput",
        B::XTest => "xtest",
        B::WlrVirtualKeyboard => "wlr-virtual-keyboard",
        B::Libei => "libei",
    }
}

/// The copied report: one headline line, the labelled rows, then the log tail
/// under a separator. Plain text — issue templates and chat both take it.
pub(super) fn report(info: &[(String, String)], log: &[String]) -> String {
    let mut out = format!("{} {}", t("app.name", "MăngRS"), env!("CARGO_PKG_VERSION"));
    for (label, value) in info {
        out.push_str(&format!("\n{label}: {value}"));
    }
    out.push_str(&format!(
        "\n--- {} ---",
        t("debug_report.log_section", "log cuối")
    ));
    for line in log {
        out.push('\n');
        out.push_str(line);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_log_filter_is_case_insensitive() {
        let all = lines(&["tone: hoà → ỏ", "Preedit → Surrounding", "backspace 1"]);
        assert_eq!(visible_lines(&all, "").len(), 3, "a blank query keeps all");
        assert_eq!(visible_lines(&all, "  ").len(), 3, "spaces are not a query");
        assert_eq!(
            visible_lines(&all, "PREEDIT"),
            lines(&["Preedit → Surrounding"])
        );
        assert_eq!(visible_lines(&all, "hoà"), lines(&["tone: hoà → ỏ"]));
        assert!(visible_lines(&all, "nope").is_empty());
    }

    #[test]
    fn the_report_carries_the_version_and_the_log_tail() {
        let info = lines_info(&[("Chế độ gõ", "Gạch chân")]);
        let text = report(&info, &lines(&["a", "b"]));
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
        assert!(text.contains("Chế độ gõ: Gạch chân"), "{text}");
        assert!(text.ends_with("--- log cuối ---\na\nb"), "{text}");
    }

    fn lines_info(rows: &[(&str, &str)]) -> Vec<(String, String)> {
        rows.iter()
            .map(|(l, v)| ((*l).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn the_log_subtitle_reports_size_then_absence() {
        let dir = TempDir::new("mangrs-debug-subtitle");
        let path = dir.path().join("debug.log");
        assert!(log_subtitle(Some(&path)).contains("chưa ghi"));
        std::fs::write(&path, vec![b'x'; 2000]).expect("write log");
        let shown = log_subtitle(Some(&path));
        assert!(shown.starts_with("2 KB · "), "{shown}");
    }
}
