//! What the page can say about this install, and the text it copies: one place
//! so the boxes on screen and the pasted report cannot drift apart. Every value
//! here is read from outside the engine — its process, IBus, the file system —
//! so nothing is guessed and nothing costs the engine a microsecond.

use super::*;
use crate::gui::engine_probe::EngineStatus;

/// How a box reads at a glance: fine, worth a look, or plain information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    Ok,
    Warn,
    Info,
}

/// One box of the diagnostics grid.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cell {
    pub label: String,
    pub value: String,
    pub status: Status,
    /// What to do about a warning, or the full value of a clipped path.
    pub hint: Option<String>,
    /// Path rows open the folder on click instead of copying.
    pub open_dir: Option<std::path::PathBuf>,
}

impl Cell {
    fn info(label: &str, value: String) -> Cell {
        Cell {
            label: label.to_string(),
            value,
            status: Status::Info,
            hint: None,
            open_dir: None,
        }
    }

    fn path(label: &str, path: &std::path::Path) -> Cell {
        Cell {
            label: label.to_string(),
            value: path.display().to_string(),
            status: Status::Info,
            hint: Some(path.display().to_string()),
            open_dir: path.parent().map(|p| p.to_path_buf()),
        }
    }

    fn with_status(mut self, status: Status, hint: &str) -> Cell {
        self.status = status;
        if !hint.is_empty() {
            self.hint = Some(hint.to_string());
        }
        self
    }
}

/// Everything the grid reads, gathered once by the page. Keeping the window
/// out of here is what makes the boxes testable.
pub(super) struct Facts<'a> {
    pub preferred: crate::config::PreferredBackendConfig,
    pub mode: crate::config::ModeConfig,
    pub extension: (&'a str, &'a str, bool),
    pub config_path: Option<&'a std::path::Path>,
    pub log_path: Option<&'a std::path::Path>,
    pub engine: Option<&'a EngineStatus>,
    pub transport: Option<&'a str>,
}

