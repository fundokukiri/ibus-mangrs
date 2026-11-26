//! Pick-driven personalization: the two TSV files under the config dir that
//! record which strip candidates the user picked, and the in-band bonus
//! those counts buy at scoring time.
//!
//! Files are written only when a pick lands (never from keystrokes — the
//! engine never records typed text), flushed by the engine's tick, and
//! reloaded when a file's mtime moves underneath us (an external edit, or
//! the GUI's clear button). Mode 0600: the files are per-user behavioral
//! data. A password field records nothing — the server's ContentType
//! handler keeps that gate closed before anything reaches here.

use std::collections::HashMap;
use std::fs::OpenOptions;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub const PICKS_FILE: &str = "user-picks.dict";
pub const PAIRS_FILE: &str = "user-picks-pairs.dict";

// Per-pick lifts. Multiplicative because the model's own counts live on a
// scale the bonus cannot be added to: a word at 38 000 unigrams gains
// nothing visible from an absolute +98, the same pick count changes nothing
// when the model is retrained to another scale, while a relative lift means
// the same thing on every model. 10 picks of a pair triple the score.
pub(crate) const LIFT_PAIR: f64 = 0.25;
pub(crate) const LIFT_FREQ: f64 = 0.05;

/// The user's pick counts: word frequencies and (context, word) pairs.
/// All mutating paths stay in memory until `flush` (the engine's tick) —
/// the key path never touches the filesystem.
pub struct UserPicks {
    dir: PathBuf,
    picks: HashMap<String, u32>,
    pairs: HashMap<(String, String), u32>,
    picks_mtime: Option<SystemTime>,
    pairs_mtime: Option<SystemTime>,
    dirty: bool,
}

impl UserPicks {
    /// Load both files from `dir` (missing files are an empty state).
    pub fn load(dir: &Path) -> UserPicks {
        let (picks, picks_mtime) = read_counts(&dir.join(PICKS_FILE));
        let (pairs, pairs_mtime) = read_pairs(&dir.join(PAIRS_FILE));
        UserPicks {
            dir: dir.to_path_buf(),
            picks,
            pairs,
            picks_mtime,
            pairs_mtime,
            dirty: false,
        }
    }

    /// Record one strip pick: the picked surface and, when known, the word
    /// whose prediction list it came from. Password fields never reach here.
    pub fn record_pick(&mut self, surface: &str, context: Option<&str>) {
        *self.picks.entry(surface.to_string()).or_insert(0) += 1;
        if let Some(prev) = context {
            *self
                .pairs
                .entry((prev.to_string(), surface.to_string()))
                .or_insert(0) += 1;
        }
        self.dirty = true;
    }

    /// The personalization multiplier for `surface` in the context of
    /// `context` (None for the mid-word state-A surface):
    /// `1 + LIFT_PAIR*p + LIFT_FREQ*f`. It multiplies a candidate's score
    /// inside its own band, so picks reorder near-ties and never outvote a
    /// word the model sees several times more often.
    pub fn lift(&self, surface: &str, context: Option<&str>) -> f64 {
        let freq = self.picks.get(surface).copied().unwrap_or(0);
        let pair = context.map_or(0, |prev| self.pair_count(prev, surface));
        1.0 + LIFT_PAIR * f64::from(pair) + LIFT_FREQ * f64::from(freq)
    }

    /// The recorded count of one surface, for tests and debug.
    pub fn picks_count(&self, surface: &str) -> Option<u32> {
        self.picks.get(surface).copied()
    }

