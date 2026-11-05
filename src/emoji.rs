//! Emoji shortcodes, in two tables next to `config.toml` and `macros.toml`:
//!
//! * the embedded table (`data/emoji.toml`, generated once by
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
        const DATA: &str = include_str!("../data/emoji.toml");
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
                for word in fold_ascii(&keyword).split_whitespace() {
                    blob.push(' ');
                    blob.push_str(word);
                }
            }
            SearchRow { name, glyph, blob }
        })
        .collect()
}

/// Fold Vietnamese diacritics to their ASCII base and lowercase. The
/// picker's search runs on what the user types — bare letter keys — so
/// `cuoi` must find "cười".
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
    s.to_lowercase().chars().map(fold).collect()
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
        std::fs::write(
            path,
            toml::to_string_pretty(&self.map).map_err(std::io::Error::other)?,
        )
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
    fn fold_ascii_strips_vietnamese_diacritics() {
        assert_eq!(fold_ascii("Cười"), "cuoi");
        assert_eq!(fold_ascii("đặt"), "dat");
        assert_eq!(fold_ascii("Fire!"), "fire!");
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
}
