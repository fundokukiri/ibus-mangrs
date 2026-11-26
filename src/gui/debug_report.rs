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
    /// Whether the copied report carries this row. A row that always says the
    /// same thing on a working install only pads the issue.
    pub report: bool,
    /// Whether the column shows it. The version is the one fact the report
    /// needs and the window already spells out in the status bar.
    pub screen: bool,
}

impl Cell {
    fn info(label: &str, value: String) -> Cell {
        Cell {
            label: label.to_string(),
            value,
            status: Status::Info,
            hint: None,
            open_dir: None,
            report: true,
            screen: true,
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
            report: true,
            screen: true,
        }
    }

    /// Carried in the report, not drawn in the column.
    fn report_only(mut self) -> Cell {
        self.screen = false;
        self
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
    /// See `Cell::report`.
    pub report: bool,
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
    // The column leaves these two to the status bar; the report keeps them,
    // because an issue read on another machine has neither.
    let selected = status
        .and_then(|s| s.selected.clone())
        .unwrap_or_else(|| "?".to_string());
    let selected_status = match status {
        None => Status::Warn,
        Some(_) if selected == crate::gui::engine_probe::ENGINE_NAME => Status::Ok,
        Some(_) => Status::Info,
    };
    system.push(
        Cell::info(&t("rows.debug_engine.selected", "Engine"), selected)
            .with_status(
                selected_status,
                &t(
                    "rows.debug_engine.selected_hint",
                    "IBus đang chạy engine khác. Chọn MăngRS trong menu bộ gõ.",
                ),
            )
            .report_only(),
    );
    let version = format!("v{}", env!("CARGO_PKG_VERSION"));
    let install = match status {
        Some(s) if s.stale => {
            Cell::info(&t("rows.debug_engine.version", "Phiên bản MăngRS"), version).with_status(
                Status::Warn,
                &t(
                    "rows.debug_engine.stale_hint",
                    "Bản cài mới hơn tiến trình đang chạy. Khởi động lại IBus để dùng bản mới.",
                ),
            )
        }
        // The tooltip names the binary that is actually typing: the only way
        // to tell two installs apart without opening a terminal.
        Some(s) => Cell::info(&t("rows.debug_engine.version", "Phiên bản MăngRS"), version)
            .with_status(Status::Ok, &s.exe.display().to_string()),
        None => Cell::info(&t("rows.debug_engine.version", "Phiên bản MăngRS"), version),
    };
    system.push(install.report_only());
    // The IBus daemon's own version belongs with the two rows about IBus.
    system.push(Cell::info(
        &t("rows.debug_machine.ibus", "Phiên bản IBus"),
        facts.machine.ibus.clone(),
    ));
    let (ext_key, ext_fallback, ext_running) = facts.extension;
    let extension = Cell::info(
        &t("rows.debug_info.extension", "Extension"),
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
    // From this install down to the machine it sits on, ending with the session
    // it was typed in: the order the facts are usually asked in.
    for (label_key, label_fallback, value) in [
        (
            "rows.debug_machine.os",
            "Hệ điều hành",
            facts.machine.os.clone(),
        ),
        (
            "rows.debug_machine.kernel",
            "Kernel",
            facts.machine.kernel.clone(),
        ),
        (
            "rows.debug_machine.desktop",
            "DE/WM",
            facts.machine.desktop.clone(),
        ),
    ] {
        system.push(Cell::info(&t(label_key, label_fallback), value));
    }
    system.push(Cell::info(
        &t("rows.debug_info.session", "Session"),
        std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".to_string()),
    ));
    out.push(Section {
        title: t("misc.section_system", "Hệ thống"),
        rows: system,
        report: true,
    });

    let mut insert = Vec::new();
    insert.push(Cell::info(
        &t("rows.debug_info.mode", "Chế độ gõ toàn cục"),
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
        &t("rows.debug_info.backend", "Backend"),
        configured.to_string(),
    );
    insert.push(if auto_fallback {
        configured_cell.with_status(Status::Warn, "")
    } else {
        configured_cell
    });
    let actual = Cell::info(&t("rows.debug_transport.title", "Backend thực tế"), shown);
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
    // The mode is global and the backend has per-app rules, so a pasted copy of
    // these three rows tells a maintainer nothing they cannot infer.
    out.push(Section {
        title: t("misc.section_insert", "Cách chèn"),
        rows: insert,
        report: false,
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
    out.push(Section {
        title: t("misc.section_files", "Tệp"),
        rows: files,
        report: false,
    });
    out
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
    // Markdown, because the report's destination is a GitHub issue. Only what a
    // maintainer cannot infer travels: the title line, the engine's own name,
    // the insertion rows (the mode is global and the backend has per-app rules)
    // and the two paths are left out on purpose.
    let mut out = format!(
        "### {}\n\n{}\n",
        t("debug_report.repro_section", "Cách tái hiện"),
        t(
            "debug_report.repro_body",
            "- Ứng dụng:\n- Chế độ gõ:\n- Các bước:\n  1. \n  2. \n- Chữ đã gõ:\n- Mong đợi:\n- Thực tế:"
        )
    );
    for section in sections.iter().filter(|section| section.report) {
        let rows: Vec<&Cell> = section.rows.iter().filter(|cell| cell.report).collect();
        if rows.is_empty() {
            continue;
        }
        out.push_str(&format!("\n### {}\n\n", section.title));
        for cell in rows {
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
            "\n### {}\n\n```toml\n{}\n```\n",
            t("debug_report.config_section", "config.toml đang nạp"),
            config.join("\n")
        ));
    }
    if !log.is_empty() {
        out.push_str(&format!(
            "\n### {}\n\n```text\n{}\n```\n",
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

    /// Every row of every group, in reading order: the report leaves some out,
    /// so a test that asks about rows flattens them first.
    fn flat(sections: &[Section]) -> Vec<Cell> {
        sections
            .iter()
            .flat_map(|section| section.rows.iter().cloned())
            .collect()
    }

    fn lines(raw: &[&str]) -> Vec<String> {
        raw.iter().map(|s| s.to_string()).collect()
    }

    fn status(stale: bool) -> EngineStatus {
        EngineStatus {
            pid: 4242,
            exe: std::path::PathBuf::from("/usr/lib/ibus-mangrs/ibus-engine-mangrs"),
            stale,
            started: Some(std::time::SystemTime::now()),
            selected: Some("ibus-mangrs".to_string()),
        }
    }

    fn machine() -> &'static crate::gui::machine::Machine {
        static MACHINE: std::sync::OnceLock<crate::gui::machine::Machine> =
            std::sync::OnceLock::new();
        MACHINE.get_or_init(|| crate::gui::machine::Machine {
            os: "Arch Linux".to_string(),
            kernel: "6.12.1".to_string(),
            desktop: "GNOME 47.1".to_string(),
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
        assert_eq!(titles, ["Hệ thống", "Cách chèn", "Tệp"]);
        assert_eq!(
            groups[0].rows.len(),
            8,
            "four install rows and four for the machine"
        );
        assert_eq!(groups[1].rows.len(), 3);
        assert_eq!(groups[2].rows.len(), 2);
        assert!(!groups[1].report && !groups[2].report, "screen only");
    }

    #[test]
    fn a_stale_binary_asks_for_a_restart() {
        let missing = rows(None, None);
        let selected = missing
            .iter()
            .find(|c| c.label == "Engine")
            .expect("a selection row");
        assert_eq!(selected.status, Status::Warn, "no engine is a warning");

        let stale = rows(Some(&status(true)), None);
        let install = stale
            .iter()
            .find(|c| c.label == "Phiên bản MăngRS")
            .expect("an install row");
        assert_eq!(install.value, "v1.2.7", "the row names the version");
        assert_eq!(install.status, Status::Warn);
        assert!(install
            .hint
            .as_deref()
            .unwrap_or("")
            .contains("Khởi động lại IBus"));
        assert!(
            !stale.iter().any(|c| c.label == "Register"),
            "the registration row is gone: it says the same thing on every working install"
        );
    }

    #[test]
    fn auto_landing_on_a_real_backend_is_the_one_known_warning() {
        let boxes = rows(Some(&status(false)), Some("uinput"));
        let actual = boxes
            .iter()
            .find(|c| c.label == "Backend thực tế")
            .expect("a backend row");
        assert_eq!(actual.value, "uinput");
        assert_eq!(actual.status, Status::Warn, "{actual:?}");
        assert!(actual.hint.as_deref().unwrap_or("").contains("auto"));

        // A backend the user picked on purpose is not a warning.
        let explicit = flat(&sections(&facts(
            Some(&status(false)),
            Some("uinput"),
            crate::config::PreferredBackendConfig::Uinput,
        )))
        .into_iter()
        .find(|c| c.label == "Backend thực tế")
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
            !text.contains("config.toml"),
            "the paths stay on screen, not in the report: {text}"
        );
    }

    #[test]
    fn the_report_leaves_out_what_a_maintainer_can_infer() {
        let text = report(
            &sections(&facts(
                Some(&status(false)),
                Some("uinput"),
                crate::config::PreferredBackendConfig::Auto,
            )),
            &[],
            &[],
        );
        for absent in ["Cách chèn", "Tệp", "**Register**"] {
            assert!(!text.contains(absent), "{absent} is left out: {text}");
        }
        // Both rows the column hides still travel: an issue read on another
        // machine needs them.
        for present in [
            "**Engine**",
            "Phiên bản MăngRS",
            "Kernel",
            "DE/WM",
            "Phiên bản IBus",
        ] {
            assert!(text.contains(present), "{present} travels: {text}");
        }
    }

    #[test]
    fn the_report_carries_every_row_the_settings_and_the_log_tail() {
        let groups = vec![Section {
            title: "Hệ thống".to_string(),
            report: true,
            rows: vec![
                Cell::info("Engine", "đang chạy".to_string()).with_status(Status::Ok, ""),
                Cell::info("Đăng ký", "không thấy".to_string())
                    .with_status(Status::Warn, "cài lại"),
            ],
        }];
        let text = report(&groups, &lines(&["bỏ dấu = \"old\""]), &lines(&["a", "b"]));
        assert!(text.starts_with("### Cách tái hiện"), "{text}");
        assert!(
            text.contains("- Chế độ gõ:"),
            "the mode is asked for: {text}"
        );
        assert!(text.contains("\n### Hệ thống\n"), "{text}");
        assert!(
            text.contains("- Ứng dụng:\n- Chế độ gõ:\n- Các bước:"),
            "the report asks how to reproduce it: {text}"
        );
        assert!(text.contains("- **Engine**: `đang chạy`"), "{text}");
        assert!(
            text.contains("- **Đăng ký**: `không thấy` - cần xem\n  - _cài lại_"),
            "{text}"
        );
        assert!(
            text.contains("### config.toml đang nạp\n\n```toml\nbỏ dấu = \"old\"\n```"),
            "{text}"
        );
        assert!(
            text.ends_with("### Dòng log cuối\n\n```text\na\nb\n```\n"),
            "{text}"
        );
    }
}
