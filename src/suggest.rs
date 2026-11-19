//! Dictionary suggestion strip support.
//!
//! The strip suggests whole words while the user types: [`SuggestTable`] is
//! the frequency table it ranks against, `SuggestTable::candidates` the list
//! it serves for the typed word, [`toneless`] the search key that makes the
//! match tone-insensitive, and [`merge_user_entries`] the hook that folds
//! user-dictionary rows into a table's row set at a weight that outranks
//! the builtin noise.
//!
//! `SuggestTable::embedded()` compiles in `data/vietnamese-freq.dict`
//! (`syllable<TAB>freq`, NFC, sorted by toneless form — the data tests
//! below are the guard for that order); `SuggestTable::from_rows` builds a
//! table from raw rows for tests and the merged builtin + user set. No
//! unicode-normalization or edit-distance dependency: NFC handling and the
//! capped Damerau-Levenshtein are hand-rolled.

// The strip's engine and server consumers land in later tasks; until then
// only the tests below touch this module.
#![allow(dead_code)]

use std::cmp::Reverse;
use std::collections::HashSet;
use std::sync::OnceLock;

const FREQ_DATA: &str = include_str!("../data/vietnamese-freq.dict");

/// Completions and corrections lists never grow past this; the strip pages
/// through the rest.
const MAX_CANDIDATES: usize = 25;

/// User-dictionary rows land at half of `u32::MAX`: above every builtin
/// frequency in the table, with headroom left for later weighting.
const USER_WEIGHT: u32 = u32::MAX / 2;

/// A suggestion the strip can offer: the surface word and its table weight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub word: String,
    pub freq: u32,
}

/// One table row: the surface syllable, its precomputed toneless form (the
/// sort and search key) and its frequency.
struct Row {
    word: String,
    key: String,
    freq: u32,
}

/// The frequency table behind the suggestion strip: rows sorted by toneless
/// form (then surface word), so every lookup is a walk over the `key` run.
pub struct SuggestTable {
    rows: Vec<Row>,
}

/// The plain vowel behind one of the 60 lowercase tone-marked precomposed
/// vowels — the five tones over the twelve shapes — None otherwise.
/// `toneless` lowercases before consulting this, so uppercase is covered.
fn tone_base(c: char) -> Option<char> {
    match c {
        'á' | 'à' | 'ả' | 'ã' | 'ạ' => Some('a'),
        'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => Some('ă'),
        'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' => Some('â'),
        'é' | 'è' | 'ẻ' | 'ẽ' | 'ẹ' => Some('e'),
        'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => Some('ê'),
        'í' | 'ì' | 'ỉ' | 'ĩ' | 'ị' => Some('i'),
        'ó' | 'ò' | 'ỏ' | 'õ' | 'ọ' => Some('o'),
        'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' => Some('ô'),
        'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => Some('ơ'),
        'ú' | 'ù' | 'ủ' | 'ũ' | 'ụ' => Some('u'),
        'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => Some('ư'),
        'ý' | 'ỳ' | 'ỷ' | 'ỹ' | 'ỵ' => Some('y'),
        _ => None,
    }
}

/// The five tone diacritics as combining marks, for decomposed (NFD) input.
fn is_tone_mark(c: char) -> bool {
    matches!(
        c,
        '\u{0300}' | '\u{0301}' | '\u{0303}' | '\u{0309}' | '\u{0323}'
    )
}

/// The three vowel-shape marks (circumflex, breve, horn) as combining marks.
fn is_shape_mark(c: char) -> bool {
    matches!(c, '\u{0302}' | '\u{0306}' | '\u{031B}')
}

/// The precomposed vowel that `base` plus combining `mark` forms, for the
/// six shape vowels — the only compositions Vietnamese needs for NFC output.
fn composed_shape(base: char, mark: char) -> Option<char> {
    match (base, mark) {
        ('a', '\u{0302}') => Some('â'),
        ('a', '\u{0306}') => Some('ă'),
        ('e', '\u{0302}') => Some('ê'),
        ('o', '\u{0302}') => Some('ô'),
        ('o', '\u{031B}') => Some('ơ'),
        ('u', '\u{031B}') => Some('ư'),
        _ => None,
    }
}

