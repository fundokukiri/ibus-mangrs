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
/// Second tier: typed-text lines (surrounding-text content, verify diffs).
/// Only effective while `ENABLED` is also on — the default log stays free of
/// typed text so it can be attached to a bug report as-is.
static TEXT_ENABLED: AtomicBool = AtomicBool::new(false);
static KEYS_ENABLED: AtomicBool = AtomicBool::new(false);

/// Rotate `debug.log` to `debug.log.old` once the live file crosses this:
/// the disk stays bounded (~2x cap) instead of growing per session.
const ROTATE_SIZE: u64 = 5 * 1024 * 1024;
/// The opened log file, kept for the process lifetime: opening (plus
/// create_dir_all) per line put several syscalls of overhead on every
/// keystroke — exactly when latency was being diagnosed.
static LOG_FILE: Mutex<Option<(File, u64, PathBuf)>> = Mutex::new(None);
/// Test-only redirect so tests can assert log lines without touching the
/// user's cache dir; engine tests set it through `test_guard` below.
#[cfg(test)]
pub(crate) static PATH_OVERRIDE: Mutex<Option<PathBuf>> = Mutex::new(None);
/// Serializes every test that flips the process-wide log state (the
/// debug_log unit tests and the engine tests asserting log lines).
#[cfg(test)]
static TEST_LOCK: Mutex<()> = Mutex::new(());

/// Hold this while flipping `ENABLED`/`PATH_OVERRIDE` in a test: the
/// debug_log unit tests use the same lock, so the globals never race.
#[cfg(test)]
pub(crate) fn test_guard() -> std::sync::MutexGuard<'static, ()> {
    TEST_LOCK.lock().unwrap_or_else(|p| p.into_inner())
}

/// Restore the process-wide log state after a test that asserted log
/// lines, so later tests in the same binary start clean.
#[cfg(test)]
pub(crate) fn test_reset() {
    *LOG_FILE.lock().unwrap_or_else(|p| p.into_inner()) = None;
    *PATH_OVERRIDE.lock().unwrap_or_else(|p| p.into_inner()) = None;
    ENABLED.store(false, Ordering::Relaxed);
    TEXT_ENABLED.store(false, Ordering::Relaxed);
}

/// Enable or disable logging (mirrors `Config::debug_log`).
pub fn set_enabled(on: bool) {
    ENABLED.store(on, Ordering::Relaxed);
}

/// Enable or disable the typed-text tier (mirrors `Config::debug_log_text`).
pub fn set_text_enabled(on: bool) {
    TEXT_ENABLED.store(on, Ordering::Relaxed);
}

/// Enable or disable the key-event tier (mirrors `Config::debug_log_keys`).
pub fn set_keys_enabled(on: bool) {
    KEYS_ENABLED.store(on, Ordering::Relaxed);
}

/// Whether the decision log is on — call sites hot enough to build their
/// message only when it will be used check this first.
pub fn enabled() -> bool {
    ENABLED.load(Ordering::Relaxed)
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
    // The override is only consulted on open, so a cached handle whose
    // path has moved under it (a parallel test flipped PATH_OVERRIDE)
    // would swallow every line into the old inode — revalidate per line.
    let current = log_path();
    let stale = match (&*handle, &current) {
        (Some((_, _, opened)), Some(path)) => opened != path,
        (Some(_), None) => true,
        _ => false,
    };
    if stale {
        #[cfg(test)]
        eprintln!("debug_log: cached handle outlived its path — reopened");
        *handle = None;
    }
    if handle.is_none() {
        // Retried per line until it succeeds, so permissions fixed
        // mid-session are picked up; a handle on a since-deleted file keeps
        // writing to the unlinked inode, which is fine for diagnosis.
        *handle = open_log_file();
    }
    let Some(file) = handle.as_mut() else { return };
    let line_len = msg.len() as u64 + 15;
    if file.1 + line_len > ROTATE_SIZE {
        // Move the full log aside (the rename overwrites a stale .old) and
        // start fresh; a failed reopen keeps writing to the renamed inode,
        // which is fine for diagnosis.
        if let Some(path) = log_path() {
            let _ = std::fs::rename(&path, path.with_extension("log.old"));
        }
        match open_log_file() {
            Some(pair) => *file = pair,
            None => file.1 = 0,
        }
    }
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let _ = writeln!(file.0, "{ms} {msg}");
    file.1 += line_len;
}

/// Append one line that may carry typed text (the surrounding-text content,
/// the verify diffs). Requires BOTH the decision log and the text tier, so a
/// plain `debug_log = true` session never records what the user typed.
pub fn log_text(msg: &str) {
    if !TEXT_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    log(msg);
}

/// Append one per-keystroke line (keyval/state). Requires the master
/// decision switch AND the keys tier — the default log stays free of the
/// per-keystroke spam that once filled it.
pub fn log_keys(msg: &str) {
    if !KEYS_ENABLED.load(Ordering::Relaxed) {
        return;
    }
    log(msg);
}

