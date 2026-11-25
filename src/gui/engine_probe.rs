//! What can be learned about the running engine from outside it: which process
//! is typing, which binary that process actually runs, and which engine IBus
//! has selected. The settings window and the engine are separate processes, so
//! everything here reads `/proc` or asks IBus — never the engine itself.

use std::path::PathBuf;
use std::time::{Duration, SystemTime};

/// The engine instance the desktop is using, or `None` when none runs.
#[derive(Clone)]
pub(super) struct EngineStatus {
    pub pid: u32,
    /// The binary this process was started from. `/proc/<pid>/comm` truncates
    /// at 15 bytes, so the executable name is the only trustworthy match.
    pub exe: PathBuf,
    /// An upgrade replaced the binary after this process started (or deleted
    /// it under it): the code that is typing is the old one, and only an IBus
    /// restart picks up the new.
    pub stale: bool,
    pub started: Option<SystemTime>,
    /// The engine IBus reports as current, if `ibus` answered.
    pub selected: Option<String>,
}

pub(super) const ENGINE_EXE: &str = "ibus-engine-mangrs";
pub(super) const ENGINE_NAME: &str = "ibus-mangrs";

/// Kernels build with 100 clock ticks per second unless configured otherwise;
/// `/proc/<pid>/stat` counts process start in those ticks.
const CLOCK_TICKS: f64 = 100.0;

pub(super) fn probe() -> Option<EngineStatus> {
    let (pid, exe, deleted) = find_process(ENGINE_EXE)?;
    let started = process_started(pid);
    let replaced = exe
        .metadata()
        .ok()
        .and_then(|m| m.modified().ok())
        .zip(started)
        .is_some_and(|(written, began)| written > began);
    Some(EngineStatus {
        pid,
        exe,
        stale: deleted || replaced,
        started,
        selected: ibus(&["engine"]),
    })
}

/// The first process whose executable is called `exe_name`, with the flag for
/// an image the kernel reports as deleted.
pub(super) fn find_process(exe_name: &str) -> Option<(u32, PathBuf, bool)> {
    for entry in std::fs::read_dir("/proc").ok()?.flatten() {
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        let Ok(link) = std::fs::read_link(entry.path().join("exe")) else {
            continue;
        };
        let shown = link.to_string_lossy();
        let deleted = shown.ends_with(" (deleted)");
        if link.file_name().and_then(|n| n.to_str()) != Some(exe_name)
            && !shown.contains(&format!("/{exe_name} (deleted)"))
        {
            continue;
        }
        let exe = PathBuf::from(shown.trim_end_matches(" (deleted)"));
        return Some((pid, exe, deleted));
    }
    None
}

/// When the process started, from `/proc/<pid>/stat`'s start tick and the
/// system uptime.
pub(super) fn process_started(pid: u32) -> Option<SystemTime> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    // The command name is in parentheses and may hold spaces: count fields
    // after the closing one. Field 22 is the start tick, so it is index 19.
    let after = stat.rsplit_once(')')?.1;
    let start_ticks: f64 = after.split_whitespace().nth(19)?.parse().ok()?;
    let uptime: f64 = std::fs::read_to_string("/proc/uptime")
        .ok()?
        .split_whitespace()
        .next()?
        .parse()
        .ok()?;
    let age = (uptime - start_ticks / CLOCK_TICKS).max(0.0);
    Some(SystemTime::now() - Duration::from_secs_f64(age))
}

/// One `ibus` subcommand's stdout, trimmed. `None` when the tool or the daemon
/// does not answer — the rows say so instead of showing an empty value.
fn ibus(args: &[&str]) -> Option<String> {
    let out = std::process::Command::new("ibus")
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout).trim().to_string();
    (!text.is_empty()).then_some(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_process_scan_finds_this_test_binary() {
        let me = std::env::current_exe().expect("test binary path");
        let name = me.file_name().expect("name").to_string_lossy().to_string();
        let (pid, exe, deleted) = find_process(&name).expect("the scan finds this process");
        assert!(pid > 0);
        assert_eq!(exe.file_name(), me.file_name());
        assert!(!deleted, "a running test binary is not a deleted image");
    }

    #[test]
    fn the_start_time_of_this_process_is_in_the_past() {
        let began = process_started(std::process::id()).expect("own start time");
        let age = SystemTime::now()
            .duration_since(began)
            .expect("not the future");
        assert!(
            age < Duration::from_secs(3600),
            "age {age:?} is implausible"
        );
    }

    #[test]
    fn a_name_that_does_not_run_is_none() {
        assert!(find_process("ibus-engine-mangrs-does-not-run").is_none());
    }
}