/// Strips the five tone diacritics (sắc, huyền, hỏi, ngã, nặng), keeping the
/// vowel shapes (`ă â ê ô ơ ư`) and `đ`, lowercasing everything. NFC
/// precomposed in and out; decomposed (NFD) input strips the same way, so
/// pasted text in either normalization form agrees.
pub fn toneless(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_ascii() {
            out.push(c.to_ascii_lowercase());
            continue;
        }
        let lower = c.to_lowercase().next().unwrap_or(c);
        match tone_base(lower) {
            Some(base) => out.push(base),
            None if is_tone_mark(lower) => {}
            None if is_shape_mark(lower) => {
                match out
                    .chars()
                    .next_back()
                    .and_then(|base| composed_shape(base, lower))
                {
                    Some(v) => {
                        out.pop();
                        out.push(v);
                    }
                    None => out.push(lower),
                }
            }
            None => out.push(lower),
        }
    }
    out
}

/// Reweights user-dictionary rows for concatenation onto a table's row set:
/// every user word carries [`USER_WEIGHT`], outranking every builtin
/// frequency. Repeated words collapse to one row and empty words drop — a
/// duplicated or blank word would otherwise be offered twice or match
/// everything downstream.
pub fn merge_user_entries(rows: Vec<(String, u32)>) -> Vec<(String, u32)> {
    let mut seen = HashSet::new();
    rows.into_iter()
        .filter(|(word, _)| !word.is_empty() && seen.insert(word.clone()))
        .map(|(word, _)| (word, USER_WEIGHT))
        .collect()
}

/// Restricted Damerau-Levenshtein distance between `a` and `b` — adjacent
/// transpositions only, the standard spelling-correction distance — capped
/// at `max`: the distance if it is at most `max`, else `max + 1`. Walks `b`
/// char by char over three rolling rows and aborts as soon as a whole row
/// exceeds `max`; that is safe because the optimal edit path visits every
/// row (a transposition jump can be rerouted through the skipped row at the
/// same cost), so the final cost is never below the current row minimum.
fn damerau_limited(a: &[char], b: &str, max: usize) -> usize {
    if a.len().abs_diff(b.chars().count()) > max {
        return max + 1;
    }
    let cap = max as u32;
    let mut prev2 = vec![0u32; a.len() + 1]; // row i-2, read from column j >= 2 on
    let mut prev: Vec<u32> = (0..=a.len() as u32).collect(); // column j = 0
    let mut cur = vec![0u32; a.len() + 1];
    let mut prev_b = '\0'; // the previous column's b char, for the transposition check
    for (j, bc) in b.chars().enumerate() {
        cur[0] = (j + 1) as u32;
        for (idx, &ac) in a.iter().enumerate() {
            let mut v = (prev[idx] + u32::from(ac != bc))
                .min(prev[idx + 1] + 1)
                .min(cur[idx] + 1);
            if j >= 1 && idx >= 1 && ac == prev_b && a[idx - 1] == bc {
                v = v.min(prev2[idx - 1] + 1);
            }
            cur[idx + 1] = v;
        }
        if cur.iter().all(|&m| m > cap) {
            return max + 1;
        }
        std::mem::swap(&mut prev2, &mut prev);
        std::mem::swap(&mut prev, &mut cur);
        prev_b = bc;
    }
    if prev[a.len()] <= cap {
        prev[a.len()] as usize
    } else {
        max + 1
    }
}

impl SuggestTable {
    /// Builds a table from raw rows in any order: rows are sorted by
    /// toneless form (then surface word) here, and a word listed twice keeps
    /// its highest frequency — the runtime table is builtin rows plus
    /// user-dict rows, where a user override of a builtin word must not be
    /// offered twice. (`embedded` trusts the data file's order instead; the
    /// data tests are its corruption guard.)
    pub fn from_rows(rows: Vec<(String, u32)>) -> SuggestTable {
        let mut rows: Vec<Row> = rows
            .into_iter()
            .map(|(word, freq)| Row {
                key: toneless(&word),
                word,
                freq,
            })
            .collect();
        rows.sort_unstable_by(|a, b| {
            (&a.key, &a.word)
                .cmp(&(&b.key, &b.word))
                .then(b.freq.cmp(&a.freq))
        });
        rows.dedup_by(|a, b| a.word == b.word);
        SuggestTable { rows }
    }

