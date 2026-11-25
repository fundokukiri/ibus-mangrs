//! The "Chẩn đoán" block and the clipboard report: everything the page can say
//! about this install without asking the engine, in one place so the rows on
//! screen and the copied text cannot drift apart.

use super::*;
use crate::gui::engine_probe::EngineStatus;

/// The rows that never change while the window is open: build, session, mode,
/// backend, extension, both paths.
pub(super) fn diagnostic_info(ui: &Rc<Ui>, log: Option<&std::path::Path>) -> Vec<(String, String)> {
    let config = ui.read();
    vec![
        (
            t("rows.debug_info.version", "Phiên bản"),
            env!("CARGO_PKG_VERSION").to_string(),
        ),
        (
            t("rows.debug_info.session", "Phiên"),
            std::env::var("XDG_SESSION_TYPE").unwrap_or_else(|_| "?".to_string()),
        ),
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

/// What the running engine says about itself, read from outside it. Every row
/// answers a question that otherwise costs a terminal: is it running, is it the
/// code I just built, and does IBus know the component at all.
pub(super) fn engine_rows(status: Option<&EngineStatus>) -> Vec<(String, String)> {
    let Some(status) = status else {
        // Five rows either way, so the page can keep one row per slot and
        // refresh them in place.
        return vec![
            (
                t("rows.debug_engine.title", "Engine"),
                t("rows.debug_engine.not_running", "Không thấy tiến trình").to_string(),
            ),
            (
                t("rows.debug_engine.binary", "Tệp đang chạy"),
                "?".to_string(),
            ),
            (t("rows.debug_engine.fresh", "Bản cài"), "?".to_string()),
            (
                t("rows.debug_engine.selected", "IBus đang chọn"),
                "?".to_string(),
            ),
            (
                t("rows.debug_engine.registered", "Đăng ký"),
                "?".to_string(),
            ),
        ];
    };
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
    vec![
        (
            t("rows.debug_engine.title", "Engine"),
            format!(
                "{} · PID {} · {}",
                t("rows.debug_engine.running", "đang chạy"),
                status.pid,
                began
            ),
        ),
        (
            t("rows.debug_engine.binary", "Tệp đang chạy"),
            status.exe.display().to_string(),
        ),
        (
            t("rows.debug_engine.fresh", "Bản cài"),
            if status.stale {
                t(
                    "rows.debug_engine.stale",
                    "Mới hơn bản đang chạy, cần khởi động lại IBus",
                )
                .to_string()
            } else {
                t("rows.debug_engine.current", "Khớp bản đang chạy").to_string()
            },
        ),
        (
            t("rows.debug_engine.selected", "IBus đang chọn"),
            status.selected.clone().unwrap_or_else(|| "?".to_string()),
        ),
        (
            t("rows.debug_engine.registered", "Đăng ký"),
            match status.registered {
                Some(true) => t("rows.debug_engine.listed", "IBus có component").to_string(),
                Some(false) => t(
                    "rows.debug_engine.unregistered",
                    "IBus không thấy component, cài lại gói",
                )
                .to_string(),
                None => "?".to_string(),
            },
        ),
    ]
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

/// The saved settings that differ from the defaults, one `key = value` line
/// each: the report carries what the user actually runs, and nobody has to ask
/// which switches are on.
pub(super) fn non_default_config(config: &Config) -> Vec<String> {
    let Ok(now) = toml::Value::try_from(config) else {
        return Vec::new();
    };
    let Ok(defaults) = toml::Value::try_from(Config::default()) else {
        return Vec::new();
    };
    let (Some(now), Some(defaults)) = (now.as_table(), defaults.as_table()) else {
        return Vec::new();
    };
    let mut out: Vec<String> = now
        .iter()
        .filter(|(key, value)| defaults.get(*key) != Some(*value))
        .map(|(key, value)| {
            let shown = serde_json::to_string(value).unwrap_or_else(|_| "?".to_string());
            format!("{key} = {shown}")
        })
        .collect();
    // A rules list or a menu tree can be long; the report stays pasteable.
    out.truncate(25);
    out
}

/// The copied report: one headline line, the labelled rows, the settings that
/// differ from the defaults, then the log tail under a separator.
pub(super) fn report(info: &[(String, String)], config: &[String], log: &[String]) -> String {
    let mut out = format!("{} {}", t("app.name", "MăngRS"), env!("CARGO_PKG_VERSION"));
    for (label, value) in info {
        out.push_str(&format!("\n{label}: {value}"));
    }
    if !config.is_empty() {
        out.push_str(&format!(
            "\n--- {} ---",
            t("debug_report.config_section", "cấu hình khác mặc định")
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

    fn rows(raw: &[(&str, &str)]) -> Vec<(String, String)> {
        raw.iter()
            .map(|(l, v)| ((*l).to_string(), (*v).to_string()))
            .collect()
    }

    #[test]
    fn the_report_carries_the_version_the_settings_and_the_log_tail() {
        let text = report(
            &rows(&[("Chế độ gõ", "Gạch chân")]),
            &lines(&["bỏ dấu = \"old\""]),
            &lines(&["a", "b"]),
        );
        assert!(text.contains(env!("CARGO_PKG_VERSION")), "{text}");
        assert!(text.contains("Chế độ gõ: Gạch chân"), "{text}");
        assert!(
            text.contains("--- cấu hình khác mặc định ---\nbỏ dấu"),
            "{text}"
        );
        assert!(text.ends_with("--- log cuối ---\na\nb"), "{text}");
    }

    #[test]
    fn only_settings_off_their_defaults_are_reported() {
        let defaults = Config::default();
        assert!(
            non_default_config(&defaults).is_empty(),
            "a default config reports nothing: {:?}",
            non_default_config(&defaults)
        );
        let changed = Config {
            enabled: !defaults.enabled,
            ..defaults
        };
        let found = non_default_config(&changed);
        assert_eq!(found.len(), 1, "{found:?}");
        assert!(found[0].starts_with("enabled = "), "{found:?}");
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
}