    /// The recorded (context, word) pair count — the state-B lift reads it.
    pub fn pair_count(&self, prev: &str, next: &str) -> u32 {
        self.pairs
            .get(&(prev.to_string(), next.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Write the dirty state out. Skips files whose content is already
    /// exactly what would be written (an mtime bump would churn the reload
    /// loop), and never writes when nothing changed since the last flush.
    pub fn flush(&mut self) {
        if !self.dirty {
            return;
        }
        let picks_path = self.dir.join(PICKS_FILE);
        let pairs_path = self.dir.join(PAIRS_FILE);
        let picks_body = render_counts(
            &self.picks,
            "# MăngRS user picks — recorded from suggestion-strip picks only.\n# surface\tcount\n",
        );
        let pairs_body = render_pairs(&self.pairs, "# MăngRS pick pairs — the context word followed by the picked word.\n# prev\tnext\tcount\n");
        // Keep the loaded mtimes in step with what we actually wrote: a file
        // skipped as unchanged keeps its old mtime, a written one takes the
        // new file's mtime — so the next reload compares against reality.
        if let Ok(mtime) = write_file(&picks_path, &picks_body, self.picks_mtime) {
            self.picks_mtime = mtime;
        }
        if let Ok(mtime) = write_file(&pairs_path, &pairs_body, self.pairs_mtime) {
            self.pairs_mtime = mtime;
        }
        self.dirty = false;
    }

    /// Re-read a file whose mtime moved underneath us (external edit, the
    /// GUI's clear). A vanished file resets its counts.
    pub fn reload_if_changed(&mut self) {
        let picks_path = self.dir.join(PICKS_FILE);
        if let Ok(mtime) = std::fs::metadata(&picks_path).and_then(|m| m.modified()) {
            if Some(mtime) != self.picks_mtime {
                let (picks, picks_mtime) = read_counts(&picks_path);
                self.picks = picks;
                self.picks_mtime = picks_mtime;
            }
        } else if self.picks_mtime.is_some() || !self.picks.is_empty() {
            self.picks.clear();
            self.picks_mtime = None;
        }
        let pairs_path = self.dir.join(PAIRS_FILE);
        if let Ok(mtime) = std::fs::metadata(&pairs_path).and_then(|m| m.modified()) {
            if Some(mtime) != self.pairs_mtime {
                let (pairs, pairs_mtime) = read_pairs(&pairs_path);
                self.pairs = pairs;
                self.pairs_mtime = pairs_mtime;
            }
        } else if self.pairs_mtime.is_some() || !self.pairs.is_empty() {
            self.pairs.clear();
            self.pairs_mtime = None;
        }
    }

    /// Reset everything and delete both files (the engine's own clear path;
    /// the GUI button calls [`clear_dir`] directly).
    pub fn clear(&mut self) {
        clear_dir(&self.dir);
        self.picks.clear();
        self.pairs.clear();
        self.picks_mtime = None;
        self.pairs_mtime = None;
        self.dirty = false;
    }
}

/// TSV body for one map, sorted so rebuilds are byte-stable.
fn render_counts(map: &HashMap<String, u32>, header: &str) -> String {
    let mut lines: Vec<String> = map
        .iter()
        .map(|(surface, count)| format!("{surface}\t{count}\n"))
        .collect();
    lines.sort();
    let mut out = String::from(header);
    out.push_str(&lines.concat());
    out
}

fn render_pairs(map: &HashMap<(String, String), u32>, header: &str) -> String {
    let mut lines: Vec<String> = map
        .iter()
        .map(|((prev, next), count)| format!("{prev}\t{next}\t{count}\n"))
        .collect();
    lines.sort();
    let mut out = String::from(header);
    out.push_str(&lines.concat());
    out
}

/// One picks file: `surface\tcount` rows, garbage lines skipped. Returns
/// the map and the file's mtime (None when absent) for the reload loop.
fn read_counts(path: &Path) -> (HashMap<String, u32>, Option<SystemTime>) {
    let mut map = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let (surface, count) = match (parts.next(), parts.next(), parts.next()) {
                (Some(s), Some(c), None) => (s, c),
                _ => continue,
            };
            if let Ok(count) = count.trim().parse::<u32>() {
                map.insert(surface.trim().to_string(), count);
            }
        }
    }
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    (map, mtime)
}

fn read_pairs(path: &Path) -> (HashMap<(String, String), u32>, Option<SystemTime>) {
    let mut map = HashMap::new();
    if let Ok(text) = std::fs::read_to_string(path) {
        for line in text.lines() {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            let mut parts = line.split('\t');
            let (prev, next, count) = match (parts.next(), parts.next(), parts.next(), parts.next())
            {
                (Some(p), Some(n), Some(c), None) => (p, n, c),
                _ => continue,
            };
            if let Ok(count) = count.trim().parse::<u32>() {
                map.insert((prev.trim().to_string(), next.trim().to_string()), count);
            }
        }
    }
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    (map, mtime)
}

/// Write `body` to `path` mode 0600 via a temp file + rename. Returns the
/// file's post-write mtime, or None when the write was skipped as unchanged
/// (then the previous content's mtime still rules the reload loop).
fn write_file(
    path: &Path,
    body: &str,
    loaded_mtime: Option<SystemTime>,
) -> std::io::Result<Option<SystemTime>> {
    let bytes = body.as_bytes();
    if let Ok(existing) = std::fs::read(path) {
        if existing == bytes {
            return Ok(loaded_mtime);
        }
    }
    let tmp = path.with_extension("tmp");
    {
        use std::os::unix::fs::OpenOptionsExt;
        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)?;
        file.write_all(bytes)?;
    }
    std::fs::rename(&tmp, path)?;
    let mtime = std::fs::metadata(path).and_then(|m| m.modified()).ok();
    Ok(mtime)
}

