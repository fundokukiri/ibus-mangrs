//! Optional append-only debug log for live diagnosis.
//!
//! Off by default; turned on by `debug_log = true` in `config.toml` (the GUI
//! exposes it as a checkbox). When enabled, the engine appends one terse line
//! per focus change, capability update, surrounding-text report and
//! delete/injection decision to `~/.cache/ibus-mangrs/debug.log`. Timestamps are
//! milliseconds since the epoch so entries can be correlated with
//! `journalctl` (mutter warnings, crashes).

use std::fs::OpenOptions;
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ENABLED: AtomicBool = AtomicBool::new(false);

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
    let Some(path) = log_path() else { return };
    if let Some(parent) = path.parent() {
        // the cache dir may not exist (cleaned ~/.cache); without it the
        // append fails silently and diagnosis loses the log
        let _ = std::fs::create_dir_all(parent);
    }
    let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) else {
        return;
    };
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = writeln!(file, "{ms} {msg}");
}

fn log_path() -> Option<PathBuf> {
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
    fn brief_truncates_long_text() {
        assert_eq!(brief("tiếng"), "tiếng");
        assert_eq!(brief("0123456789012345678901234567"), "012345678901234567890123…");
    }
}
