//! Reading the end of the debug log for the "Gỡ lỗi" page: a byte-capped tail
//! (the log reaches megabytes and nothing behind it is worth the wait) and the
//! short size label the row shows.

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// How much of the file the tail reader is allowed to touch. 256 KB covers
/// thousands of lines and reads instantly even on a log that has grown for
/// hours.
pub(super) const TAIL_BYTES: u64 = 256 * 1024;

/// The last `max_lines` lines of `path`, reading at most `max_bytes` from its
/// end. A missing file reads as empty rather than an error: the log does not
/// exist until debug mode is switched on.
pub(super) fn tail(path: &Path, max_lines: usize, max_bytes: u64) -> Vec<String> {
    let Ok(mut file) = std::fs::File::open(path) else {
        return Vec::new();
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let start = len.saturating_sub(max_bytes);
    if file.seek(SeekFrom::Start(start)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if file.take(max_bytes).read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    let mut lines: Vec<String> = text.lines().map(str::to_string).collect();
    // A read that starts mid-line begins with a fragment of a line the reader
    // never saw: drop it, the same way a shell `tail` does not invent it.
    if start > 0 && !lines.is_empty() {
        lines.remove(0);
    }
    let skip = lines.len().saturating_sub(max_lines);
    lines.drain(..skip);
    lines
}

/// "18 B", "912 KB", "4.1 MB": at most four characters and a unit, so a row
/// subtitle can carry it next to the path.
pub(super) fn size_label(bytes: u64) -> String {
    const KB: f64 = 1000.0;
    let value = bytes as f64;
    if bytes < 1000 {
        format!("{bytes} B")
    } else if value < 999_950.0 {
        format!("{:.0} KB", value / KB)
    } else {
        format!("{:.1} MB", value / (KB * KB))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    fn write_log(dir: &TempDir, lines: usize) -> std::path::PathBuf {
        let path = dir.path().join("debug.log");
        let body: String = (0..lines).map(|i| format!("line {i:04}\n")).collect();
        std::fs::write(&path, body).expect("write log");
        path
    }

    #[test]
    fn tail_keeps_the_last_lines_in_order() {
        let dir = TempDir::new("mangrs-tail");
        let path = write_log(&dir, 4000);
        let lines = tail(&path, 3, TAIL_BYTES);
        assert_eq!(lines, vec!["line 3997", "line 3998", "line 3999"]);
    }

    #[test]
    fn tail_respects_the_byte_cap_and_drops_the_half_line() {
        let dir = TempDir::new("mangrs-tail-cap");
        let path = write_log(&dir, 4000);
        let lines = tail(&path, 10, 12);
        assert!(
            lines.len() <= 1,
            "a 12-byte read cannot hold more: {lines:?}"
        );
        assert!(
            lines
                .iter()
                .all(|l| l.ends_with(|c: char| c.is_ascii_digit())),
            "the fragment left by starting mid-line is gone: {lines:?}"
        );
    }

    #[test]
    fn a_missing_log_reads_as_empty() {
        let dir = TempDir::new("mangrs-tail-missing");
        assert!(tail(&dir.path().join("debug.log"), 50, TAIL_BYTES).is_empty());
    }

    #[test]
    fn size_labels_stay_short() {
        assert_eq!(size_label(0), "0 B");
        assert_eq!(size_label(18), "18 B");
        assert_eq!(size_label(912_000), "912 KB");
        assert_eq!(size_label(4_100_000), "4.1 MB");
    }
}
