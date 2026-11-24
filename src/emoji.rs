//! Emoji shortcodes, in two tables next to `config.toml` and `macros.toml`:
//!
//! * the embedded table (`data/contents/emoji.toml`, generated once by
//!   `tools/gen_emoji.py` from the CLDR annotations) — ~1.9k emoji with
//!   Vietnamese and English keywords, compiled into the binary with
//!   `include_str!` so no data file needs installing;
//! * `emoji.toml` in the user config directory — `name = "glyph"` pairs,
//!   exactly the shape of `macros.toml`. A name from the embedded table is
//!   overridden, a new name is added.
//!
//! Two consumers share this module: the engine's `:name:` shortcode typing
//! (lookup only) and the `-emoji` picker window (the full list plus
//! keywords for the search).

use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::OnceLock;

/// One row of the embedded table. Keywords feed the picker's search only;
/// the engine lookup goes through the name map below.
#[derive(serde::Deserialize)]
pub struct EmojiRow {
    pub name: String,
    pub glyph: String,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
}

#[derive(serde::Deserialize)]
struct EmojiFile {
    emoji: Vec<EmojiRow>,
}

/// The embedded table, parsed once per process on first use (the engine
/// pays it only when the user opens the picker or enables shortcodes).
pub fn embedded() -> &'static [EmojiRow] {
    static EMBEDDED: OnceLock<Vec<EmojiRow>> = OnceLock::new();
    EMBEDDED.get_or_init(|| {
        const DATA: &str = include_str!("../data/contents/emoji.toml");
        toml::from_str::<EmojiFile>(DATA)
            .map(|f| f.emoji)
            .unwrap_or_default()
    })
}

/// Every name and alias mapped to its glyph — the engine's lookup shape.
/// A later duplicate would win, but the generator refuses collisions, and
/// user names are checked before this map anyway.
pub fn embedded_names() -> &'static HashMap<String, String> {
    static NAMES: OnceLock<HashMap<String, String>> = OnceLock::new();
    NAMES.get_or_init(|| {
        embedded()
            .iter()
            .flat_map(|row| {
                std::iter::once((&row.name, &row.glyph))
                    .chain(row.aliases.iter().map(|a| (a, &row.glyph)))
            })
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect()
    })
}

/// One searchable row of the picker: name, glyph and the folded search
/// blob (name + aliases + keywords, lowercase, diacritics folded).
pub struct SearchRow {
    pub name: String,
    pub glyph: String,
    pub blob: String,
}

/// The picker's search list: the embedded table in its order (so the
/// common smileys lead when nothing was picked yet), then user-only
/// names, with user glyphs applied — by row name or by any of its aliases
/// (`smile = "🙂"` overrides the grinning-face row).
pub fn search_rows(user: &BTreeMap<String, String>) -> Vec<SearchRow> {
    let mut rows: Vec<(String, String, Vec<String>)> = embedded()
        .iter()
        .map(|row| {
            let glyph = user
                .get(&row.name)
                .or_else(|| row.aliases.iter().find_map(|a| user.get(a)))
                .cloned()
                .unwrap_or_else(|| row.glyph.clone());
            let mut keywords = row.keywords.clone();
            keywords.extend(row.aliases.iter().cloned());
            (row.name.clone(), glyph, keywords)
        })
        .collect();
    for (name, glyph) in user {
        if !rows.iter().any(|(n, _, _)| n == name) {
            rows.push((name.clone(), glyph.clone(), Vec::new()));
        }
    }
    rows.into_iter()
        .map(|(name, glyph, keywords)| {
            let mut blob = fold_ascii(&name);
            for keyword in keywords {
                // fold_ascii has stripped the spaces: one keyword, one
                // token, joined by a separator the query can never match.
                blob.push(' ');
                blob.push_str(&fold_ascii(&keyword));
            }
            SearchRow { name, glyph, blob }
        })
        .collect()
}

/// Fold Vietnamese diacritics to their ASCII base, lowercase, and drop
/// spaces and underscores. The picker's search runs without word
/// boundaries — bare letter keys — so `cuoi` finds "cười", `nuocmy` finds
/// "nước mỹ" and `united states` finds "flag_united_states".
pub fn fold_ascii(s: &str) -> String {
    fn fold(c: char) -> char {
        match c {
            'à' | 'á' | 'ả' | 'ã' | 'ạ' | 'ă' | 'ằ' | 'ắ' | 'ẳ' | 'ẵ' | 'ặ' | 'â' | 'ầ' | 'ấ'
            | 'ẩ' | 'ẫ' | 'ậ' => 'a',
            'è' | 'é' | 'ẻ' | 'ẽ' | 'ẹ' | 'ê' | 'ề' | 'ế' | 'ể' | 'ễ' | 'ệ' => {
                'e'
            }
            'ì' | 'í' | 'ỉ' | 'ĩ' | 'ị' => 'i',
            'ò' | 'ó' | 'ỏ' | 'õ' | 'ọ' | 'ô' | 'ồ' | 'ố' | 'ổ' | 'ỗ' | 'ộ' | 'ơ' | 'ờ' | 'ớ'
            | 'ở' | 'ỡ' | 'ợ' => 'o',
            'ù' | 'ú' | 'ủ' | 'ũ' | 'ụ' | 'ư' | 'ừ' | 'ứ' | 'ử' | 'ữ' | 'ự' => {
                'u'
            }
            'ỳ' | 'ý' | 'ỷ' | 'ỹ' | 'ỵ' => 'y',
            'đ' => 'd',
            _ => c,
        }
    }
    s.to_lowercase()
        .chars()
        .filter(|c| !matches!(c, ' ' | '_'))
        .map(fold)
        .collect()
}

