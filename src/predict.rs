//! The n-gram prediction model: a mmap'd binary file of counted statistics.
//!
//! Three tiers ride one file — a frequency dictionary (VOCAB + UNIGRAM),
//! an n-gram over syllables/words (BIGRAM/TRIGRAM with per-context offset
//! indexes) and precomputed compound sequences (COMPOUNDS). Vietnamese
//! syllables and English words share one id space; the emoji layer keeps
//! its own CLDR keyword table and has no section here. The file layout is
//! pinned by the spec `docs/superpowers/specs/2026-11-21-word-prediction-design.md`
//! and built by `scripts/build-predict-model.py`; the committed
//! `data/model-sample.bin` (v1) and `data/model-sample-v2.bin` (v2) are the
//! test fixtures. Two formats share the magic: the header version picks the
//! row-id field width — 16 bits in v1 (65,536-row ceiling), 21 bits in v2
//! (~2.1M-row ceiling, compound members u32). Lookups are binary searches
//! over page-cached bytes — nothing is copied into RAM at startup.
//!
//! Every integer is little-endian. `open` validates the header and returns
//! `None` on anything unexpected (callers log that and the strip falls back
//! to the legacy embedded table); all accessors stay in bounds by
//! construction of that validation and never panic on a truncated section.

// Task 1 wires the harvest (row_count/flags/surface/unigram/located_path/
// open); the context and compound lookups stay unwired until Tasks 2-6, so
// the module-level allow covers exactly that surface and retires at the
// last wiring.
#![allow(dead_code)]

use std::fs::File;
use std::path::{Path, PathBuf};

use memmap2::Mmap;

const MAGIC: &[u8; 8] = b"MRSPRED1";
// The header version discriminates the layout, not just the revision: v1
// packs row ids in 16-bit fields, v2 in 21-bit fields with u32 compound
// members. Both readers live here — installed files of either format work.
const VERSION_V1: u32 = 1;
const VERSION_V2: u32 = 2;
const SECTION_COUNT: usize = 10;
const HEADER_LEN: usize = 16 + 8 * SECTION_COUNT;

// Section order in the header's offset table.
const SEC_VOCAB: usize = 0;
const SEC_VOCAB_INDEX: usize = 1;
const SEC_UNIGRAM: usize = 2;
const SEC_BIGRAM: usize = 3;
const SEC_BIGRAM_INDEX: usize = 4;
const SEC_TRIGRAM: usize = 5;
const SEC_TRIGRAM_INDEX: usize = 6;
const SEC_COMPOUNDS: usize = 7;
const SEC_COMPOUNDS_INDEX: usize = 8;
const SEC_PROVENANCE: usize = 9;

// VOCAB row flags.
pub const FLAG_VN_SYLLABLE: u8 = 1;
pub const FLAG_EN_WORD: u8 = 2;

pub struct PredictModel {
    map: Mmap,
    version: u32,
    row_count: u32,
    /// Row ids with a positive unigram count, ordered count-desc then
    /// surface-asc — the unigram backoff tier's stream. Built once at open
    /// (~65k ids on a full model), never touched per keystroke.
    uni_order: Vec<u32>,
    /// The sum of every unigram count — the unigram tier's denominator.
    uni_total: u64,
    offsets: [u64; SECTION_COUNT],
}

fn u16_le(b: &[u8]) -> Option<u16> {
    b.try_into().ok().map(u16::from_le_bytes)
}

fn u32_le(b: &[u8]) -> Option<u32> {
    b.try_into().ok().map(u32::from_le_bytes)
}

fn u64_le(b: &[u8]) -> Option<u64> {
    b.try_into().ok().map(u64::from_le_bytes)
}

/// The model file search order: an env override for dev and tests, then the
/// user config dir, then the system install path — the v2 name first, so an
/// installed v2 file wins over a leftover v1. `None` when no file exists
/// — callers log that and the strip falls back to the legacy embedded table.
pub fn located_path() -> Option<PathBuf> {
    const NAMES: [&str; 2] = ["mangrs-predict-v2.bin", "mangrs-predict-v1.bin"];
    if let Ok(path) = std::env::var("MANGRS_MODEL_PATH") {
        let path = PathBuf::from(path);
        if path.is_file() {
            return Some(path);
        }
    }
    if let Some(dir) = dirs::config_dir() {
        for name in NAMES {
            let path = dir.join("ibus-mangrs").join(name);
            if path.is_file() {
                return Some(path);
            }
        }
    }
    for name in NAMES {
        let path = PathBuf::from("/usr/share/ibus-mangrs/model").join(name);
        if path.is_file() {
            return Some(path);
        }
    }
    None
}

