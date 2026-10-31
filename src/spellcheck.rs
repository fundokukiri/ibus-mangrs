//! The bundled Vietnamese word list, loaded for the Spell Check toggle.
//!
//! Lookup-only by design (owner decision): the list never changes what the
//! engine commits — it backs word checks and future suggestion features.
//!
//! Load order, first that exists:
//! 1. `~/.config/ibus-mangrs/vn-words.txt` (user override)
//! 2. `$PREFIX/share/ibus-mangrs/vn-words.txt` (installed data file)
//! 3. the repository copy at build time (development and tests)
//!
//! Data: 38 350 words, UTF-8 NFC, one word per line, `#` comments — see
//! `data/README.md` for the source and its license.

use std::collections::HashSet;
use std::path::PathBuf;

pub struct WordList {
    words: HashSet<String>,
}

impl WordList {
    /// The file `load` reads (first candidate that exists), for mtime-gated
    /// reloads in the engine.
    pub(crate) fn path() -> Option<PathBuf> {
        Self::candidates().into_iter().find(|p| p.exists())
    }

    /// Candidate paths in priority order; `install_dir` is where
    /// `scripts/install.sh` / the PKGBUILD put the data file.
    fn candidates() -> Vec<PathBuf> {
        let mut paths = Vec::new();
        if let Some(dir) = dirs::config_dir() {
            paths.push(dir.join("ibus-mangrs").join("vn-words.txt"));
        }
        paths.push(PathBuf::from("/usr/share/ibus-mangrs/vn-words.txt"));
        paths.push(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("data")
                .join("vn-words.txt"),
        );
        paths
    }

    pub fn load() -> Self {
        let words = Self::candidates()
            .iter()
            .find_map(|path| std::fs::read_to_string(path).ok())
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty() && !l.starts_with('#'))
                    .map(|l| l.to_lowercase())
                    .collect()
            })
            .unwrap_or_default();
        Self { words }
    }

    /// Case-insensitive membership check on a single word (no spaces).
    pub fn known_word(&self, word: &str) -> bool {
        !word.is_empty() && self.words.contains(word.to_lowercase().as_str())
    }

    #[cfg(test)]
    pub fn len(&self) -> usize {
        self.words.len()
    }

    #[cfg(test)]
    pub fn is_empty(&self) -> bool {
        self.words.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_the_bundled_list() {
        let list = WordList::load();
        assert!(!list.is_empty(), "bundled data/vn-words.txt must be found");
        assert!(list.len() > 30_000, "unexpectedly small word list");
    }

    #[test]
    fn knows_common_words_case_insensitively() {
        let list = WordList::load();
        assert!(list.known_word("Học"));
        assert!(list.known_word("NGƯỜI"));
        assert!(list.known_word("cà-phê"));
        assert!(!list.known_word("toolbox"), "english word must not match");
        assert!(!list.known_word(""));
        assert!(!list.known_word("zzzqqq"));
    }
}
