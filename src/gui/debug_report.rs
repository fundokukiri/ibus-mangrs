//! What the page can say about this install, and the text it copies: one place
//! so the rows on screen and the pasted report cannot drift apart. Every value
//! here is read from outside the engine — its process, IBus, the file system —
//! so nothing is guessed and nothing costs the engine a microsecond.

use super::*;
use crate::gui::engine_probe::EngineStatus;

/// How a row reads at a glance: fine, worth a look, or plain information.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Status {
    Ok,
    Warn,
    Info,
}

/// One row of the tools column.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Cell {
    pub label: String,
    pub value: String,
    pub status: Status,
    /// What to do about a warning, or the full value behind a clipped one.
    pub hint: Option<String>,
    /// Path rows: the folder the row's button opens.
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

    /// A file row shows the file's name only: the full path is long, and the
    /// row has a folder button for anyone who needs it.
    fn path(label: &str, path: &std::path::Path) -> Cell {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| path.display().to_string());
        Cell {
            label: label.to_string(),
            value: name,
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

/// One titled group of rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Section {
    pub title: String,
    pub rows: Vec<Cell>,
}

/// Everything the rows read, gathered once by the page. Keeping the window out
/// of here is what makes the rows testable.
pub(super) struct Facts<'a> {
    /// The machine the window runs on: the report is read on another one.
    pub machine: &'a crate::gui::machine::Machine,
    pub preferred: crate::config::PreferredBackendConfig,
    pub mode: crate::config::ModeConfig,
    pub extension: (&'a str, &'a str, bool),
    pub config_path: Option<&'a std::path::Path>,
    pub log_path: Option<&'a std::path::Path>,
    pub engine: Option<&'a EngineStatus>,
    /// The backend the log says was used for the last keystroke.
    pub backend: Option<&'a str>,
}

