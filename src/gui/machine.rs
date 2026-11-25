//! What this machine is: distribution, kernel, desktop and the IBus daemon's
//! version. The debug page lists them and the copied report carries them, so an
//! issue arrives with the environment it happened in instead of a round trip
//! asking for it.

use std::process::Command;

/// The four answers, empty when they could not be read.
#[derive(Clone, Default, PartialEq, Eq, Debug)]
pub(super) struct Machine {
    pub os: String,
    pub kernel: String,
    pub desktop: String,
    pub ibus: String,
}

/// Reads files, the environment and asks two programs: call it off the UI
/// thread, the same way the other diagnostics that need another process are.
pub(super) fn collect() -> Machine {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default();
    Machine {
        os: std::fs::read_to_string("/etc/os-release")
            .ok()
            .as_deref()
            .and_then(os_from_release)
            .unwrap_or_else(|| "?".to_string()),
        kernel: std::fs::read_to_string("/proc/sys/kernel/osrelease")
            .map(|s| s.trim().to_string())
            .ok()
            .filter(|s| !s.is_empty())
            .unwrap_or_else(|| "?".to_string()),
        desktop: desktop_label(&desktop, desktop_version(&desktop)),
        ibus: command_version(&["ibus", "version"])
            .map(|text| version_from_ibus(&text).unwrap_or(text))
            .unwrap_or_else(|| "?".to_string()),
    }
}

/// "Fedora Linux 41" from `/etc/os-release`: the pretty name when the
/// distribution ships one, otherwise the name and the version together.
fn os_from_release(text: &str) -> Option<String> {
    let value = |key: &str| {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{key}=")))
            .map(|v| v.trim().trim_matches('"').to_string())
            .filter(|v| !v.is_empty())
    };
    if let Some(pretty) = value("PRETTY_NAME") {
        return Some(pretty);
    }
    match (value("NAME"), value("VERSION_ID")) {
        (Some(name), Some(version)) => Some(format!("{name} {version}")),
        (Some(name), None) => Some(name),
        _ => None,
    }
}

/// "GNOME 47.1" or "GNOME" when the desktop does not print a version. The
/// session type rides along: on Wayland and on X11 the same desktop behaves
/// differently, and that is the first thing a bug report needs.
fn desktop_label(current: &str, version: Option<String>) -> String {
    let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
    let name = current.split(':').next().unwrap_or("").trim().to_string();
    if name.is_empty() {
        return if session.is_empty() {
            "?".to_string()
        } else {
            session
        };
    }
    let mut label = match version {
        Some(version) => format!("{name} {version}"),
        None => name,
    };
    if !session.is_empty() {
        label.push_str(&format!(" · {session}"));
    }
    label
}

/// The desktop's own version, asked from the program that owns it. An unknown
/// desktop simply has none.
fn desktop_version(current: &str) -> Option<String> {
    let current = current.to_ascii_uppercase();
    let text = if current.contains("GNOME") {
        command_version(&["gnome-shell", "--version"])?
    } else if current.contains("KDE") {
        command_version(&["plasmashell", "--version"])?
    } else {
        return None;
    };
    // "GNOME Shell 47.1" and "plasmashell 6.2.3" both end in the number.
    let version = text.split_whitespace().last()?.trim();
    let looks_like_version =
        version.starts_with(|c: char| c.is_ascii_digit()) && version.chars().any(|c| c == '.');
    looks_like_version.then(|| version.to_string())
}

/// "IBus 1.5.35" -> "1.5.35": the row already says IBus.
fn version_from_ibus(text: &str) -> Option<String> {
    let version = text.split_whitespace().last()?.trim();
    let looks_like_version =
        version.starts_with(|c: char| c.is_ascii_digit()) && version.chars().any(|c| c == '.');
    looks_like_version.then(|| version.to_string())
}

fn command_version(args: &[&str]) -> Option<String> {
    let (program, rest) = args.split_first()?;
    let output = Command::new(program).args(rest).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_os_name_prefers_the_pretty_one() {
        let release = "NAME=\"Fedora Linux\"\nVERSION_ID=41\nPRETTY_NAME=\"Fedora Linux 41 (Workstation Edition)\"\n";
        assert_eq!(
            os_from_release(release).as_deref(),
            Some("Fedora Linux 41 (Workstation Edition)")
        );
    }

    #[test]
    fn the_os_name_falls_back_to_name_and_version() {
        let release = "NAME=\"Arch Linux\"\nVERSION_ID=20241101\n";
        assert_eq!(
            os_from_release(release).as_deref(),
            Some("Arch Linux 20241101")
        );
        let rolled = "NAME=\"Arch Linux\"\n";
        assert_eq!(os_from_release(rolled).as_deref(), Some("Arch Linux"));
        assert_eq!(os_from_release(""), None);
    }

    #[test]
    fn a_desktop_label_carries_the_version_and_the_session() {
        assert_eq!(desktop_label("GNOME", Some("47.1".into())), {
            let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
            if session.is_empty() {
                "GNOME 47.1".to_string()
            } else {
                format!("GNOME 47.1 · {session}")
            }
        });
        assert!(
            !desktop_label("", None).is_empty(),
            "an unknown desktop still says something"
        );
        // A colon-separated list is common: take the first name only.
        assert!(desktop_label("ubuntu:GNOME", None).starts_with("ubuntu"));
    }

    #[test]
    fn only_a_number_counts_as_a_version() {
        assert_eq!(version_from_ibus("IBus 1.5.35").as_deref(), Some("1.5.35"));
        assert_eq!(version_from_ibus("IBus").as_deref(), None);
        assert_eq!(version_from_ibus("").as_deref(), None);
        assert_eq!(desktop_version("XFCE"), None);
    }

    #[test]
    fn collect_reports_every_field() {
        let machine = collect();
        assert!(!machine.os.is_empty(), "the distribution is named");
        assert!(!machine.kernel.is_empty(), "the kernel is named");
        assert!(!machine.desktop.is_empty(), "the desktop is named");
        assert!(!machine.ibus.is_empty(), "the IBus version is named");
    }
}