    /// The compiled-in frequency table, parsed once per process. Malformed
    /// lines are skipped silently — the data tests prove the committed file
    /// well-formed, and a bad line must never break typing. The file's sort
    /// order (toneless form, then syllable) is trusted as-is: re-sorting
    /// here would silently mask a corrupted file.
    pub fn embedded() -> &'static SuggestTable {
        static EMBEDDED: OnceLock<SuggestTable> = OnceLock::new();
        EMBEDDED.get_or_init(|| SuggestTable {
            rows: FREQ_DATA
                .lines()
                .filter(|line| !line.is_empty() && !line.starts_with('#'))
                .filter_map(|line| {
                    let (word, freq) = line.split_once('\t')?;
                    let freq = freq.parse::<u32>().ok()?;
                    Some(Row {
                        key: toneless(word),
                        word: word.to_string(),
                        freq,
                    })
                })
                .collect(),
        })
    }

    /// Entries whose toneless form starts with `toneless(word)` — every tone
    /// variant and longer word over the typed prefix — ranked by frequency
    /// (equal frequencies keep table order), the typed word itself excluded,
    /// capped at [`MAX_CANDIDATES`]. The exclusion compares surface forms,
    /// so a wrong-tone spelling (sỏt) still reaches its toneless neighbors.
    pub fn completions(&self, word: &str) -> Vec<Candidate> {
        let key = toneless(word);
        let start = self
            .rows
            .partition_point(|row| row.key.as_str() < key.as_str());
        let mut matched: Vec<&Row> = Vec::new();
        for row in &self.rows[start..] {
            if !row.key.starts_with(key.as_str()) {
                break;
            }
            if row.word != word {
                matched.push(row);
            }
        }
        matched.sort_by_key(|row| Reverse(row.freq));
        matched
            .into_iter()
            .take(MAX_CANDIDATES)
            .map(|row| Candidate {
                word: row.word.clone(),
                freq: row.freq,
            })
            .collect()
    }

    /// Distance-2 toneless neighbors of `word`, ranked by distance then
    /// frequency, capped at [`MAX_CANDIDATES`]; the typed word itself is not
    /// its own correction. Pure on purpose: the strip's gating (completions
    /// first, corrections only for unknown words) lives in
    /// [`Self::candidates`], because an exclusive gate could never surface a
    /// distance-1 shape neighbor (sốt behind sỏt) once a prefix hit fills
    /// completions. The ±2 length pre-filter skips the distance walk for
    /// hopeless lengths.
    pub fn corrections(&self, word: &str) -> Vec<Candidate> {
        let key = toneless(word);
        let key_chars: Vec<char> = key.chars().collect();
        let mut matched: Vec<(usize, &Row)> = Vec::new();
        for row in &self.rows {
            if row.word == word {
                continue;
            }
            if row.key.chars().count().abs_diff(key_chars.len()) > 2 {
                continue;
            }
            let distance = damerau_limited(&key_chars, &row.key, 2);
            if distance <= 2 {
                matched.push((distance, row));
            }
        }
        matched.sort_by(|a, b| a.0.cmp(&b.0).then(b.1.freq.cmp(&a.1.freq)));
        matched
            .into_iter()
            .take(MAX_CANDIDATES)
            .map(|(_, row)| Candidate {
                word: row.word.clone(),
                freq: row.freq,
            })
            .collect()
    }

    /// The strip's list for `word`: completions first (frequency order),
    /// then — only when the table does not know the word — distance-2
    /// corrections not already listed, filling up to the same
    /// [`MAX_CANDIDATES`] cap. A known word stays quiet: completions only,
    /// even when that is nothing.
    pub fn candidates(&self, word: &str) -> Vec<Candidate> {
        let mut list = self.completions(word);
        if self.contains(word) || list.len() == MAX_CANDIDATES {
            return list;
        }
        for candidate in self.corrections(word) {
            if list.iter().any(|known| known.word == candidate.word) {
                continue;
            }
            list.push(candidate);
            if list.len() == MAX_CANDIDATES {
                break;
            }
        }
        list
    }

    /// True when the table has the word itself, case folded — the word the
    /// user typed is a dictionary word, so [`Self::candidates`] keeps its
    /// list at completions only. Exact membership, not toneless membership:
    /// a tone-mangled spelling (sỏt) is not a row even though its toneless
    /// form matches sót, which is what keeps its shape neighbor sốt
    /// correction-eligible.
    pub fn contains(&self, word: &str) -> bool {
        let word = word.to_lowercase();
        let key = toneless(&word);
        let start = self.rows.partition_point(|row| {
            (row.key.as_str(), row.word.as_str()) < (key.as_str(), word.as_str())
        });
        self.rows
            .get(start)
            .is_some_and(|row| row.key == key && row.word == word)
    }
}