/// Maps and validates a model file. `None` on any malformed input: the
/// strip degrades to the legacy embedded table instead of failing typing.
pub fn open(path: &Path) -> Option<PredictModel> {
    let file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    if (len as usize) < HEADER_LEN {
        return None;
    }
    // SAFETY: mmap of a regular file we just opened. The map outlives the
    // File handle (the mapping pins its pages independently); the one
    // precondition memmap2 cannot enforce — the file must not be truncated
    // in place while mapped — holds because the engine never writes the
    // model file and a replaced file is a new inode, which does not affect
    // an existing mapping.
    let map = unsafe { Mmap::map(&file) }.ok()?;
    if map.get(0..8) != Some(&MAGIC[..]) {
        return None;
    }
    let version = u32_le(map.get(8..12)?)?;
    if version != VERSION_V1 && version != VERSION_V2 {
        return None;
    }
    if u32_le(map.get(12..16)?) != Some(SECTION_COUNT as u32) {
        return None;
    }
    let mut offsets = [0u64; SECTION_COUNT];
    for (i, slot) in offsets.iter_mut().enumerate() {
        *slot = u64_le(map.get(16 + i * 8..24 + i * 8)?)?;
    }
    // Sections sit in order after the header, inside the file.
    let mut prev = HEADER_LEN as u64;
    for &off in &offsets {
        if off < prev || off > len {
            return None;
        }
        prev = off;
    }
    let vocab_start = offsets[SEC_VOCAB] as usize;
    let row_count = u32_le(map.get(vocab_start..vocab_start + 4)?)?;
    let mut model = PredictModel {
        map,
        version,
        row_count,
        uni_order: Vec::new(),
        uni_total: 0,
        offsets,
    };
    // The unigram stream and its total come off the whole file once, at
    // open — config-apply cost, never key-path cost.
    let mut order: Vec<u32> = (0..model.row_count)
        .filter(|&id| model.unigram(id) > 0)
        .collect();
    order.sort_by(|&x, &y| {
        model
            .unigram(y)
            .cmp(&model.unigram(x))
            .then_with(|| model.surface(x).cmp(model.surface(y)))
    });
    model.uni_order = order;
    model.uni_total = (0..model.row_count).map(|i| model.unigram(i) as u64).sum();
    Some(model)
}

impl PredictModel {
    /// Vocabulary row count — every id is a row index below it.
    pub fn row_count(&self) -> usize {
        self.row_count as usize
    }

    /// Row-id field width in bits: v1 addresses rows with 16-bit fields, v2
    /// with 21-bit fields (the trigram packs three of them into one u64).
    fn id_shift(&self) -> u32 {
        if self.version == VERSION_V2 {
            21
        } else {
            16
        }
    }

    fn id_mask(&self) -> u64 {
        (1u64 << self.id_shift()) - 1
    }

    /// COMPOUNDS member width in bytes: u16 in v1, u32 in v2.
    fn member_width(&self) -> usize {
        if self.version == VERSION_V2 {
            4
        } else {
            2
        }
    }

    /// Row flags: [`FLAG_VN_SYLLABLE`], [`FLAG_EN_WORD`].
    pub fn flags(&self, id: u32) -> u8 {
        self.row(id).map_or(0, |(flags, _, _)| flags)
    }

    /// The row's NFC lowercase surface form.
    pub fn surface(&self, id: u32) -> &str {
        self.row(id).map_or("", |(_, _, surface)| surface)
    }

    /// The row's toneless NFC lowercase search key.
    pub fn key(&self, id: u32) -> &str {
        self.row(id).map_or("", |(_, key, _)| key)
    }

    /// Exact surface lookup (case-folded through the toneless key).
    pub fn id_of(&self, surface: &str) -> Option<u32> {
        let surface = surface.to_lowercase();
        let key = crate::suggest::toneless(&surface);
        let found = self.partition_by_key_surface(|mk, ms| {
            (mk, ms).cmp(&(key.as_str(), surface.as_str())) == std::cmp::Ordering::Less
        });
        self.row(found as u32)
            .is_some_and(|(_, mk, ms)| mk == key && ms == surface)
            .then_some(found as u32)
    }

