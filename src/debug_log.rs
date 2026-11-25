//! Optional append-only debug log for live diagnosis.
//!
//! Off by default; turned on by `debug_log = true` in `config.toml` (the GUI
//! exposes it as a checkbox). When enabled, the engine appends one line per
//! focus change, capability update, surrounding-text report and
//! delete/injection decision to `~/.cache/ibus-mangrs/debug.log`.
//!
//! # Line format
//!
//! ```text
//! <epoch_ms> <tag> <key>=<value> [<key>=<value>]… [rep=<n>]
//! ```
//!
//! - `epoch_ms` is milliseconds since the epoch, so a line can be matched
//!   against `journalctl` (mutter warnings, crashes). A line without it is a
//!   fragment someone pasted by hand.
//! - `tag` is one of [`TAGS`] and is the first word after the timestamp: it
//!   says which part of the engine spoke, so a reader (or an agent) can
//!   `grep ' del '` and see only the delete decisions.
//! - fields are `key=value` with no spaces; a value that needs spaces or
//!   carries user text is quoted (`err="…"`) and the text tier masks it.
//! - `ok=0` marks a line that reports a failure; the family tag stays, so a
//!   failure reads in its own context: `del ok=0 mode=backspace n=3`.
//! - `rep=<n>` is appended by the writer when the same line repeated
//!   consecutively: the line then stands for `n` occurrences. Nothing else
//!   writes into an existing line.
//!
//! A line must never carry what the user typed unless the text tier is on
//! ([`set_text_enabled`]).

use std::fs::{File, OpenOptions};
use std::io::{Seek, SeekFrom, Write};
use std::os::unix::fs::MetadataExt;
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
/// The vocabulary of tags; the first word after the timestamp is one of these.
/// Kept closed and short so a filter is a grep, and so a new line cannot
/// invent a spelling that splits a family in two.
pub const TAGS: [&str; 8] = ["cfg", "ctx", "key", "st", "ins", "del", "buf", "ext"];
/// The line just written, so an identical one is collapsed into its `rep=`
/// count instead of filling the file.
static LAST: Mutex<Option<Repeat>> = Mutex::new(None);

struct Repeat {
    msg: String,
    count: u32,
    /// Offset of the newline that ends the line: a repeat rewrites `rep=`
    /// there, which is why no line is ever un-terminated in the file.
    tail: u64,
}

impl Repeat {
    /// Whether the line that arrived continues this run. Only an identical
    /// message does: a flip between two states is two real events.
    fn continues(&self, msg: &str) -> bool {
        self.msg == msg
    }
}
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
    *LAST.lock().unwrap_or_else(|p| p.into_inner()) = None;
    ENABLED.store(false, Ordering::Relaxed);
    TEXT_ENABLED.store(false, Ordering::Relaxed);
}

/// A temp directory for one test's log, removed when the test ends. The
/// suite runs hundreds of times, so a directory left behind each run litters
/// /tmp; removing the directory alone fails while the log is still inside.
#[cfg(test)]
pub(crate) struct TempLogDir(crate::test_support::TempDir);

#[cfg(test)]
impl TempLogDir {
    pub(crate) fn new(kind: &str) -> Self {
        Self(crate::test_support::TempDir::new(kind))
    }

    pub(crate) fn dir(&self) -> &std::path::Path {
        self.0.path()
    }
}