/// The grid, in reading order. Paths come last so their double width lands on
/// the end of a row rather than in the middle of it.
pub(super) fn cells(facts: &Facts) -> Vec<Cell> {
    let status = facts.engine;
    let transport = facts.transport;
    let log = facts.log_path;
    let mut out = Vec::new();
    match status {
        Some(status) => {
            let began = status
                .started
                .and_then(|time| {
                    let secs = time.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
                    gtk4::glib::DateTime::from_unix_local(secs)
                        .ok()?
                        .format("%H:%M")
                        .ok()?
                        .to_string()
                        .into()
                })
                .unwrap_or_else(|| "?".to_string());
            out.push(
                Cell::info(
                    &t("rows.debug_engine.title", "Engine"),
                    format!(
                        "{} · PID {} · {}",
                        t("rows.debug_engine.running", "đang chạy"),
                        status.pid,
                        began
                    ),
                )
                .with_status(Status::Ok, ""),
            );
            out.push(Cell::path(
                &t("rows.debug_engine.binary", "Tệp đang chạy"),
                &status.exe,
            ));
            let fresh = if status.stale {
                Cell::info(&t("rows.debug_engine.fresh", "Bản cài"), "?".to_string()).with_status(
                    Status::Warn,
                    &t(
                        "rows.debug_engine.stale_hint",
                        "Bản cài mới hơn tiến trình đang chạy. Khởi động lại IBus để dùng bản mới.",
                    ),
                )
            } else {
                Cell::info(
                    &t("rows.debug_engine.fresh", "Bản cài"),
                    t("rows.debug_engine.current", "Khớp bản đang chạy"),
                )
                .with_status(Status::Ok, "")
            };
            out.push(if status.stale {
                Cell {
                    value: t("rows.debug_engine.stale", "Mới hơn, cần khởi động lại IBus"),
                    ..fresh
                }
            } else {
                fresh
            });
            let selected = status.selected.clone().unwrap_or_else(|| "?".to_string());
            let selected_status = if selected == crate::gui::engine_probe::ENGINE_NAME {
                Status::Ok
            } else {
                Status::Info
            };
            out.push(
                Cell::info(&t("rows.debug_engine.selected", "IBus đang chọn"), selected)
                    .with_status(selected_status, ""),
            );
            out.push(match status.registered {
                Some(true) => Cell::info(
                    &t("rows.debug_engine.registered", "Đăng ký"),
                    t("rows.debug_engine.listed", "IBus có component"),
                )
                .with_status(Status::Ok, ""),
                Some(false) => Cell::info(
                    &t("rows.debug_engine.registered", "Đăng ký"),
                    t(
                        "rows.debug_engine.unregistered",
                        "IBus không thấy component",
                    ),
                )
                .with_status(
                    Status::Warn,
                    &t(
                        "rows.debug_engine.unregistered_hint",
                        "Cài lại gói rồi khởi động lại IBus để IBus đọc lại component.",
                    ),
                ),
                None => Cell::info(
                    &t("rows.debug_engine.registered", "Đăng ký"),
                    "?".to_string(),
                ),
            });
        }
        None => {
            out.push(
                Cell::info(
                    &t("rows.debug_engine.title", "Engine"),
                    t("rows.debug_engine.not_running", "Không thấy tiến trình"),
                )
                .with_status(
                    Status::Warn,
                    &t(
                        "rows.debug_engine.not_running_hint",
                        "IBus chỉ khởi động engine khi bạn gõ ở một ứng dụng. Gõ thử ở đây rồi bấm Đọc lại.",
                    ),
                ),
            );
        }
    }

    let transport_shown = transport
        .map(|s| s.to_string())
        .unwrap_or_else(|| t("rows.debug_transport.none", "chưa thấy trong log"));
    let chosen = backend_name(facts.preferred);
    // Auto landing on a concrete transport is normal, but it is also the case
    // where typing breaks in one application and not another.
    let transport_cell = if facts.preferred == crate::config::PreferredBackendConfig::Auto
        && transport.is_some_and(|t| t != "auto")
    {
        Cell::info(
            &t("rows.debug_transport.title", "Cách chèn thực tế"),
            transport_shown,
        )
        .with_status(
            Status::Warn,
            &format!(
                "{}: auto → {}. {}",
                t(
                    "rows.debug_transport.auto_note",
                    "Cách chèn auto đang rơi về"
                ),
                transport.unwrap_or("?"),
                t(
                    "rows.debug_transport.auto_hint",
                    "Đặt luật theo app nếu gõ lỗi ở một ứng dụng.",
                )
            ),
        )
    } else {
        Cell::info(
            &t("rows.debug_transport.title", "Cách chèn thực tế"),
            transport_shown,
        )
    };
    out.push(transport_cell);

    out.push(Cell::info(
        &t("rows.debug_info.version", "Phiên bản"),
        env!("CARGO_PKG_VERSION").to_string(),
    ));
    out.push(Cell::info(
        &t("rows.debug_info.session", "Phiên"),
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".to_string()),
    ));
    out.push(Cell::info(
        &t("rows.debug_info.mode", "Chế độ gõ"),
        mode_label(facts.mode).to_string(),
    ));
    out.push(Cell::info(
        &t("rows.debug_info.backend", "Cách chèn"),
        chosen.to_string(),
    ));
    let (ext_key, ext_fallback, ext_running) = facts.extension;
    let extension = Cell::info(
        &t("rows.debug_info.extension", "Extension GNOME"),
        t(ext_key, ext_fallback).to_string(),
    );
    out.push(if ext_running {
        extension.with_status(Status::Ok, "")
    } else {
        extension.with_status(
            Status::Warn,
            &t(
                "rows.debug_info.extension_hint",
                "Extension chưa chạy nên app tự báo tên ứng dụng có thể sai. Bật nó ở tab Cài đặt nâng cao.",
            ),
        )
    });

    if let Some(path) = facts.config_path {
        out.push(Cell::path(&t("rows.debug_info.config", "Cấu hình"), path));
    }
    match log {
        Some(path) => out.push(Cell::path(&t("rows.debug_info.log", "Nhật ký"), path)),
        None => out.push(Cell::info(
            &t("rows.debug_info.log", "Nhật ký"),
            t("misc.debug_log_missing", "Không có đường dẫn log"),
        )),
    }
    out
}

/// "8 ổn · 2 cần xem" — the two numbers that say whether this page is telling
/// a story about a healthy install.
pub(super) fn counts(cells: &[Cell]) -> (usize, usize) {
    let ok = cells.iter().filter(|c| c.status == Status::Ok).count();
    let warn = cells.iter().filter(|c| c.status == Status::Warn).count();
    (ok, warn)
}

/// The transport the last log lines mention. The engine picks one at runtime
/// (auto can land on uinput, xtest or libei), and that choice is written only
/// to the log — this lifts it back out instead of guessing from the config.
pub(super) fn transport_from_log(lines: &[String]) -> Option<String> {
    const NAMES: [&str; 4] = ["uinput", "xtest", "libei", "wlr"];
    lines.iter().rev().find_map(|line| {
        let lower = line.to_lowercase();
        NAMES
            .iter()
            .find(|name| lower.contains(*name))
            .map(|name| (*name).to_string())
    })
}

