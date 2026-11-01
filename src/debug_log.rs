//! Optional append-only debug log for live diagnosis.
//!
//! Off by default; turned on by `debug_log = true` in `config.toml` (the GUI
//! exposes it as a checkbox). When enabled, the engine appends one terse line
//! per focus change, capability update, surrounding-text report and
//! delete/injection decision to `~/.cache/ibus-mangrs/debug.log`. Timestamps are
//! milliseconds since the epoch so entries can be correlated with
//! `journalctl` (mutter warnings, crashes).

use std::fs::{File, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static ENABLED: AtomicBool = AtomicBool::new(false);
/// The opened log file, kept for the process lifetime: opening (plus
/// create_dir_all) per line put several syscalls of overhead on every
/// keystroke — exactly when latency was being diagnosed.
static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
#[cfg(test)]
static PATH_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);

/// Enable or disable logging (mirrors `Config::debug_log`).
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Append one line to the debug log. No-op unless enabled; never panics —
/// a logging failure must never take the input engine down.
pub fn log(msg: &str) {
    if !ENABLED.load(Ordering::Relaxed) {
        return;
    }
    let mut handle = LOG_FILE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if handle.is_none() {
        // Retried per line until it succeeds, so permissions fixed
        // mid-session are picked up; a handle on a since-deleted file keeps
        // writing to the unlinked inode, which is fine for diagnosis.
        *handle = open_log_file();
    }
    let Some(file) = handle.as_mut() else { return };
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = writeln!(file, "{ms} {msg}");
}

fn open_log_file() -> Option<File> {
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        // the cache dir may not exist (cleaned ~/.cache); without it the
        // append fails silently and diagnosis loses the log
        let _ = std::fs::create_dir_all(parent);
    }
    OpenOptions::new().create(true).append(true).open(path).ok()
}

fn log_path() -> Option<PathBuf> {
    #[cfg(test)]
    if let Some(path) = PATH_OVERRIDE
        .lock()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
    {
        return Some(path);
    }
    dirs::cache_dir().map(|d| d.join("ibus-mangrs").join("debug.log"))
}

/// Truncate a string for logging (surrounding text can be a whole paragraph).
pub fn brief(s: &str) -> std::borrow::Cow<'_, str> {
    if s.chars().count() <= 24 {
        return std::borrow::Cow::Borrowed(s);
    }
    std::borrow::Cow::Owned(format!("{}…", s.chars().take(24).collect::<String>()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disabled_by_default_and_never_panics() {
        set_enabled(false);
        log("no-op");
        assert!(!ENABLED.load(Ordering::Relaxed));
    }

    #[test]
    fn enabled_log_appends_to_one_handle() {
        let dir = std::env::temp_dir().join(format!("ibus-mangrs-log-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("debug.log");
        let _ = std::fs::remove_file(&path);
        *PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
        set_enabled(true);
        log("first");
        log("second");
        set_enabled(false);
        log("no-op");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(text.contains(" first"), "{text}");
        assert!(text.contains(" second"), "{text}");
        assert!(
            !text.contains("no-op"),
            "disabled lines must not land: {text}"
        );
        // Reset the globals: other tests share this process.
        *LOG_FILE.lock().unwrap() = None;
        *PATH_OVERRIDE.lock().unwrap() = None;
        ENABLED.store(false, Ordering::Relaxed);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn brief_truncates_long_text() {
        assert_eq!(brief("tiếng"), "tiếng");
        assert_eq!(
            brief("0123456789012345678901234567"),
            "012345678901234567890123…"
        );
    }
}
