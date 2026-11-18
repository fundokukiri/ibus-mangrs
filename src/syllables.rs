//! The bundled canon of real Vietnamese syllables.
//!
//! Provenance: ibus-bamboo's `vietnamese.cm.dict` — 7,884 real Vietnamese
//! syllables, GPLv3, the same license as this crate. The file and its
//! upstream attribution (`data/LICENSE.vietnamese-syllables.dict`) are
//! bundled verbatim; never edit them — engine-guaranteed additions go in
//! [`SUPPLEMENT`]. bamboo-core validates a syllable's shape but not its
//! existence (`sỏt` is shaped exactly like the real `sốt`), so the word-end
//! restore consults this closed canon to tell real words from tone-ized
//! English ones. Closed set: the language's written syllables are a fixed
//! inventory, so the table needs no updating when vocabulary changes.

use std::sync::OnceLock;

/// Forms this engine guarantees composed but the upstream canon lacks.
/// Filled from the golden coverage test; each entry needs a reason.
/// Kept sorted for the same binary search as the canon.
pub(crate) const SUPPLEMENT: &[&str] = &[];

const CANON_DATA: &str = include_str!("../data/vietnamese-syllables.dict");

/// The canon, sorted once on first use — the upstream file's order is not
/// sorted, and the binary search below needs it.
fn canon() -> &'static Vec<&'static str> {
    static CANON: OnceLock<Vec<&'static str>> = OnceLock::new();
    CANON.get_or_init(|| {
        let mut words: Vec<&str> = CANON_DATA.lines().filter(|l| !l.is_empty()).collect();
        words.sort_unstable();
        words.dedup();
        words
    })
}

/// Whether `word` (exact, case-sensitive — callers lowercase first) is a
/// real Vietnamese syllable. Word-end only: one binary search, never a
/// per-key cost.
pub fn contains(word: &str) -> bool {
    canon().binary_search(&word).is_ok() || SUPPLEMENT.binary_search(&word).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn canon_answers_real_and_fake() {
        assert!(contains("tôi"));
        assert!(contains("sốt"));
        assert!(!contains("sỏt"));
        assert!(!contains("bôt"));
    }

    #[test]
    fn canon_copy_is_pristine() {
        let words: Vec<&str> = CANON_DATA.lines().filter(|l| !l.is_empty()).collect();
        assert_eq!(words.len(), 7884);
        for w in &words {
            assert!(w.chars().all(|c| !c.is_whitespace()));
            assert_eq!(w.to_lowercase(), *w, "entry must be lowercase: {w}");
        }
        let mut dedup = words.clone();
        dedup.dedup();
        assert_eq!(dedup.len(), words.len(), "no duplicates");
        // The upstream file ships unsorted; the sorted property lives in the
        // loader's output, which the binary search relies on.
        let loaded = canon();
        assert!(loaded.windows(2).all(|w| w[0] < w[1]), "canon sorted");
        assert!(SUPPLEMENT.windows(2).all(|w| w[0] < w[1]), "supplement sorted");
    }
}