/// How many emoji glyphs the suggestion strip offers per list — a keyword
/// like `cười` sits on dozens of rows, and the strip's one row must not
/// flood with faces.
pub const STRIP_EMOJI_CAP: usize = 3;

/// The emoji glyphs whose name, an alias or a keyword folds (see
/// `fold_ascii`) to EXACTLY the typed word's fold — `meo` reaches the cat
/// through the keyword `mèo`, `cuoi` the laughing faces via `cười`. Only
/// single-word keys can equal one typed word, so multi-word keywords are
/// not indexed. The index builds once on first strip use; the query is one
/// fold plus a hash lookup (the engine's key path).
pub fn glyphs_for_word(word: &str) -> Vec<&'static str> {
    static INDEX: OnceLock<HashMap<String, Vec<&'static str>>> = OnceLock::new();
    let index = INDEX.get_or_init(|| {
        let mut map: HashMap<String, Vec<&'static str>> = HashMap::new();
        for row in embedded() {
            for key in std::iter::once(&row.name)
                .chain(row.aliases.iter())
                .chain(row.keywords.iter())
            {
                if key.chars().any(|c| c == ' ' || c == '_') {
                    continue;
                }
                let glyphs = map.entry(fold_ascii(key)).or_default();
                if !glyphs.contains(&row.glyph.as_str()) {
                    glyphs.push(row.glyph.as_str());
                }
            }
        }
        map
    });
    index.get(&fold_ascii(word)).cloned().unwrap_or_default()
}

/// The emoji recently picked through the picker, newest first — its first
/// page. One name per line in `~/.config/ibus-mangrs/emoji_recent.txt`
/// (names, not glyphs: a later user-table override keeps applying).
pub struct RecentEmoji {
    names: Vec<String>,
}

/// How many picks the recents list remembers — one picker page.
pub const RECENT_CAP: usize = 10;

impl RecentEmoji {
    pub(crate) fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ibus-mangrs").join("emoji_recent.txt"))
    }

    pub fn load() -> Self {
        let names = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| {
                text.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .take(RECENT_CAP)
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        Self { names }
    }

    pub fn names(&self) -> &[String] {
        &self.names
    }

    /// Record a pick: to the front, deduped, capped, then saved (best
    /// effort — a failed write only loses the recency order).
    pub fn push(name: &str) {
        let mut list = Self::load().names;
        push_name(&mut list, name);
        if let Some(path) = Self::path() {
            if let Some(parent) = path.parent() {
                let _ = std::fs::create_dir_all(parent);
            }
            let _ = std::fs::write(path, list.join("\n"));
        }
    }
}

/// The pure part of `push`, seam for the tests: the developer's real
/// recents file must never be touched by a test run.
fn push_name(list: &mut Vec<String>, name: &str) {
    list.retain(|n| n != name);
    list.insert(0, name.to_string());
    list.truncate(RECENT_CAP);
}

/// The user's own table, `~/.config/ibus-mangrs/emoji.toml` — the same
/// `name = "glyph"` pairs as `macros.toml`.
pub struct UserEmoji {
    pub(crate) map: BTreeMap<String, String>,
}