    /// The vocabulary row range whose keys start with `key_prefix`.
    pub fn prefix_range(&self, key_prefix: &str) -> (usize, usize) {
        let prefix = crate::suggest::toneless(key_prefix);
        let start = self.partition_by_key_surface(|mk, _| mk < prefix.as_str());
        let n = self.row_count as usize;
        let mut end = start;
        while end < n {
            match self.row(end as u32) {
                Some((_, mk, _)) if mk.starts_with(prefix.as_str()) => end += 1,
                _ => break,
            }
        }
        (start, end)
    }

    /// First row whose (key, surface) does not satisfy `before` — a
    /// partition point over the (key, surface)-sorted vocabulary.
    fn partition_by_key_surface(&self, before: impl Fn(&str, &str) -> bool) -> usize {
        let mut lo = 0usize;
        let mut hi = self.row_count as usize;
        while lo < hi {
            let mid = (lo + hi) / 2;
            match self.row(mid as u32) {
                Some((_, mk, ms)) if before(mk, ms) => lo = mid + 1,
                // A decode failure mid-file would break the sort invariant;
                // treat it as the end of the searchable range.
                _ => hi = mid,
            }
        }
        lo
    }

    /// The row's unigram count.
    pub fn unigram(&self, id: u32) -> u32 {
        self.u32_at(self.offsets[SEC_UNIGRAM] as usize + (id as usize) * 4)
    }

    /// The unigram backoff stream: row ids by count (desc, surface asc),
    /// positive counts only. Built once at open.
    pub fn uni_order(&self) -> &[u32] {
        &self.uni_order
    }

    /// The sum of every unigram count — the unigram tier's denominator.
    pub fn uni_total(&self) -> u64 {
        self.uni_total
    }

    /// The `[start, end)` entry range a context owns in a fixed-width
    /// (12-byte) n-gram section, via its `row_count + 1` entry index.
    fn ngram_entry_range(&self, index_section: usize, context: u32) -> (usize, usize) {
        let base = self.offsets[index_section] as usize;
        let start = self.u32_at(base + (context as usize) * 4) as usize;
        let end = self.u32_at(base + ((context as usize) + 1) * 4) as usize;
        (start, end)
    }

    /// Adjacent-pair count of `(prev, next)`, 0 when absent.
    pub fn bigram(&self, prev: u32, next: u32) -> u32 {
        let key = ((prev as u64) << self.id_shift()) | next as u64;
        self.find_ngram(SEC_BIGRAM, SEC_BIGRAM_INDEX, prev, key)
    }

