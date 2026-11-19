//! Dictionary suggestion strip support.
//!
//! Task 2 lands the suggestion logic itself; this file currently carries only
//! the build-time validation test for the embedded frequency data table
//! (`data/vietnamese-freq.dict`).

#[cfg(test)]
mod data_tests {
    const FREQ_DATA: &str = include_str!("../data/vietnamese-freq.dict");

    /// Each of the 60 tone-marked vowels maps to its plain vowel; everything
    /// else passes through. The same strip rule the build script implements
    /// by NFD-filtering the five tone marks — horn, hook, circumflex and
    /// breve shapes stay intact. Task 2 owns the real toneless
    /// implementation; the data test only needs the sort order.
    fn toneless_char(c: char) -> char {
        match c {
            'á' | 'à' | 'ả' | 'ã' | 'ạ' => 'a',
            'ắ' | 'ằ' | 'ẳ' | 'ẵ' | 'ặ' => 'ă',
            'ấ' | 'ầ' | 'ẩ' | 'ẫ' | 'ậ' => 'â',
            'é' | 'è' | 'ẻ' | 'ẽ' | 'ẹ' => 'e',
            'ế' | 'ề' | 'ể' | 'ễ' | 'ệ' => 'ê',
            'í' | 'ì' | 'ỉ' | 'ĩ' | 'ị' => 'i',
            'ó' | 'ò' | 'ỏ' | 'õ' | 'ọ' => 'o',
            'ố' | 'ồ' | 'ổ' | 'ỗ' | 'ộ' => 'ô',
            'ớ' | 'ờ' | 'ở' | 'ỡ' | 'ợ' => 'ơ',
            'ú' | 'ù' | 'ủ' | 'ũ' | 'ụ' => 'u',
            'ứ' | 'ừ' | 'ử' | 'ữ' | 'ự' => 'ư',
            'ý' | 'ỳ' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
            other => other,
        }
    }

    fn toneless(s: &str) -> String {
        s.chars().map(toneless_char).collect()
    }

    /// unicode-normalization is not a dependency, so NFC is checked as
    /// membership in the precomposed Vietnamese letter set: a decomposed row
    /// would carry combining marks, which this set rejects. `đ` has no marked
    /// forms and the six plain letters only exist composed.
    fn is_nfc_letter(c: char) -> bool {
        c.is_ascii_lowercase() || "ăâđêôơư".contains(c) || toneless_char(c) != c
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