/// The copied report: the diagnostics table, the settings that were in force,
/// then the log tail. Same order as the page reads top to bottom.
pub(super) fn report(cells: &[Cell], config: &[String], log: &[String]) -> String {
    let mut out = format!("{} {}", t("app.name", "MăngRS"), env!("CARGO_PKG_VERSION"));
    let (ok, warn) = counts(cells);
    out.push_str(&format!(
        "\n{} {ok} · {warn} {}",
        t("misc.diag_title", "Chẩn đoán"),
        t("misc.diag_warn", "cần xem")
    ));
    for cell in cells {
        let mark = match cell.status {
            Status::Ok => "ok",
            Status::Warn => "warn",
            Status::Info => "info",
        };
        out.push_str(&format!("\n[{mark}] {}: {}", cell.label, cell.value));
    }
    if !config.is_empty() {
        out.push_str(&format!(
            "\n--- {} ---",
            t("debug_report.config_section", "config.toml đang nạp")
        ));
        for line in config {
            out.push('\n');
            out.push_str(line);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }

    fn status(stale: bool, registered: Option<bool>) -> EngineStatus {
        EngineStatus {
            pid: 4242,
            exe: std::path::PathBuf::from("/usr/lib/ibus-mangrs/ibus-engine-mangrs"),
            stale,
            started: Some(std::time::SystemTime::now()),
            selected: Some("ibus-mangrs".to_string()),
            registered,
        }
    }

    #[test]
    fn the_report_carries_every_box_the_settings_and_the_log_tail() {
        let cells = vec![
            Cell::info("Engine", "đang chạy".to_string()).with_status(Status::Ok, ""),
            Cell::info("Đăng ký", "không thấy".to_string()).with_status(Status::Warn, "cài lại"),
        ];
        let text = report(&cells, &lines(&["bỏ dấu = \"old\""]), &lines(&["a", "b"]));
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
        assert!(text.contains("Chẩn đoán 1 · 1 cần xem"), "{text}");
        assert!(text.contains("[ok] Engine: đang chạy"), "{text}");
        assert!(text.contains("[warn] Đăng ký: không thấy"), "{text}");
        assert!(
            text.contains("--- config.toml đang nạp ---\nbỏ dấu"),
            "{text}"
        );
        assert!(text.ends_with("--- log cuối ---\na\nb"), "{text}");
    }

    #[test]
    fn a_stale_binary_and_a_missing_component_ask_for_a_fix() {
        let missing_engine = cells_with(None, None);
        assert_eq!(missing_engine[0].status, Status::Warn, "{missing_engine:?}");

        let stale = cells_with(Some(&status(true, Some(false))), None);
        let stale_cell = stale
            .iter()
            .find(|c| c.label == "Bản cài")
            .expect("a fresh-build box");
        assert_eq!(stale_cell.status, Status::Warn);
        assert!(stale_cell
            .hint
            .as_deref()
            .unwrap_or("")
            .contains("Khởi động lại IBus"));
        let unregistered = stale
            .iter()
            .find(|c| c.label == "Đăng ký")
            .expect("a registration box");
        assert_eq!(unregistered.status, Status::Warn);
    }

    #[test]
    fn auto_landing_on_uinput_is_the_one_known_warning() {
        let boxes = cells_with(Some(&status(false, Some(true))), Some("uinput"));
        let transport = boxes
            .iter()
            .find(|c| c.label == "Cách chèn thực tế")
            .expect("a transport box");
        assert_eq!(transport.status, Status::Warn, "{transport:?}");
        assert!(transport.hint.as_deref().unwrap_or("").contains("auto"));

        // A backend the user picked on purpose is not a warning.
        let explicit = cells(&facts(
            Some(&status(false, Some(true))),
            Some("uinput"),
            crate::config::PreferredBackendConfig::Uinput,
        ))
        .iter()
        .find(|c| c.label == "Cách chèn thực tế")
        .expect("a transport box")
        .status;
        assert_eq!(explicit, Status::Info);
    }

    #[test]
    fn the_transport_comes_from_the_newest_matching_line() {
        let log = lines(&[
            "tone: hoà → ỏ",
            "transport: uinput ready",
            "backspace: uinput 1",
            "nothing here",
        ]);
        assert_eq!(transport_from_log(&log), Some("uinput".to_string()));
        assert_eq!(transport_from_log(&[]), None);
    }

    fn facts<'a>(
        engine: Option<&'a EngineStatus>,
        transport: Option<&'a str>,
        preferred: crate::config::PreferredBackendConfig,
    ) -> Facts<'a> {
        Facts {
            preferred,
            mode: crate::config::ModeConfig::default(),
            extension: ("rows.focus_extension.subtitle_running", "Đang chạy", true),
            config_path: Some(std::path::Path::new(
                "/home/u/.config/ibus-mangrs/config.toml",
            )),
            log_path: Some(std::path::Path::new("/home/u/.cache/ibus-mangrs/debug.log")),
            engine,
            transport,
        }
    }

    fn cells_with(engine: Option<&EngineStatus>, transport: Option<&str>) -> Vec<Cell> {
        cells(&facts(
            engine,
            transport,
            crate::config::PreferredBackendConfig::Auto,
        ))
    }
}
