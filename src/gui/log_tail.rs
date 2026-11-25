//! Reading the end of the debug log for the "Gỡ lỗi" page: a byte-capped tail
//! (the log reaches megabytes and nothing behind it is worth the wait) and the
//! short size label the row shows.

use std::collections::VecDeque;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

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

/// A log reader that only reads what was appended since the last look. The
/// viewer polls it once a second; re-reading the whole file every second would
/// cost memory bandwidth for nothing, and the file reaches megabytes.
pub(super) struct Reader {
    path: Option<PathBuf>,
    /// Byte offset already consumed, so the next read starts where this one
    /// stopped.
    offset: u64,
    /// A line that was still being written: it is completed by the next read
    /// instead of being shown cut in half.
    carry: String,
    lines: VecDeque<String>,
    max_lines: usize,
    started: bool,
}

impl Reader {
    pub(super) fn new(path: Option<PathBuf>, max_lines: usize) -> Reader {
        Reader {
            path,
            offset: 0,
            carry: String::new(),
            lines: VecDeque::new(),
            max_lines,
            started: false,
        }
    }

    /// Read what arrived since the last call and return only those lines. The
    /// first call opens at the end of the file, a tail rather than the whole
    /// history. Copying just the new lines keeps the per-second poll free of
    /// work proportional to the cache.
    pub(super) fn poll(&mut self) -> Vec<String> {
        let Some(path) = self.path.clone() else {
            return Vec::new();
        };
        let path = path.as_path();
        let Ok(meta) = std::fs::metadata(path) else {
            // The log may not exist yet; when it appears, start over.
            self.reset();
            return Vec::new();
        };
        let len = meta.len();
        if !self.started || len < self.offset {
            // First look, or the file was cleared or rotated under us.
            self.reset();
            let fresh = tail(path, self.max_lines, TAIL_BYTES);
            self.lines = fresh.iter().cloned().collect();
            self.offset = len;
            self.started = true;
            return fresh;
        }
        if len == self.offset {
            return Vec::new();
        }
        let Ok(mut file) = std::fs::File::open(path) else {
            return Vec::new();
        };
        if file.seek(SeekFrom::Start(self.offset)).is_err() {
            return Vec::new();
        }
        let mut buf = Vec::new();
        if file.read_to_end(&mut buf).is_err() {
            return Vec::new();
        }
        self.offset = len;
        let text = String::from_utf8_lossy(&buf);
        let mut added: Vec<String> = Vec::new();
        let mut rest: &str = &text;
        // A previous read that ended mid-line owes that line this one.
        if !self.carry.is_empty() {
            match rest.find('\n') {
                Some(pos) => {
                    let mut line = std::mem::take(&mut self.carry);
                    line.push_str(&rest[..pos]);
                    self.push(line, &mut added);
                    rest = &rest[pos + 1..];
                }
                None => {
                    self.carry.push_str(rest);
                    return Vec::new();
                }
            }
        }
        // Everything up to the last newline is complete lines; what follows it
        // is the line the engine is still writing.
        match rest.rfind('\n') {
            Some(pos) => {
                for line in rest[..pos].split('\n') {
                    self.push(line.to_string(), &mut added);
                }
                self.carry = rest[pos + 1..].to_string();
            }
            None => self.carry.push_str(rest),
        }
        added
    }

    fn push(&mut self, line: String, added: &mut Vec<String>) {
        if line.is_empty() {
            return;
        }
        added.push(line.clone());
        self.lines.push_back(line);
        while self.lines.len() > self.max_lines {
            self.lines.pop_front();
        }
    }

    /// Forget everything: the file is gone or was truncated.
    pub(super) fn reset(&mut self) {
        self.lines.clear();
        self.carry.clear();
        self.offset = 0;
        self.started = false;
    }

    /// The last `count` lines: the diagnostics look for the newest mention of
    /// an insertion backend, and cloning five thousand lines for that would be
    /// work in the same place the page is trying to stay cheap.
    pub(super) fn recent(&self, count: usize) -> Vec<String> {
        self.lines
            .iter()
            .skip(self.lines.len().saturating_sub(count))
            .cloned()
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TempDir;

    /// The viewer's line cap, repeated here so the tests read the same number.
    const VIEW_LINES: usize = 5_000;

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

    fn append(path: &Path, text: &str) {
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .expect("open log");
        file.write_all(text.as_bytes()).expect("append");
    }

    #[test]
    fn the_reader_picks_up_only_what_was_appended() {
        let dir = TempDir::new("mangrs-reader");
        let path = write_log(&dir, 3);
        let mut reader = Reader::new(Some(path.clone()), VIEW_LINES);
        assert_eq!(reader.poll().len(), 3);
        assert!(reader.poll().is_empty(), "nothing new, nothing read");
        append(&path, "line 0003\nline 0004\n");
        assert_eq!(reader.poll().len(), 2);
        assert_eq!(reader.recent(9).len(), 5);
        assert_eq!(reader.recent(1)[0], "line 0004");
    }

    #[test]
    fn a_half_written_line_waits_for_its_newline() {
        let dir = TempDir::new("mangrs-reader-carry");
        let path = write_log(&dir, 1);
        let mut reader = Reader::new(Some(path.clone()), VIEW_LINES);
        reader.poll();
        append(&path, "partial");
        assert!(reader.poll().is_empty(), "no newline yet, so not a line");
        assert_eq!(reader.recent(9).len(), 1);
        append(&path, " line\n");
        assert_eq!(reader.poll().len(), 1);
        assert_eq!(reader.recent(1)[0], "partial line");
    }

    #[test]
    fn recent_returns_the_newest_lines_in_order() {
        let dir = TempDir::new("mangrs-recent");
        let path = write_log(&dir, 20);
        let mut reader = Reader::new(Some(path), VIEW_LINES);
        reader.poll();
        assert_eq!(reader.recent(2), vec!["line 0018", "line 0019"]);
        assert_eq!(reader.recent(0).len(), 0);
        assert_eq!(reader.recent(100).len(), 20, "asking for more gives all");
    }

    #[test]
    fn the_reader_keeps_the_last_lines_only() {
        let dir = TempDir::new("mangrs-reader-cap");
        let path = write_log(&dir, 1);
        let mut reader = Reader::new(Some(path.clone()), 3);
        reader.poll();
        append(&path, "a\nb\nc\nd\ne\n");
        assert_eq!(reader.poll().len(), 5);
        assert_eq!(reader.recent(9), vec!["c", "d", "e"]);
    }

    #[test]
    fn a_cleared_log_starts_over() {
        let dir = TempDir::new("mangrs-reader-cleared");
        let path = write_log(&dir, 2);
        let mut reader = Reader::new(Some(path.clone()), VIEW_LINES);
        reader.poll();
        std::fs::write(&path, "fresh\n").expect("clear log");
        assert_eq!(reader.poll().len(), 1);
        assert_eq!(reader.recent(9), vec!["fresh"]);
    }

    #[test]
    fn size_labels_stay_short() {
        assert_eq!(size_label(0), "0 B");
        assert_eq!(size_label(18), "18 B");
        assert_eq!(size_label(912_000), "912 KB");
        assert_eq!(size_label(4_100_000), "4.1 MB");
    }
}