fn open_log_file() -> Option<(File, u64, PathBuf)> {
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let path = log_path()?;
    if let Some(parent) = path.parent() {
        // the cache dir may not exist (cleaned ~/.cache); without it the
        // append fails silently and diagnosis loses the log
        let _ = std::fs::create_dir_all(parent);
    }
    let file = OpenOptions::new()
        .create(true)
        .append(true)
        // The typed-text tier writes what the user typed; the file must not be
        // readable by other local users (0666 minus the usual 022 umask).
        .mode(0o600)
        .open(&path)
        .ok()?;
    // Files written by older versions are 0644; tighten them on the next open.
    let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600));
    let len = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
    Some((file, len, path))
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
        let _guard = test_guard();
        set_enabled(false);
        log("no-op");
        assert!(!ENABLED.load(Ordering::Relaxed));
    }

    #[test]
    fn enabled_log_appends_to_one_handle() {
        let _guard = test_guard();
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
    fn text_tier_needs_both_switches() {
        let _guard = test_guard();
        let dir = std::env::temp_dir().join(format!("ibus-mangrs-logtest2-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("debug.log");
        let _ = std::fs::remove_file(&path);
        *PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
        set_enabled(true);
        set_text_enabled(false);
        log_text("text line while tier off");
        log("decision only");
        set_text_enabled(true);
        log_text("text line while tier on");
        set_enabled(false);
        set_text_enabled(true);
        log_text("text line while master off");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(text.contains("decision only"), "{text}");
        assert!(!text.contains("while tier off"), "{text}");
        assert!(text.contains("while tier on"), "{text}");
        assert!(!text.contains("while master off"), "{text}");
        // Reset the globals: other tests share this process.
        *LOG_FILE.lock().unwrap() = None;
        *PATH_OVERRIDE.lock().unwrap() = None;
        ENABLED.store(false, Ordering::Relaxed);
        TEXT_ENABLED.store(false, Ordering::Relaxed);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn keys_tier_needs_both_switches() {
        let _guard = test_guard();
        let dir = std::env::temp_dir().join(format!("ibus-mangrs-logtest3-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("debug.log");
        let _ = std::fs::remove_file(&path);
        *PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
        set_enabled(true);
        set_keys_enabled(false);
        log_keys("key line while tier off");
        log("decision only");
        set_keys_enabled(true);
        log_keys("key line while tier on");
        set_enabled(false);
        set_keys_enabled(true);
        log_keys("key line while master off");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(text.contains("decision only"), "{text}");
        assert!(!text.contains("while tier off"), "{text}");
        assert!(text.contains("while tier on"), "{text}");
        assert!(!text.contains("while master off"), "{text}");
        // Reset the globals: other tests share this process.
        *LOG_FILE.lock().unwrap() = None;
        *PATH_OVERRIDE.lock().unwrap() = None;
        ENABLED.store(false, Ordering::Relaxed);
        KEYS_ENABLED.store(false, Ordering::Relaxed);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn stale_handle_reopens_when_the_override_moves() {
        let _guard = test_guard();
        let dir = std::env::temp_dir().join(format!("ibus-mangrs-logtest5-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let first = dir.join("one.log");
        let second = dir.join("two.log");
        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
        *PATH_OVERRIDE.lock().unwrap() = Some(first.clone());
        set_enabled(true);
        log("to the first file");
        // A parallel test moved the override under us: the next line must
        // land in the NEW path, not in the cached handle's old inode.
        *PATH_OVERRIDE.lock().unwrap() = Some(second.clone());
        log("to the second file");
        let one = std::fs::read_to_string(&first).unwrap_or_default();
        assert!(one.contains("to the first file"), "{one}");
        let two = std::fs::read_to_string(&second).unwrap_or_default();
        assert!(two.contains("to the second file"), "{two}");
        // Reset the globals: other tests share this process.
        *LOG_FILE.lock().unwrap() = None;
        *PATH_OVERRIDE.lock().unwrap() = None;
        ENABLED.store(false, Ordering::Relaxed);
        let _ = std::fs::remove_file(&first);
        let _ = std::fs::remove_file(&second);
        let _ = std::fs::remove_dir(&dir);
    }

    #[test]
    fn oversized_log_rotates_to_old() {
        let _guard = test_guard();
        let dir = std::env::temp_dir().join(format!("ibus-mangrs-logtest4-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("debug.log");
        let old = dir.join("debug.log.old");
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&old);
        *PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
        set_enabled(true);
        log("marker-before-rotation");
        let big = "x".repeat(1024 * 1024);
        for _ in 0..6 {
            log(&big);
        }
        assert!(old.exists(), "the cap must move the full log aside");
        let moved = std::fs::read_to_string(&old).unwrap_or_default();
        assert!(moved.contains("marker-before-rotation"));
        let live = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        assert!(live < ROTATE_SIZE, "the live file starts fresh: {live}");
        // Reset the globals: other tests share this process.
        *LOG_FILE.lock().unwrap() = None;
        *PATH_OVERRIDE.lock().unwrap() = None;
        ENABLED.store(false, Ordering::Relaxed);
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(&old);
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