#[cfg(test)]
mod data_tests {
    use super::*;

    /// unicode-normalization is not a dependency, so NFC is checked as
    /// membership in the precomposed Vietnamese letter set: a decomposed row
    /// would carry combining marks, which this set rejects. `đ` has no marked
    /// forms and the six plain letters only exist composed.
    fn is_nfc_letter(c: char) -> bool {
        c.is_ascii_lowercase() || "ăâđêôơư".contains(c) || tone_base(c).is_some()
    }

    /// Panic text instead of expect(): test-only parsing of build-time data,
    /// where a message naming the offending line is the whole point.
    fn rows() -> Vec<(&'static str, u64)> {
        FREQ_DATA
            .lines()
            .enumerate()
            .map(|(n, line)| (n + 1, line))
            .filter(|(_, line)| !line.is_empty() && !line.starts_with('#'))
            .map(|(n, line)| {
                let (syllable, freq) = line
                    .split_once('\t')
                    .unwrap_or_else(|| panic!("line {n} is not syllable<TAB>freq: {line:?}"));
                let freq = freq
                    .parse::<u64>()
                    .unwrap_or_else(|_| panic!("line {n} has a non-numeric frequency: {freq:?}"));
                (syllable, freq)
            })
            .collect()
    }

    #[test]
    fn data_file_parses_into_syllable_tab_freq_rows() {
        assert!(!FREQ_DATA.is_empty());
        assert!(
            !rows().is_empty(),
            "no data rows under the # comment header"
        );
    }

    #[test]
    fn every_row_is_nfc() {
        for (syllable, _) in rows() {
            assert!(
                syllable.chars().all(is_nfc_letter),
                "row {syllable:?} is not composed Vietnamese (NFC)"
            );
        }
    }

    #[test]
    fn rows_are_sorted_by_toneless_form_then_syllable() {
        let rows = rows();
        for pair in rows.windows(2) {
            let (prev, next) = (&pair[0], &pair[1]);
            let prev_key = (toneless(prev.0), prev.0);
            let next_key = (toneless(next.0), next.0);
            assert!(
                prev_key <= next_key,
                "rows out of order: {:?} (toneless {:?}) must not come after {:?} (toneless {:?})",
                prev.0,
                prev_key.0,
                next.0,
                next_key.0
            );
        }
    }

    #[test]
    fn every_frequency_is_at_least_one() {
        for (syllable, freq) in rows() {
            assert!(freq >= 1, "row {syllable:?} has frequency {freq}");
        }
    }