impl UserEmoji {
    pub(crate) fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ibus-mangrs").join("emoji.toml"))
    }

    pub fn load() -> Self {
        let map = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| toml::from_str::<BTreeMap<String, String>>(&text).ok())
            .unwrap_or_default();
        Self { map }
    }

    /// The shortcode for `name`: the user's table wins over the embedded
    /// one (override or addition), so the user can fix or extend entries
    /// without regenerating anything.
    pub fn lookup(&self, name: &str) -> Option<&str> {
        if let Some(glyph) = self.map.get(name) {
            return Some(glyph);
        }
        embedded_names().get(name).map(String::as_str)
    }

    pub fn entries(&self) -> impl Iterator<Item = (&str, &str)> {
        self.map.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }

    pub fn set_entries(&mut self, entries: impl Iterator<Item = (String, String)>) {
        self.map = entries.collect();
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no config dir",
            ));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(&self.map).map_err(std::io::Error::other)?;
        // Unchanged content: skip the write (see the save-round note in
        // config.rs).
        if std::fs::read_to_string(&path).unwrap_or_default() == text {
            return Ok(());
        }
        std::fs::write(path, text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(entries: &[(&str, &str)]) -> UserEmoji {
        UserEmoji {
            map: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn embedded_table_parses_and_is_populated() {
        let rows = embedded();
        assert!(rows.len() > 1500, "{} rows", rows.len());
        assert!(embedded_names().contains_key("grinning_face_with_smiling_eyes"));
        assert!(embedded_names().contains_key("smile"));
        // The flags live in the CLDR derived annotations, not the main
        // ones; a table built from the main file alone silently drops
        // them all.
        assert!(embedded_names().contains_key("flag_vietnam"));
        assert_eq!(embedded_names().get("vn").map(String::as_str), Some("🇻🇳"));
        // Curated out as non-emoji (punctuation, math operators); a
        // re-run of the bootstrap must not resurrect them.
        assert!(!embedded_names().contains_key("en_dash"));
    }

    #[test]
    fn lookup_prefers_the_user_table() {
        let u = user(&[("smile", "🙂")]);
        assert_eq!(u.lookup("smile"), Some("🙂"));
        // An embedded name the user did not touch.
        assert_eq!(u.lookup("fire"), Some("🔥"));
        assert_eq!(u.lookup("nope"), None);
    }

    #[test]
    fn picker_merges_user_rows() {
        let u = user(&[("smile", "🙂"), ("vnu", "🇻🇳 custom")]);
        let rows = search_rows(&u.map);
        // An alias override lands on its canonical row.
        let smile = rows
            .iter()
            .find(|r| r.name == "grinning_face_with_smiling_eyes")
            .unwrap();
        assert_eq!(smile.glyph, "🙂");
        assert!(smile.blob.contains("smile"), "aliases join the blob");
        // The Vietnamese keywords are folded into the blob: bare keys find
        // accented words.
        assert!(smile.blob.contains("cuoi"), "{}", smile.blob);
        // A user-only row searches by its name.
        let vnu = rows.iter().find(|r| r.name == "vnu").unwrap();
        assert_eq!(vnu.glyph, "🇻🇳 custom");
    }

    #[test]
    fn flag_search_finds_the_common_names() {
        // The flags' CLDR keywords are the official names only; the picker
        // must also find them by code and colloquial name — the way users
        // actually type them.
        let rows = search_rows(&BTreeMap::new());
        let found = |q: &str| {
            let q = fold_ascii(q);
            rows.iter()
                .any(|r| r.name == "flag_united_states" && r.blob.contains(&q))
        };
        assert!(found("usa"), "alpha-3 code");
        assert!(found("america"), "english common name");
        assert!(found("nuoc my"), "vietnamese, spaced");
        assert!(found("nuocmy"), "vietnamese, no space");
    }

    #[test]
    fn fold_ascii_strips_vietnamese_diacritics() {
        assert_eq!(fold_ascii("Cười"), "cuoi");
        assert_eq!(fold_ascii("đặt"), "dat");
        assert_eq!(fold_ascii("Fire!"), "fire!");
        // The search runs without word boundaries: a no-space query meets a
        // spaced keyword, and a shortcode name's underscores never block a
        // phrase.
        assert_eq!(fold_ascii("nước mỹ"), "nuocmy");
        assert_eq!(fold_ascii("flag_united_states"), "flagunitedstates");
    }

    #[test]
    fn recents_dedupe_and_cap() {
        let mut list: Vec<String> = Vec::new();
        push_name(&mut list, "fire");
        push_name(&mut list, "smile");
        push_name(&mut list, "fire");
        assert_eq!(list, vec!["fire".to_string(), "smile".to_string()]);
        for i in 0..(RECENT_CAP + 3) {
            push_name(&mut list, &format!("e{i}"));
        }
        assert_eq!(list.len(), RECENT_CAP);
        assert_eq!(list[0], "e12");
    }

    // --- strip keyword lookup ---

    #[test]
    fn glyphs_for_word_reaches_the_vietnamese_keywords() {
        // `meo` reaches the cat family through the keyword `mèo`, `cuoi`
        // the laughing faces via `cười` — the embedded table is static, so
        // the anchors hold on every machine.
        let got = glyphs_for_word("meo");
        assert!(!got.is_empty(), "{got:?}");
        assert!(got.contains(&"🐱"), "{got:?}");
        assert!(
            glyphs_for_word("cuoi")
                .iter()
                .any(|g| g.contains('😃') || g.contains('😄') || g.contains('😀')),
            "the laughing faces answer cuoi"
        );
    }

    #[test]
    fn glyphs_for_word_answers_english_and_names() {
        assert!(
            glyphs_for_word("dog").contains(&"🐶"),
            "{:?}",
            glyphs_for_word("dog")
        );
        assert!(glyphs_for_word("kitty").contains(&"🐱"));
    }

    #[test]
    fn glyphs_for_word_stays_silent_without_an_exact_match() {
        // Substring folds must not leak (the picker searches, the strip
        // matches exactly): an unknown word reaches nothing, and a
        // multi-word keyword can never equal one typed word.
        assert!(glyphs_for_word("qqq").is_empty());
        assert!(glyphs_for_word("meoma").is_empty());
        assert!(glyphs_for_word("").is_empty());
        // `con mèo` is a two-word keyword — not indexed.
        assert!(glyphs_for_word("conmeo").is_empty());
    }
}