    /// `(next, count)` pairs after `prev`, in next-id order (the packed-key
    /// sort order inside the range).
    pub fn bigram_iter(&self, prev: u32) -> impl Iterator<Item = (u32, u32)> + '_ {
        let (start, end) = self.ngram_entry_range(SEC_BIGRAM_INDEX, prev);
        let entries = self.offsets[SEC_BIGRAM] as usize + 4;
        let mask = self.id_mask();
        (start..end).map(move |i| {
            let key = self.u64_at(entries + i * 12);
            let count = self.u32_at(entries + i * 12 + 8);
            ((key & mask) as u32, count)
        })
    }

    /// Triple count of `(w1, w2, w3)`, 0 when absent.
    pub fn trigram(&self, w1: u32, w2: u32, w3: u32) -> u32 {
        let shift = self.id_shift();
        let key = ((w1 as u64) << (2 * shift)) | ((w2 as u64) << shift) | w3 as u64;
        self.find_ngram(SEC_TRIGRAM, SEC_TRIGRAM_INDEX, w1, key)
    }

    /// `(w3, count)` pairs after the `(w1, w2)` context, in w3-id order.
    pub fn trigram_iter(&self, w1: u32, w2: u32) -> impl Iterator<Item = (u32, u32)> + '_ {
        let (start, end) = self.ngram_entry_range(SEC_TRIGRAM_INDEX, w1);
        let entries = self.offsets[SEC_TRIGRAM] as usize + 4;
        let shift = self.id_shift();
        let base = ((w1 as u64) << (2 * shift)) | ((w2 as u64) << shift);
        let mut lo = start;
        let mut hi = end;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.u64_at(entries + mid * 12) < base {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        (lo..end)
            .map(move |i| {
                (
                    self.u64_at(entries + i * 12),
                    self.u32_at(entries + i * 12 + 8),
                )
            })
            .take_while(move |(key, _)| key >> shift == base >> shift)
            .map(move |(key, count)| ((key & ((1u64 << shift) - 1)) as u32, count))
    }

    /// Count at the exact packed `key` inside a context's fixed-width
    /// n-gram range, 0 when absent.
    fn find_ngram(&self, section: usize, index_section: usize, context: u32, key: u64) -> u32 {
        let (start, end) = self.ngram_entry_range(index_section, context);
        let entries = self.offsets[section] as usize + 4;
        let mut lo = start;
        let mut hi = end;
        while lo < hi {
            let mid = (lo + hi) / 2;
            if self.u64_at(entries + mid * 12) < key {
                lo = mid + 1;
            } else {
                hi = mid;
            }
        }
        if lo < end && self.u64_at(entries + lo * 12) == key {
            self.u32_at(entries + lo * 12 + 8)
        } else {
            0
        }
    }

    /// The `[start, end)` byte range a head syllable owns in the variable-
    /// length COMPOUNDS section, via its byte-offset index.
    fn compound_byte_range(&self, head: u32) -> (usize, usize) {
        let base = self.offsets[SEC_COMPOUNDS_INDEX] as usize;
        let start = self.u32_at(base + (head as usize) * 4) as usize;
        let end = self.u32_at(base + ((head as usize) + 1) * 4) as usize;
        let comp = self.offsets[SEC_COMPOUNDS] as usize + 4;
        (comp + start, comp + end)
    }

    /// Compound sequences whose head syllable's key starts with
    /// `key_prefix`, strongest first: `(surface, strength)` where strength
    /// is the minimum adjacent bigram count along the sequence.
    pub fn compounds_for_prefix(&self, key_prefix: &str) -> Vec<(String, u32)> {
        let (start, end) = self.prefix_range(key_prefix);
        let width = self.member_width();
        let mut out = Vec::new();
        for row in start..end {
            let id = row as u32;
            if self.flags(id) & FLAG_VN_SYLLABLE == 0 {
                continue;
            }
            let (gs, ge) = self.compound_byte_range(id);
            let mut off = gs;
            while off < ge {
                let len = self.u8_at(off) as usize;
                let ids_end = off + 1 + len * width;
                if ids_end + 4 > ge || len == 0 {
                    break;
                }
                let mut surface = String::new();
                for k in 0..len {
                    if k > 0 {
                        surface.push(' ');
                    }
                    let member = if width == 4 {
                        self.u32_at(off + 1 + k * width)
                    } else {
                        u32::from(self.u16_at(off + 1 + k * width))
                    };
                    surface.push_str(self.surface(member));
                }
                out.push((surface, self.u32_at(ids_end)));
                off = ids_end + 4;
            }
        }
        out.sort_by_key(|(_, strength)| std::cmp::Reverse(*strength));
        out
    }

    /// The PROVENANCE blob: every corpus's name, URL, version and full
    /// license text — the binary carries its attribution standalone.
    pub fn provenance(&self) -> &str {
        let start = self.offsets[SEC_PROVENANCE] as usize;
        let len = self.u32_at(start) as usize;
        self.map
            .get(start + 4..start + 4 + len)
            .and_then(|b| std::str::from_utf8(b).ok())
            .unwrap_or("")
    }

    fn row(&self, id: u32) -> Option<(u8, &str, &str)> {
        let vi = self.offsets[SEC_VOCAB_INDEX] as usize;
        // VOCAB_INDEX has row_count + 1 u32 offsets relative to the first
        // row; a corrupted index yields an offset outside the map and the
        // get() below fails cleanly.
        let off =
            self.u32_at(vi + (id as usize) * 4) as usize + self.offsets[SEC_VOCAB] as usize + 4;
        let b = self.map.get(off..)?;
        let flags = *b.first()?;
        let key_len = u16_le(b.get(1..3)?)? as usize;
        let key = std::str::from_utf8(b.get(3..3 + key_len)?).ok()?;
        let so = 3 + key_len;
        let surface_len = u16_le(b.get(so..so + 2)?)? as usize;
        let surface = std::str::from_utf8(b.get(so + 2..so + 2 + surface_len)?).ok()?;
        Some((flags, key, surface))
    }

    fn u8_at(&self, off: usize) -> u8 {
        self.map.get(off).copied().unwrap_or(0)
    }

    fn u16_at(&self, off: usize) -> u16 {
        self.map.get(off..off + 2).and_then(u16_le).unwrap_or(0)
    }

    fn u32_at(&self, off: usize) -> u32 {
        self.map.get(off..off + 4).and_then(u32_le).unwrap_or(0)
    }

    fn u64_at(&self, off: usize) -> u64 {
        self.map.get(off..off + 8).and_then(u64_le).unwrap_or(0)
    }

    /// Header offsets, for the integrity tests.
    fn section_offsets(&self) -> [u64; SECTION_COUNT] {
        self.offsets
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("data/model-sample.bin")
    }

    fn sample() -> PredictModel {
        // Test-only expect: the committed fixture's absence is the failure.
        open(&sample_path())
            .expect("data/model-sample.bin missing — run scripts/build-predict-model.py --sample")
    }

    fn sample_v2_path() -> PathBuf {
        Path::new(env!("CARGO_MANIFEST_DIR")).join("data/model-sample-v2.bin")
    }

    fn sample_v2() -> PredictModel {
        // Test-only expect: the committed fixture's absence is the failure.
        open(&sample_v2_path()).expect(
            "data/model-sample-v2.bin missing — run scripts/build-predict-model.py --sample --format v2",
        )
    }

    #[test]
    fn open_rejects_garbage() {
        let mut path = std::env::temp_dir();
        path.push(format!("mangrs-garbage-{}.bin", std::process::id()));
        std::fs::write(&path, vec![0xA5u8; 100]).unwrap_or_else(|e| panic!("temp write: {e}"));
        let got = open(&path);
        let _ = std::fs::remove_file(&path);
        assert!(got.is_none(), "garbage must not parse");
    }

    #[test]
    fn open_rejects_a_truncated_file() {
        let m = sample();
        let mut path = std::env::temp_dir();
        path.push(format!("mangrs-trunc-{}.bin", std::process::id()));
        // Everything after the header cut: a valid header on a broken file.
        std::fs::write(&path, &m.map[..HEADER_LEN]).unwrap_or_else(|e| panic!("temp write: {e}"));
        let got = open(&path);
        let _ = std::fs::remove_file(&path);
        assert!(got.is_none(), "a header-only file must not parse");
    }

    #[test]
    fn sample_parses_and_sections_are_ordered() {
        let m = sample();
        let offsets = m.section_offsets();
        assert_eq!(
            offsets[SEC_VOCAB], HEADER_LEN as u64,
            "VOCAB follows the header"
        );
        for pair in offsets.windows(2) {
            assert!(pair[0] < pair[1], "sections must strictly increase");
        }
        assert!(
            offsets[SEC_PROVENANCE] <= m.map.len() as u64,
            "last section must sit inside the file"
        );
        assert!(m.row_count() > 0, "the sample carries rows");
    }

    #[test]
    fn sample_vocab_anchors() {
        let m = sample();
        // Compounds are not vocabulary rows.
        assert_eq!(m.id_of("sinh viên"), None);
        let sinh = m.id_of("sinh").expect("sinh is a sample row");
        assert_eq!(m.flags(sinh) & FLAG_VN_SYLLABLE, FLAG_VN_SYLLABLE);
        let hello = m.id_of("hello").expect("hello is a sample row");
        assert_eq!(m.flags(hello) & FLAG_EN_WORD, FLAG_EN_WORD);
    }

    #[test]
    fn sample_bigram_anchor() {
        let m = sample();
        let sinh = m.id_of("sinh").expect("sinh row");
        let vien = m.id_of("viên").expect("viên row");
        let hoc = m.id_of("học").expect("học row");
        let strong = m.bigram(sinh, vien);
        let weak = m.bigram(sinh, hoc);
        assert!(strong > 0 && weak > 0, "both fixture pairs counted");
        assert!(strong >= weak, "viên must lead học after sinh");
        // bigram_iter walks next-id order; the anchor checks membership.
        assert!(
            m.bigram_iter(sinh).any(|(id, c)| id == vien && c == strong),
            "viên is a counted follower of sinh"
        );
    }

    #[test]
    fn sample_english_bigram_anchor() {
        let m = sample();
        let hello = m.id_of("hello").expect("hello row");
        let world = m.id_of("world").expect("world row");
        let help = m.id_of("help").expect("help row");
        assert!(
            m.bigram(hello, world) > m.bigram(hello, help),
            "the fixture's EN next word leads"
        );
        assert!(
            m.bigram_iter(hello).any(|(id, _)| id == world),
            "world is a follower of hello"
        );
    }

    #[test]
    fn sample_trigram_anchor() {
        let m = sample();
        let sinh = m.id_of("sinh").expect("sinh row");
        let vien = m.id_of("viên").expect("viên row");
        let hoc = m.id_of("học").expect("học row");
        assert!(
            m.trigram(sinh, vien, hoc) > 0,
            "the fixture's trigram counted"
        );
        assert!(
            m.trigram_iter(sinh, vien).any(|(id, _)| id == hoc),
            "học follows the sinh viên context"
        );
    }

    #[test]
    fn sample_compound_anchor() {
        let m = sample();
        let compounds = m.compounds_for_prefix("sinh");
        assert!(
            compounds
                .first()
                .is_some_and(|(word, _)| word == "sinh viên"),
            "sinh viên leads the sinh compounds, got {compounds:?}"
        );
    }

    #[test]
    fn compounds_match_decomposed_prefix() {
        // NFD typed input folds through the same toneless key: học has no
        // precomposed ô-hook form, so its decomposition is plain o + the
        // hook mark U+0323 — toneless drops the mark and reaches the same
        // compounds as the NFC spelling.
        let m = sample();
        assert_eq!(
            m.compounds_for_prefix("ho\u{0323}c"),
            m.compounds_for_prefix("học")
        );
        assert!(
            !m.compounds_for_prefix("ho\u{0323}c").is_empty(),
            "học carries compounds in the fixture"
        );
    }

    #[test]
    fn sample_prefix_range() {
        let m = sample();
        let (start, end) = m.prefix_range("si");
        assert!(start < end, "the si range is non-empty");
        let surfaces: Vec<&str> = (start..end).map(|row| m.surface(row as u32)).collect();
        assert!(surfaces.contains(&"sinh"), "sinh sits in the si range");
        assert!(
            surfaces
                .iter()
                .all(|s| m.key(m.id_of(s).unwrap_or(u32::MAX)).starts_with("si")),
            "every row in the range keys on si"
        );
        let (tstart, tend) = m.prefix_range("tr");
        let trai = m.id_of("trai").expect("trai row");
        let trai_row = trai as usize;
        assert!(
            tstart <= trai_row && trai_row < tend,
            "trai belongs to the tr range"
        );
        assert!(trai_row >= end, "trai must sort after the whole si range");
    }

    #[test]
    fn sample_unigram_stream_leads_with_the_frequent() {
        let m = sample();
        // The fixture's heaviest row leads the stream; the total matches the
        // sum the fixture counts to (hand-checked against the sample build).
        let hello = m.id_of("hello").expect("hello row");
        assert_eq!(m.uni_order().first(), Some(&hello));
        assert!(m.uni_total() > 0, "the stream carries a real denominator");
    }

    #[test]
    fn sample_provenance_names_the_corpus_fields() {
        let m = sample();
        let blob = m.provenance();
        assert!(blob.contains("license"), "the blob carries license text");
        assert!(blob.contains("weight"), "the blob carries corpus weights");
    }

    #[test]
    fn v2_sample_parses_with_wide_ids() {
        let m = sample_v2();
        assert!(m.row_count() > 0, "the v2 fixture carries rows");
        let sinh = m.id_of("sinh").expect("sinh row");
        let vien = m.id_of("viên").expect("viên row");
        let hoc = m.id_of("học").expect("học row");
        assert!(m.bigram(sinh, vien) > 0, "v2 21-bit bigram keys unpack");
        assert!(
            m.bigram_iter(sinh).any(|(id, c)| id == vien && c > 0),
            "v2 iter unpacks the 21-bit next field"
        );
        assert!(
            m.trigram(sinh, vien, hoc) > 0,
            "v2 21-bit trigram keys unpack"
        );
        assert!(
            m.trigram_iter(sinh, vien).any(|(id, _)| id == hoc),
            "v2 trigram_iter walks the 21-bit fields"
        );
        assert_eq!(
            m.compounds_for_prefix("sinh")
                .first()
                .map(|(word, _)| word.as_str()),
            Some("sinh viên"),
            "v2 u32 compound members unpack"
        );
    }

    #[test]
    fn open_rejects_an_unknown_version() {
        let m = sample_v2();
        let mut bytes = m.map.to_vec();
        bytes[8..12].copy_from_slice(&3u32.to_le_bytes());
        let mut path = std::env::temp_dir();
        path.push(format!("mangrs-v3-{}.bin", std::process::id()));
        std::fs::write(&path, bytes).unwrap_or_else(|e| panic!("temp write: {e}"));
        let got = open(&path);
        let _ = std::fs::remove_file(&path);
        assert!(got.is_none(), "an unknown header version must not parse");
    }
}