    #[test]
    fn row_count_covers_the_vietnamese_syllable_set() {
        assert!(rows().len() >= 7000, "only {} rows", rows().len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rows(list: &[(&str, u32)]) -> Vec<(String, u32)> {
        list.iter()
            .map(|(word, freq)| (word.to_string(), *freq))
            .collect()
    }

    // ---- toneless ----

    #[test]
    fn toneless_strips_every_tone_on_every_vowel_shape() {
        // The five tones (sắc huyền hỏi ngã nặng) over the twelve shapes.
        let shapes = [
            ("a", "áàảãạ"),
            ("ă", "ắằẳẵặ"),
            ("â", "ấầẩẫậ"),
            ("e", "éèẻẽẹ"),
            ("ê", "ếềểễệ"),
            ("i", "íìỉĩị"),
            ("o", "óòỏõọ"),
            ("ô", "ốồổỗộ"),
            ("ơ", "ớờởỡợ"),
            ("u", "úùủũụ"),
            ("ư", "ứừửữự"),
            ("y", "ýỳỷỹỵ"),
        ];
        for (plain, marked) in shapes {
            assert_eq!(toneless(plain), plain);
            for c in marked.chars() {
                assert_eq!(toneless(&c.to_string()), plain, "{c} must strip to {plain}");
            }
        }
    }

    #[test]
    fn toneless_keeps_shape_marks_and_d_strips_tones_folds_case() {
        assert_eq!(toneless("sỏt"), "sot");
        assert_eq!(toneless("chủ"), "chu");
        assert_eq!(toneless("thì"), "thi");
        // The horn on ư is a vowel shape, not a tone: chưa is unchanged and
        // a tone over the horn strips to the same shape.
        assert_eq!(toneless("chưa"), "chưa");
        assert_eq!(toneless("chứa"), "chưa");
        assert_eq!(toneless("đ"), "đ");
        assert_eq!(toneless("Đà"), "đa");
        assert_eq!(toneless("THÌ"), "thi");
        assert_eq!(toneless("ChƯa"), "chưa");
    }

    #[test]
    fn toneless_handles_decomposed_input() {
        // Pasted text may arrive NFD: the five tone marks drop, the three
        // shape marks compose back so the output stays precomposed.
        assert_eq!(toneless("e\u{0301}"), "e");
        assert_eq!(toneless("a\u{0306}"), "ă");
        assert_eq!(toneless("A\u{0306}\u{0301}"), "ă");
        assert_eq!(toneless("o\u{0302}\u{0300}"), "ô");
    }

    // ---- completions ----

    #[test]
    fn completions_rank_by_frequency_and_exclude_the_word() {
        let t = SuggestTable::from_rows(rows(&[
            ("thương", 30),
            ("thôi", 20),
            ("trước", 10),
            ("thì", 5),
        ]));
        // trước (toneless "trước") does not start with "th".
        assert_eq!(
            t.completions("th"),
            vec![
                Candidate {
                    word: "thương".into(),
                    freq: 30
                },
                Candidate {
                    word: "thôi".into(),
                    freq: 20
                },
                Candidate {
                    word: "thì".into(),
                    freq: 5
                },
            ]
        );
        assert_eq!(
            t.completions("tr"),
            vec![Candidate {
                word: "trước".into(),
                freq: 10
            }]
        );
        // The typed word itself never comes back.
        assert!(t.completions("thôi").is_empty());
        assert!(t.completions("trước").is_empty());
    }

    #[test]
    fn completions_reach_wrong_tone_spellings_through_the_toneless_prefix() {
        // sỏt is not a table row; its toneless form "sot" prefixes sót and
        // sọt. sốt (toneless "sôt") is a distance-1 neighbor, not a prefix
        // hit — a different mechanism would have to offer it.
        assert_eq!(
            SuggestTable::embedded().completions("sỏt"),
            vec![
                Candidate {
                    word: "sót".into(),
                    freq: 1257
                },
                Candidate {
                    word: "sọt".into(),
                    freq: 48
                },
            ]
        );
    }

    #[test]
    fn completions_cap_at_25() {
        let mut list = vec![("a".to_string(), 100u32)];
        for i in 1..=29u32 {
            list.push((format!("a{i}"), i));
        }
        let t = SuggestTable::from_rows(list);
        let got = t.completions("a");
        // 29 candidates ("a" itself excluded), capped to the top 25 by freq.
        assert_eq!(got.len(), 25);
        assert_eq!(
            got[0],
            Candidate {
                word: "a29".into(),
                freq: 29
            }
        );
        assert_eq!(
            got[24],
            Candidate {
                word: "a5".into(),
                freq: 5
            }
        );
        assert!(got.iter().all(|c| c.word != "a"));
    }

    // ---- corrections ----

    #[test]
    fn corrections_rank_by_distance_then_frequency() {
        let t = SuggestTable::from_rows(rows(&[("sót", 10), ("sốt", 30), ("sao", 100)]));
        // No toneless form starts with "srot". "sot" (sót) is one edit away,
        // "sao" and "sôt" (sốt) two — distance outranks sao's higher freq.
        assert_eq!(
            t.corrections("srot"),
            vec![
                Candidate {
                    word: "sót".into(),
                    freq: 10
                },
                Candidate {
                    word: "sao".into(),
                    freq: 100
                },
                Candidate {
                    word: "sốt".into(),
                    freq: 30
                },
            ]
        );
    }

    #[test]
    fn corrections_transpositions_count_as_one_edit() {
        // abcd -> badc is two adjacent swaps: Damerau 2, plain Levenshtein 3,
        // so this only passes with transpositions in the distance.
        let t = SuggestTable::from_rows(rows(&[("badc", 5)]));
        assert_eq!(
            t.corrections("abcd"),
            vec![Candidate {
                word: "badc".into(),
                freq: 5
            }]
        );
    }

    #[test]
    fn corrections_are_a_pure_distance_primitive() {
        // The additive ruling moved the gating into candidates(): corrections
        // lists its distance-2 neighbors even behind a non-empty completion
        // set. Distance 0 first (sót, sọt share sỏt's "sot" key), then the
        // distance-1 group by frequency — sát leads it, sốt ("sôt") is in.
        let got = SuggestTable::embedded().corrections("sỏt");
        assert_eq!(
            got[..3],
            [
                Candidate {
                    word: "sót".into(),
                    freq: 1257
                },
                Candidate {
                    word: "sọt".into(),
                    freq: 48
                },
                Candidate {
                    word: "sát".into(),
                    freq: 12536
                },
            ]
        );
        assert!(got.iter().any(|c| c.word == "sốt" && c.freq == 1969));
        // The typed word itself is never its own correction.
        let t = SuggestTable::from_rows(rows(&[("sót", 10), ("sốt", 30)]));
        assert_eq!(
            t.corrections("sót"),
            vec![Candidate {
                word: "sốt".into(),
                freq: 30
            }]
        );
    }

    #[test]
    fn candidates_mix_completions_then_corrections_for_unknown_words() {
        // The ruled list: completions first, corrections behind, deduped.
        // sót is both a completion and a distance-0 correction — it must
        // appear exactly once; sốt ("sôt") is the distance-1 shape neighbor.
        let t = SuggestTable::from_rows(rows(&[("sót", 10), ("sốt", 30)]));
        assert_eq!(
            t.candidates("sỏt"),
            vec![
                Candidate {
                    word: "sót".into(),
                    freq: 10
                },
                Candidate {
                    word: "sốt".into(),
                    freq: 30
                },
            ]
        );
        // The same shape on the real table: the spec's sỏt -> sốt example —
        // completions sót/sọt first, then the correction group led by sát,
        // with sốt visible inside the 25.
        let got = SuggestTable::embedded().candidates("sỏt");
        assert_eq!(got.len(), 25);
        assert_eq!(
            got[..3],
            [
                Candidate {
                    word: "sót".into(),
                    freq: 1257
                },
                Candidate {
                    word: "sọt".into(),
                    freq: 48
                },
                Candidate {
                    word: "sát".into(),
                    freq: 12536
                },
            ]
        );
        assert!(got.iter().any(|c| c.word == "sốt" && c.freq == 1969));
    }

    #[test]
    fn candidates_leave_known_words_at_completions_only() {
        // "tha" is known: its list is completions only — the distance-1
        // neighbor "thua" (and "tha" itself) must not appear behind it.
        let t = SuggestTable::from_rows(rows(&[("tha", 50), ("thang", 30), ("thua", 20)]));
        assert_eq!(
            t.candidates("tha"),
            vec![Candidate {
                word: "thang".into(),
                freq: 30
            }]
        );
        // nghiêng is known with nothing completing it: the quiet empty list.
        assert!(SuggestTable::embedded().completions("nghiêng").is_empty());
        assert!(SuggestTable::embedded().contains("nghiêng"));
        assert!(SuggestTable::embedded().candidates("nghiêng").is_empty());
    }

    #[test]
    fn candidates_cap_across_the_union() {
        // One completion (abcd0) plus thirty distance-2 corrections (abcfX):
        // the union caps at 25 with completions keeping their places.
        let mut list = vec![("abcd0".to_string(), 1000u32)];
        let suffixes = "0123456789abcefghijklmnopqrstu"; // 30 one-edit extensions
        list.extend(
            suffixes
                .chars()
                .enumerate()
                .map(|(i, s)| (format!("abcf{s}"), (i + 1) as u32)),
        );
        let t = SuggestTable::from_rows(list);
        let got = t.candidates("abcd");
        assert_eq!(got.len(), 25);
        assert_eq!(
            got[0],
            Candidate {
                word: "abcd0".into(),
                freq: 1000
            }
        );
        assert_eq!(
            got[1],
            Candidate {
                word: "abcfu".into(),
                freq: 30
            }
        );
        assert_eq!(
            got[24],
            Candidate {
                word: "abcf6".into(),
                freq: 7
            }
        );
    }

    #[test]
    fn corrections_on_embedded_rank_distance_then_frequency_and_cap() {
        // "srot" mangles no real syllable, so the fallback fires: six
        // distance-1 rows (the "sot"/"rot"/"trot" toneless forms) first,
        // distance-2 rows behind, capped at 25 of the 153 candidates.
        let got = SuggestTable::embedded().corrections("srot");
        assert_eq!(got.len(), 25);
        assert_eq!(
            got[0],
            Candidate {
                word: "sót".into(),
                freq: 1257
            }
        );
        assert_eq!(
            got[1],
            Candidate {
                word: "rót".into(),
                freq: 234
            }
        );
        assert_eq!(
            got[5],
            Candidate {
                word: "rọt".into(),
                freq: 24
            }
        );
        assert_eq!(
            got[6],
            Candidate {
                word: "rất".into(),
                freq: 47841
            }
        );
    }

    #[test]
    fn corrections_find_nothing_for_unmatchable_words() {
        // No Vietnamese toneless form is within distance 2 of either probe;
        // both walks exercise the early abort.
        assert!(SuggestTable::embedded().corrections("wjhkr").is_empty());
        assert!(SuggestTable::embedded().corrections("aaaaaaa").is_empty());
    }

    #[test]
    fn corrections_cap_at_25() {
        // 30 one-edit extensions, with 'd' left out: "abcfd" would sit one
        // insertion from "abcd" and rank ahead at distance 1.
        let suffixes = "0123456789abcefghijklmnopqrstu";
        let list: Vec<(String, u32)> = suffixes
            .chars()
            .enumerate()
            .map(|(i, s)| (format!("abcf{s}"), (i + 1) as u32))
            .collect();
        let t = SuggestTable::from_rows(list);
        // Every candidate is two edits from "abcd" (d->f plus one insert) and
        // none extends the "abcd" prefix, so the fallback fires.
        let got = t.corrections("abcd");
        assert_eq!(got.len(), 25);
        assert_eq!(
            got[0],
            Candidate {
                word: "abcfu".into(),
                freq: 30
            }
        );
        assert_eq!(
            got[24],
            Candidate {
                word: "abcf5".into(),
                freq: 6
            }
        );
    }

    // ---- contains ----

    #[test]
    fn contains_checks_exact_dictionary_membership() {
        let t = SuggestTable::embedded();
        assert!(t.contains("thì"));
        assert!(t.contains("nghiêng"));
        // The rows are lowercase; the typed word folds its case first.
        assert!(t.contains("THÌ"));
        // A tone-mangled or bare spelling is not a row even though its
        // toneless form matches one — that is what keeps sỏt eligible for
        // its distance-1 shape neighbor sốt in candidates().
        assert!(!t.contains("sỏt"));
        assert!(!t.contains("sot"));
        assert!(!t.contains("srot"));
        assert!(!t.contains(""));
    }

    // ---- merge ----

    #[test]
    fn merge_user_entries_weight_and_dedup() {
        assert_eq!(
            merge_user_entries(rows(&[("thang", 1), ("thang", 9)])),
            vec![("thang".to_string(), u32::MAX / 2)]
        );
        // An empty word would prefix-match everything downstream; drop it.
        assert!(merge_user_entries(vec![("".to_string(), 1)]).is_empty());
    }

    #[test]
    fn merged_user_weight_beats_builtin_frequency() {
        let mut list = rows(&[("thanh", 500)]);
        list.extend(merge_user_entries(rows(&[("thang", 1)])));
        let t = SuggestTable::from_rows(list);
        assert_eq!(
            t.completions("than"),
            vec![
                Candidate {
                    word: "thang".into(),
                    freq: u32::MAX / 2
                },
                Candidate {
                    word: "thanh".into(),
                    freq: 500
                },
            ]
        );
    }

    #[test]
    fn from_rows_keeps_one_row_per_word_at_the_highest_frequency() {
        // The runtime table is builtin rows plus user rows: a word the user
        // also has must not be offered twice.
        let mut list = rows(&[("thôi", 5375)]);
        list.extend(merge_user_entries(rows(&[("thôi", 1)])));
        let t = SuggestTable::from_rows(list);
        assert_eq!(
            t.completions("thô"),
            vec![Candidate {
                word: "thôi".into(),
                freq: u32::MAX / 2
            }]
        );
    }
}