/// Remove both pick files from `dir` (the GUI's delete button). Missing
/// files are fine — clearing an empty setup is a no-op.
pub fn clear_dir(dir: &Path) {
    for name in [PICKS_FILE, PAIRS_FILE] {
        let _ = std::fs::remove_file(dir.join(name));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> crate::test_support::TempDir {
        crate::test_support::TempDir::new(&format!("mangrs-learn-{tag}"))
    }

    #[test]
    fn pick_bumps_both_files_by_one() {
        let dir = temp_dir("bump");
        let mut picks = UserPicks::load(&dir);
        picks.record_pick("hello", Some("world"));
        picks.record_pick("hello", None);
        picks.flush();
        let picks_text = std::fs::read_to_string(dir.join(PICKS_FILE)).unwrap();
        let pairs_text = std::fs::read_to_string(dir.join(PAIRS_FILE)).unwrap();
        assert!(picks_text.contains("hello\t2\n"), "{picks_text}");
        assert!(pairs_text.contains("world\thello\t1\n"), "{pairs_text}");
        assert!(
            !picks_text.contains("world"),
            "the context word alone never lands in the picks file"
        );
    }

    #[test]
    fn files_are_0600_and_skip_unchanged_writes() {
        let dir = temp_dir("perm");
        let mut picks = UserPicks::load(&dir);
        picks.record_pick("hello", None);
        picks.flush();
        use std::os::unix::fs::PermissionsExt;
        let mode = std::fs::metadata(dir.join(PICKS_FILE))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o777, 0o600, "behavioral data stays private");
        let before = std::fs::metadata(dir.join(PICKS_FILE))
            .unwrap()
            .modified()
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(30));
        picks.flush();
        picks.flush();
        let after = std::fs::metadata(dir.join(PICKS_FILE))
            .unwrap()
            .modified()
            .unwrap();
        assert_eq!(
            before, after,
            "an unchanged flush must not rewrite the file"
        );
    }

    #[test]
    fn picks_reload_on_mtime() {
        let dir = temp_dir("reload");
        let mut picks = UserPicks::load(&dir);
        picks.record_pick("hello", None);
        picks.flush();
        // External edit behind our back (the user tuned their file).
        std::fs::write(dir.join(PICKS_FILE), "hello\t7\n").unwrap();
        picks.reload_if_changed();
        assert_eq!(picks.picks_count("hello"), Some(7));
        assert!(
            (picks.lift("hello", None) - 1.35).abs() < 1e-9,
            "7 picks lift 35%"
        );
    }

    #[test]
    fn clear_removes_both_files() {
        let dir = temp_dir("clear");
        let mut picks = UserPicks::load(&dir);
        picks.record_pick("hello", Some("world"));
        picks.flush();
        assert!(dir.join(PICKS_FILE).exists() && dir.join(PAIRS_FILE).exists());
        picks.clear();
        assert!(!dir.join(PICKS_FILE).exists() && !dir.join(PAIRS_FILE).exists());
        assert_eq!(picks.picks_count("hello"), None);
        assert_eq!(picks.lift("hello", Some("world")), 1.0);
    }
}
