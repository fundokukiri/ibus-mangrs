//! Clipboard Unicode channel for the uinput modes.
//!
//! Vietnamese glyphs have no keycodes, so when `unicode_channel = "clipboard"`
//! the word travels as: snapshot the user's clipboard, set the clipboard to
//! the word, inject Ctrl+V through the virtual keyboard, restore the user's
//! clipboard once the app has read it. The restore is debounced: a burst of
//! typing only restores after the last paste + 600 ms (the pattern vietc's
//! `uinput_monitor.rs` uses), so the app never reads back a half-restored
//! clipboard.
//!
//! Wayland uses `wl-copy`/`wl-paste`, X11 `xclip`; whichever matches the
//! session wins.

use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// How long after the last paste the user's clipboard is restored.
const RESTORE_DELAY_MS: u64 = 600;

/// The clipboard content the user had before our first paste in a burst.
static ORIGINAL: Mutex<Option<String>> = Mutex::new(None);
/// Deadline (ms since epoch) when the restore should run; 0 = none pending.
static RESTORE_AT: AtomicU64 = AtomicU64::new(0);

fn session_tool() -> Option<(&'static str, bool)> {
    // (binary, is_x11)
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        return Some(("wl-copy", false));
    }
    if std::env::var_os("DISPLAY").is_some() {
        return Some(("xclip", true));
    }
    None
}

/// The clipboard tool this process would use (diagnostics).
pub fn tool() -> Option<&'static str> {
    session_tool().map(|(t, _)| t)
}

fn read_clipboard() -> Option<String> {
    let (_, is_x11) = session_tool()?;
    let mut cmd = if is_x11 {
        let mut c = Command::new("xclip");
        c.args(["-selection", "clipboard", "-o"]);
        c
    } else {
        Command::new("wl-paste")
    };
    let output =
        cmd.stdin(Stdio::null()).stderr(Stdio::null()).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn write_clipboard(text: &str) -> bool {
    let Some((binary, is_x11)) = session_tool() else { return false };
    let mut command = Command::new(binary);
    if is_x11 {
        command.args(["-selection", "clipboard"]);
    }
    command.stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    let mut child = match command.spawn() {
        Ok(c) => c,
        Err(_) => return false,
    };
    use std::io::Write;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(text.as_bytes());
    }
    child.wait().map(|s| s.success()).unwrap_or(false)
}

fn now_ms() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_millis() as u64
}

/// Put `text` on the clipboard (saving the user's content once per burst),
/// then call `ctrl_v` to paste it. The clipboard is restored in the
/// background after the debounce window. False when no clipboard tool exists
/// or the write failed — the server then falls back to CommitText.
pub fn paste_and_restore(text: &str, ctrl_v: &dyn Fn() -> bool) -> bool {
    if tool().is_none() {
        return false;
    }
    // Snapshot the user's clipboard once per burst (before our first write).
    {
        let mut original = ORIGINAL.lock().unwrap();
        if original.is_none() {
            *original = read_clipboard();
        }
    }
    if !write_clipboard(text) {
        return false;
    }
    if !ctrl_v() {
        return false;
    }
    schedule_restore();
    true
}

/// Arm the debounced restore and spawn a sleeper for it. One thread per
/// paste is slightly wasteful but simple; the sleepers converge on the
/// latest deadline and the first one past it performs the restore.
fn schedule_restore() {
    RESTORE_AT.store(now_ms() + RESTORE_DELAY_MS, Ordering::SeqCst);
    thread::spawn(|| loop {
        let deadline = RESTORE_AT.load(Ordering::SeqCst);
        let remaining = deadline.saturating_sub(now_ms());
        if remaining > 5 {
            thread::sleep(Duration::from_millis(remaining.min(50)));
            continue;
        }
        restore_now();
        break;
    });
}

fn restore_now() {
    let original = ORIGINAL.lock().unwrap().take();
    if let Some(text) = original {
        write_clipboard(&text);
    }
}

/// For tests: pretend the app read the clipboard and reset the burst state.
#[cfg(test)]
pub fn reset_for_tests() {
    ORIGINAL.lock().unwrap().take();
    RESTORE_AT.store(0, Ordering::SeqCst);
}

/// Milliseconds until the pending restore fires (0 when none).
#[cfg(test)]
pub fn pending_restore_ms() -> u64 {
    let at = RESTORE_AT.load(Ordering::SeqCst);
    at.saturating_sub(now_ms())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    #[test]
    fn paste_schedules_a_debounced_restore() {
        reset_for_tests();
        static CALLS: AtomicUsize = AtomicUsize::new(0);
        let ok = paste_and_restore("việt", &|| {
            CALLS.fetch_add(1, Ordering::SeqCst);
            true
        });
        // No clipboard tool or no way to write (sandbox) → paste reports
        // failure and the engine falls back to CommitText; with a working
        // tool it succeeds and schedules the restore.
        if ok {
            assert!(pending_restore_ms() > 0, "restore must be scheduled");
        } else {
            assert_eq!(pending_restore_ms(), 0, "failed paste must not schedule a restore");
            assert_eq!(CALLS.load(Ordering::SeqCst), 0, "no ctrl+v without a clipboard");
        }
        reset_for_tests();
    }
}