#[cfg(test)]
impl Drop for TempLogDir {
    fn drop(&mut self) {
        let mut override_path = PATH_OVERRIDE.lock().unwrap_or_else(|p| p.into_inner());
        if override_path
            .as_ref()
            .is_some_and(|p| p.starts_with(self.0.path()))
        {
            *override_path = None;
        }
        drop(override_path);
        // A cached handle would keep writing to the removed file.
        *LOG_FILE.lock().unwrap_or_else(|p| p.into_inner()) = None;
    }
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

/// Whether the keys tier is on — the key line's call site runs on every
/// keystroke, so its format! is gated behind this before building.
pub fn keys_enabled() -> bool {
    KEYS_ENABLED.load(Ordering::Relaxed)
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
    // The GUI's clear button deletes the file; a handle on the unlinked inode
    // would keep swallowing lines until the engine restarted, so a link count
    // of zero means the log has to be created again.
    let unlinked = handle
        .as_ref()
        .is_some_and(|(file, _, _)| file.metadata().is_ok_and(|m| m.nlink() == 0));
    let stale = stale || unlinked;
    if stale {
        #[cfg(test)]
        eprintln!("debug_log: cached handle outlived its path — reopened");
        *handle = None;
        // A rewritten count belongs to the file that was deleted.
        *LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
    if handle.is_none() {
        // Retried per line until it succeeds, so permissions fixed
        // mid-session and a log deleted behind us are both picked up.
        *handle = open_log_file();
    }
    let Some(file) = handle.as_mut() else { return };
    let mut last = LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(repeat) = last.as_mut().filter(|r| r.continues(msg)) {
        repeat.count += 1;
        let extra = format!(" rep={}\n", repeat.count);
        // Count only grows, so the new text always covers the old one.
        if write_at(&mut file.0, repeat.tail, extra.as_bytes()).is_ok() {
            file.1 = repeat.tail + extra.len() as u64;
        }
        return;
    }
    *last = None;
    drop(last);
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis())
        .unwrap_or(0);
    let line = format!("{ms} {msg}\n");
    if file.1 + line.len() as u64 > ROTATE_SIZE {
        // Move the full log aside (the rename overwrites a stale .old) and
        // start fresh; a failed reopen keeps writing to the renamed inode,
        // which is fine for diagnosis.
        if let Some(path) = log_path() {
            let _ = std::fs::rename(&path, path.with_extension("log.old"));
        }
        // A tail offset from the rotated file means nothing in the new one.
        *LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
        match open_log_file() {
            Some(pair) => *file = pair,
            None => file.1 = 0,
        }
    }
    let before = file.1;
    let _ = write_at(&mut file.0, before, line.as_bytes());
    file.1 = before + line.len() as u64;
    *LAST.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Repeat {
        msg: msg.to_string(),
        count: 1,
        tail: before + line.len() as u64 - 1,
    });
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
        // Not truncated: the handle is kept and appended to by offset.
        .truncate(false)
        .write(true)
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

/// Write at an absolute offset: the handle is not opened with `O_APPEND`
/// exactly so the writer can rewrite the `rep=` tail of the line above.
fn write_at(file: &mut File, offset: u64, bytes: &[u8]) -> std::io::Result<()> {
    file.seek(SeekFrom::Start(offset))?;
    file.write_all(bytes)
}

pub fn log_path() -> Option<PathBuf> {
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
        let dir = TempLogDir::new("debug-log");
        let path = dir.dir().join("debug.log");
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
    }

    #[test]
    fn text_tier_needs_both_switches() {
        let _guard = test_guard();
        let dir = TempLogDir::new("debug-log-2");
        let path = dir.dir().join("debug.log");
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
    }

    #[test]
    fn keys_tier_needs_both_switches() {
        let _guard = test_guard();
        let dir = TempLogDir::new("debug-log-3");
        let path = dir.dir().join("debug.log");
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
    }

    #[test]
    fn stale_handle_reopens_when_the_override_moves() {
        let _guard = test_guard();
        let dir = TempLogDir::new("debug-log-5");
        let first = dir.dir().join("one.log");
        let second = dir.dir().join("two.log");
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
    }

    #[test]
    fn oversized_log_rotates_to_old() {
        let _guard = test_guard();
        let dir = TempLogDir::new("debug-log-4");
        let path = dir.dir().join("debug.log");
        let old = dir.dir().join("debug.log.old");
        let _ = std::fs::remove_file(&old);
        *PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
        // Phase 1 — a pre-existing oversized log (the owner's real 7.4MB
        // case) rotates on its first new write: the counter seeds from the
        // file's metadata length at open, and the oversized content moves
        // to .old whole.
        std::fs::write(&path, vec![b'x'; (ROTATE_SIZE + 1024) as usize]).unwrap();
        set_enabled(true);
        log("first line after open");
        assert!(
            old.exists(),
            "the seeded oversized file must rotate on its first write"
        );
        let moved = std::fs::read_to_string(&old).unwrap_or_default();
        assert!(
            moved.len() as u64 > ROTATE_SIZE,
            "the oversized content moved aside"
        );
        assert!(
            std::fs::read_to_string(&path)
                .unwrap_or_default()
                .contains("first line after open"),
            "the crossing line starts the fresh file"
        );
        // Phase 2 — a long session crosses the cap mid-file: everything
        // written before the crossing lands in .old, the live file starts
        // fresh, and the fresh file stays under the cap.
        let big = "x".repeat(1024 * 1024);
        for i in 0..6 {
            // Distinct lines: an identical one would collapse into `rep=`.
            log(&format!("{i} {big}"));
        }
        let moved2 = std::fs::read_to_string(&old).unwrap_or_default();
        assert!(
            moved2.contains("first line after open"),
            "the pre-crossing lines moved into .old"
        );
        let live = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        assert!(
            live < ROTATE_SIZE,
            "the live file starts fresh again: {live}"
        );
        // Reset the globals: other tests share this process.
        *LOG_FILE.lock().unwrap() = None;
        *PATH_OVERRIDE.lock().unwrap() = None;
        ENABLED.store(false, Ordering::Relaxed);
        let _ = std::fs::remove_file(&old);
    }