/// The rows, grouped the way the right-hand column stacks them. The version,
/// the PID and the latency numbers are not here: they ride in the status bar.
pub(super) fn sections(facts: &Facts) -> Vec<Section> {
    let status = facts.engine;
    let mut out = Vec::new();

    let mut system = Vec::new();
    let selected = status
        .and_then(|s| s.selected.clone())
        .unwrap_or_else(|| "?".to_string());
    let selected_status = match status {
        None => Status::Warn,
        Some(_) if selected == crate::gui::engine_probe::ENGINE_NAME => Status::Ok,
        Some(_) => Status::Info,
    };
    system.push(
        Cell::info(&t("rows.debug_engine.selected", "IBus"), selected).with_status(
            selected_status,
            &t(
                "rows.debug_engine.selected_hint",
                "IBus đang chạy engine khác. Chọn MăngRS trong menu bộ gõ.",
            ),
        ),
    );
    system.push(match status.and_then(|s| s.registered) {
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
    let install = match status {
        Some(s) if s.stale => Cell::info(
            &t("rows.debug_engine.fresh", "Bản cài"),
            t("rows.debug_engine.stale", "Mới hơn, cần khởi động lại IBus"),
        )
        .with_status(
            Status::Warn,
            &t(
                "rows.debug_engine.stale_hint",
                "Bản cài mới hơn tiến trình đang chạy. Khởi động lại IBus để dùng bản mới.",
            ),
        ),
        // The tooltip names the binary that is actually typing: the only way
        // to tell two installs apart without opening a terminal.
        Some(s) => Cell::info(
            &t("rows.debug_engine.fresh", "Bản cài"),
            t("rows.debug_engine.current", "Khớp bản đang chạy"),
        )
        .with_status(Status::Ok, &s.exe.display().to_string()),
        None => Cell::info(
            &t("rows.debug_engine.fresh", "Bản cài"),
            t("rows.debug_engine.unknown", "Không rõ, engine chưa chạy"),
        ),
    };
    system.push(install);
    let (ext_key, ext_fallback, ext_running) = facts.extension;
    let extension = Cell::info(
        &t("rows.debug_info.extension", "Extension GNOME"),
        t(ext_key, ext_fallback).to_string(),
    );
    system.push(if ext_running {
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
    system.push(Cell::info(
        &t("rows.debug_info.session", "Phiên"),
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".to_string()),
    ));
    out.push(Section {
        title: t("misc.section_system", "Hệ thống"),
        rows: system,
    });

    let mut insert = Vec::new();
    insert.push(Cell::info(
        &t("rows.debug_info.mode", "Chế độ gõ"),
        mode_label(facts.mode).to_string(),
    ));
    let configured = backend_name(facts.preferred);
    let shown = facts
        .backend
        .map(|s| s.to_string())
        .unwrap_or_else(|| t("rows.debug_transport.none", "chưa thấy trong log"));
    // Auto landing on a concrete backend is normal, but it is also the case
    // where typing breaks in one application and not another.
    let auto_fallback = facts.preferred == crate::config::PreferredBackendConfig::Auto
        && facts.backend.is_some_and(|b| b != "auto");
    // Both rows carry the same tint when they disagree: the pair is the
    // evidence, and one of them alone says nothing.
    let configured_cell = Cell::info(
        &t("rows.debug_info.backend", "Cấu hình"),
        configured.to_string(),
    );
    insert.push(if auto_fallback {
        configured_cell.with_status(Status::Warn, "")
    } else {
        configured_cell
    });
    let actual = Cell::info(&t("rows.debug_transport.title", "Thực tế"), shown);
    insert.push(if auto_fallback {
        actual.with_status(
            Status::Warn,
            &format!(
                "{} {}. {}",
                t(
                    "rows.debug_transport.auto_note",
                    "Cách chèn auto đang rơi về"
                ),
                facts.backend.unwrap_or("?"),
                t(
                    "rows.debug_transport.auto_hint",
                    "Nếu gõ lỗi ở một ứng dụng, hãy đặt luật theo app cho nó.",
                )
            ),
        )
    } else {
        actual
    });
    out.push(Section {
        title: t("misc.section_insert", "Cách chèn"),
        rows: insert,
    });

    let mut files = Vec::new();
    match facts.config_path {
        Some(path) => files.push(Cell::path(&t("rows.debug_info.config", "Cấu hình"), path)),
        None => files.push(Cell::info(
            &t("rows.debug_info.config", "Cấu hình"),
            t("misc.debug_log_missing", "Không có đường dẫn"),
        )),
    }
    match facts.log_path {
        Some(path) => files.push(Cell::path(&t("rows.debug_info.log", "Nhật ký"), path)),
        None => files.push(Cell::info(
            &t("rows.debug_info.log", "Nhật ký"),
            t("misc.debug_log_missing", "Không có đường dẫn log"),
        )),
    }
    let machine = facts.machine;
    let mut host = Vec::new();
    for (label_key, label_fallback, value) in [
        ("rows.debug_machine.os", "Hệ điều hành", machine.os.clone()),
        ("rows.debug_machine.kernel", "Nhân", machine.kernel.clone()),
        (
            "rows.debug_machine.desktop",
            "Màn hình nền",
            machine.desktop.clone(),
        ),
        ("rows.debug_machine.ibus", "IBus", machine.ibus.clone()),
    ] {
        host.push(Cell::info(&t(label_key, label_fallback), value));
    }
    out.push(Section {
        title: t("misc.section_machine", "Máy"),
        rows: host,
    });

    out.push(Section {
        title: t("misc.section_files", "Tệp"),
        rows: files,
    });
    out
}

/// Every row of every group, in reading order.
pub(super) fn flat(sections: &[Section]) -> Vec<Cell> {
    sections
        .iter()
        .flat_map(|section| section.rows.iter().cloned())
        .collect()
}

/// "8 ổn · 2 cần xem" — the two numbers that say whether this page is telling
/// a story about a healthy install.
pub(super) fn counts(cells: &[Cell]) -> (usize, usize) {
    let ok = cells.iter().filter(|c| c.status == Status::Ok).count();
    let warn = cells.iter().filter(|c| c.status == Status::Warn).count();
    (ok, warn)
}

/// The backend the engine named in the log. `auto` is resolved at runtime
/// (uinput, xtest, libei), and that choice is written only there.
pub(super) fn backend_from_log(lines: &[String]) -> Option<String> {
    for line in lines.iter().rev() {
        let Some(rest) = line.split_once("injection backend:").map(|(_, r)| r.trim()) else {
            continue;
        };
        let name = rest.split([' ', '(']).next().unwrap_or("").trim();
        if !name.is_empty() && name != "none" {
            return Some(name.to_string());
        }
    }
    transport_from_log(lines)
}

/// Older logs and the transport's own lines name a backend without the
/// `injection backend:` prefix.
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

/// The copied report: the rows by group, the settings that were in force, then
/// the log tail. Same order as the page reads top to bottom.
pub(super) fn report(sections: &[Section], config: &[String], log: &[String]) -> String {
    let all = flat(sections);
    let (ok, warn) = counts(&all);
    // Markdown, because the report's destination is a GitHub issue: headings,
    // bullets and fenced blocks paste as they are instead of arriving as one
    // grey wall of text.
    let mut out = format!(
        "### {} {} - {} {ok} · {warn} {}\n",
        t("app.name", "MăngRS"),
        env!("CARGO_PKG_VERSION"),
        t("misc.diag_title", "Chẩn đoán"),
        t("misc.diag_warn", "cần xem")
    );
    // The first thing a maintainer needs is how to make it happen again, so the
    // report arrives with the questions already asked.
    out.push_str(&format!(
        "\n#### {}\n\n{}
",
        t("debug_report.repro_section", "Cách tái hiện"),
        t(
            "debug_report.repro_body",
            "- Ứng dụng:\n- Các bước:\n  1. \n  2. \n- Chữ đã gõ:\n- Mong đợi:\n- Thực tế:"
        )
    ));
    for section in sections {
        out.push_str(&format!("\n#### {}\n\n", section.title));
        for cell in &section.rows {
            // A file row carries its full path: the name alone cannot be
            // checked by whoever reads the issue.
            let value = match &cell.open_dir {
                Some(_) => cell.hint.as_deref().unwrap_or(&cell.value),
                None => cell.value.as_str(),
            };
            let look = if cell.status == Status::Warn {
                format!(" {}", t("debug_report.needs_a_look", "- cần xem"))
            } else {
                String::new()
            };
            out.push_str(&format!("- **{}**: `{value}`{look}\n", cell.label));
            if cell.status == Status::Warn {
                if let Some(hint) = &cell.hint {
                    out.push_str(&format!("  - _{hint}_\n"));
                }
            }
        }
    }
    if !config.is_empty() {
        out.push_str(&format!(
            "\n#### {}\n\n```toml\n{}\n```\n",
            t("debug_report.config_section", "config.toml đang nạp"),
            config.join("\n")
        ));
    }
    if !log.is_empty() {
        out.push_str(&format!(
            "\n#### {}\n\n```text\n{}\n```\n",
            t("debug_report.log_section", "Dòng log cuối"),
            log.join("\n")
        ));
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

    fn machine() -> &'static crate::gui::machine::Machine {
        static MACHINE: std::sync::OnceLock<crate::gui::machine::Machine> =
            std::sync::OnceLock::new();
        MACHINE.get_or_init(|| crate::gui::machine::Machine {
            os: "Arch Linux".to_string(),
            kernel: "6.12.1".to_string(),
            desktop: "GNOME 47.1 · wayland".to_string(),
            ibus: "1.5.35".to_string(),
        })
    }

    fn facts<'a>(
        engine: Option<&'a EngineStatus>,
        backend: Option<&'a str>,
        preferred: crate::config::PreferredBackendConfig,
    ) -> Facts<'a> {
        Facts {
            machine: machine(),
            preferred,
            mode: crate::config::ModeConfig::default(),
            extension: ("rows.focus_extension.subtitle_running", "Đang chạy", true),
            config_path: Some(std::path::Path::new(
                "/home/u/.config/ibus-mangrs/config.toml",
            )),
            log_path: Some(std::path::Path::new("/home/u/.cache/ibus-mangrs/debug.log")),
            engine,
            backend,
        }
    }

    fn rows(engine: Option<&EngineStatus>, backend: Option<&str>) -> Vec<Cell> {
        flat(&sections(&facts(
            engine,
            backend,
            crate::config::PreferredBackendConfig::Auto,
        )))
    }

    #[test]
    fn the_rows_come_in_three_groups() {
        let groups = sections(&facts(
            None,
            None,
            crate::config::PreferredBackendConfig::Auto,
        ));
        let titles: Vec<&str> = groups.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["Hệ thống", "Cách chèn", "Máy", "Tệp"]);
        assert_eq!(groups[0].rows.len(), 5, "the system rows stay five");
        assert_eq!(groups[1].rows.len(), 3);
        assert_eq!(groups[2].rows.len(), 4, "os, kernel, desktop, ibus");
        assert_eq!(groups[3].rows.len(), 2);
    }

    #[test]
    fn a_stale_binary_and_a_missing_component_ask_for_a_fix() {
        let missing = rows(None, None);
        let selected = missing
            .iter()
            .find(|c| c.label == "IBus")
            .expect("a selection row");
        assert_eq!(selected.status, Status::Warn, "no engine is a warning");

        let stale = rows(Some(&status(true, Some(false))), None);
        let install = stale
            .iter()
            .find(|c| c.label == "Bản cài")
            .expect("an install row");
        assert_eq!(install.status, Status::Warn);
        assert!(install
            .hint
            .as_deref()
            .unwrap_or("")
            .contains("Khởi động lại IBus"));
        let unregistered = stale
            .iter()
            .find(|c| c.label == "Đăng ký")
            .expect("a registration row");
        assert_eq!(unregistered.status, Status::Warn);
    }

    #[test]
    fn auto_landing_on_a_real_backend_is_the_one_known_warning() {
        let boxes = rows(Some(&status(false, Some(true))), Some("uinput"));
        let actual = boxes
            .iter()
            .find(|c| c.label == "Thực tế")
            .expect("a backend row");
        assert_eq!(actual.value, "uinput");
        assert_eq!(actual.status, Status::Warn, "{actual:?}");
        assert!(actual.hint.as_deref().unwrap_or("").contains("auto"));

        // A backend the user picked on purpose is not a warning.
        let explicit = flat(&sections(&facts(
            Some(&status(false, Some(true))),
            Some("uinput"),
            crate::config::PreferredBackendConfig::Uinput,
        )))
        .into_iter()
        .find(|c| c.label == "Thực tế")
        .expect("a backend row")
        .status;
        assert_eq!(explicit, Status::Info);
    }

    #[test]
    fn the_backend_comes_from_the_line_the_engine_names_it_in() {
        let log = lines(&[
            "injection backend: libei (matrix)",
            "transport: uinput ready",
            "injection backend: uinput (fallback — libei denied)",
        ]);
        assert_eq!(backend_from_log(&log), Some("uinput".to_string()));
        // A log without the named line falls back to the loose search.
        assert_eq!(
            backend_from_log(&lines(&["transport: xtest ready"])),
            Some("xtest".to_string())
        );
        // `none` means no backend was picked yet, not a name.
        assert_eq!(
            backend_from_log(&lines(&["injection backend: none (matrix)"])),
            None
        );
        assert_eq!(backend_from_log(&[]), None);
    }

    #[test]
    fn a_file_row_shows_its_name_and_keeps_the_path_in_the_report() {
        let files = flat(&sections(&facts(
            None,
            None,
            crate::config::PreferredBackendConfig::Auto,
        )));
        let config = files
            .iter()
            .find(|c| c.label == "Cấu hình" && c.open_dir.is_some())
            .expect("a config row");
        assert_eq!(config.value, "config.toml");
        assert_eq!(
            config.hint.as_deref(),
            Some("/home/u/.config/ibus-mangrs/config.toml")
        );
        let text = report(
            &sections(&facts(
                None,
                None,
                crate::config::PreferredBackendConfig::Auto,
            )),
            &[],
            &[],
        );
        assert!(
            text.contains("- **Cấu hình**: `/home/u/.config/ibus-mangrs/config.toml`"),
            "{text}"
        );
    }

    #[test]
    fn the_report_carries_every_row_the_settings_and_the_log_tail() {
        let groups = vec![Section {
            title: "Hệ thống".to_string(),
            rows: vec![
                Cell::info("Engine", "đang chạy".to_string()).with_status(Status::Ok, ""),
                Cell::info("Đăng ký", "không thấy".to_string())
                    .with_status(Status::Warn, "cài lại"),
            ],
        }];
        let text = report(&groups, &lines(&["bỏ dấu = \"old\""]), &lines(&["a", "b"]));
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
        assert!(text.starts_with("### MăngRS"), "{text}");
        assert!(text.contains("Chẩn đoán 1 · 1 cần xem"), "{text}");
        assert!(text.contains("\n#### Hệ thống\n"), "{text}");
        assert!(
            text.contains("- Ứng dụng:\n- Các bước:"),
            "the report asks how to reproduce it: {text}"
        );
        assert!(text.contains("- **Engine**: `đang chạy`"), "{text}");
        assert!(
            text.contains("- **Đăng ký**: `không thấy` - cần xem\n  - _cài lại_"),
            "{text}"
        );
        assert!(
            text.contains("#### config.toml đang nạp\n\n```toml\nbỏ dấu = \"old\"\n```"),
            "{text}"
        );
        assert!(
            text.ends_with("#### Dòng log cuối\n\n```text\na\nb\n```\n"),
            "{text}"
        );
    }
}