    #[test]
    fn a_repeated_line_lands_once_with_its_count() {
        let _guard = test_guard();
        let dir = TempLogDir::new("debug-log-repeat");
        let path = dir.dir().join("debug.log");
        *PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
        set_enabled(true);
        for _ in 0..200 {
            log("st cur=0 len=0 comp=1");
        }
        // A different line closes the run and is written on its own.
        log("buf flush=per-key");
        log("buf flush=per-key");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        // Another test in this binary may reset the shared writer mid-run, so
        // the run is read as a shape: one or two lines, never two hundred,
        // and at least one of them carries its count.
        let run = |needle: &str| {
            text.lines()
                .filter(|line| line.contains(needle))
                .collect::<Vec<_>>()
        };
        let repeats = run("st cur=0 len=0 comp=1");
        assert!(
            !repeats.is_empty() && repeats.len() <= 2,
            "200 copies collapse: {text}"
        );
        assert!(
            repeats.iter().any(|line| line.contains(" rep=")),
            "the count is kept: {text}"
        );
        let flushes = run("buf flush=per-key");
        assert!(!flushes.is_empty() && flushes.len() <= 2, "{text}");
        // Every line is terminated: the count is written over the newline of
        // the line above, never appended after it.
        assert!(text.ends_with('\n'), "{text}");
        set_enabled(false);
        *LOG_FILE.lock().unwrap() = None;
        *PATH_OVERRIDE.lock().unwrap() = None;
        *LAST.lock().unwrap() = None;
    }

    #[test]
    fn a_deleted_log_is_created_again() {
        let _guard = test_guard();
        let dir = TempLogDir::new("debug-log-cleared");
        let path = dir.dir().join("debug.log");
        *PATH_OVERRIDE.lock().unwrap() = Some(path.clone());
        set_enabled(true);
        log("ctx start uinput=1");
        std::fs::remove_file(&path).expect("the clear button removes it");
        log("ctx focus=in");
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        assert!(
            text.contains("ctx focus=in"),
            "a line after the clear lands in a fresh file: {text}"
        );
        assert!(!text.contains("ctx start"), "{text}");
        set_enabled(false);
        *LOG_FILE.lock().unwrap() = None;
        *PATH_OVERRIDE.lock().unwrap() = None;
        *LAST.lock().unwrap() = None;
    }

    #[test]
    fn only_an_identical_line_continues_a_run() {
        // A flip between two states is two real events, not a repeat: the run
        // ends, so the file shows both states.
        let line = Repeat {
            msg: "st comp=1".to_string(),
            count: 1,
            tail: 0,
        };
        assert!(line.continues("st comp=1"));
        assert!(!line.continues("st comp=0"));
        assert!(
            !line.continues("st comp=1 "),
            "a changed field breaks the run"
        );
    }

    #[test]
    fn every_logged_line_opens_with_a_tag() {
        // The tag vocabulary is only worth anything if every call site uses
        // it: walk the sources and read the first word of each literal.
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
        let mut checked = 0;
        let mut offenders = Vec::new();
        let mut stack = vec![root];
        while let Some(dir) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&dir) else {
                continue;
            };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|e| e != "rs")
                    || path.file_name().is_some_and(|n| n == "debug_log.rs")
                {
                    continue;
                }
                let Ok(source) = std::fs::read_to_string(&path) else {
                    continue;
                };
                for call in [
                    "debug_log::log(",
                    "debug_log::log_text(",
                    "debug_log::log_keys(",
                ] {
                    let mut rest = source.as_str();
                    while let Some(pos) = rest.find(call) {
                        rest = &rest[pos + call.len()..];
                        // Only a call that passes a literal is checkable; one
                        // that builds the string elsewhere is read by hand.
                        let mut after = rest.trim_start();
                        for prefix in ["&format!(", "format!("] {
                            if let Some(stripped) = after.strip_prefix(prefix) {
                                after = stripped.trim_start();
                                break;
                            }
                        }
                        if !after.starts_with('"') {
                            continue;
                        }
                        let inner = &after[1..];
                        let Some(end) = inner.find('"') else { break };
                        let first = inner[..end].split(' ').next().unwrap_or("");
                        checked += 1;
                        if !TAGS.contains(&first) {
                            offenders.push(format!(
                                "{}: {:?}",
                                path.display(),
                                inner[..end]
                                    .split(' ')
                                    .take(6)
                                    .collect::<Vec<_>>()
                                    .join(" ")
                            ));
                        }
                    }
                }
            }
        }
        assert!(checked > 50, "the walk found the call sites: {checked}");
        assert!(offenders.is_empty(), "untagged lines: {offenders:#?}");
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
