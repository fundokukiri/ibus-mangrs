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

/// A merged row for the picker: the embedded entry with the user's glyph,
/// or a user-only entry with no keywords (search falls back to the name).
pub struct PickerEntry {
    pub name: String,
    pub glyph: String,
    pub keywords: Vec<String>,
}

/// The full picker list: the embedded table in its order, then user-only
/// names, with user glyphs applied — by row name or by any of its aliases
/// (`smile = "🙂"` overrides the grinning-face row).
pub fn picker_entries(user: &BTreeMap<String, String>) -> Vec<PickerEntry> {
    let mut out: Vec<PickerEntry> = embedded()
        .iter()
        .map(|row| {
            let glyph = user
                .get(&row.name)
                .or_else(|| row.aliases.iter().find_map(|a| user.get(a)))
                .cloned()
                .unwrap_or_else(|| row.glyph.clone());
            PickerEntry {
                glyph,
                name: row.name.clone(),
                keywords: row.keywords.clone(),
            }
        })
        .collect();
    for (name, glyph) in user {
        if !out.iter().any(|e| &e.name == name) {
            out.push(PickerEntry {
                name: name.clone(),
                glyph: glyph.clone(),
                keywords: Vec::new(),
            });
        }
    }
    out
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
        let entries = picker_entries(&u.map);
        // An alias override lands on its canonical row; the keywords of the
        // embedded row survive (the search keeps working).
        let smile = entries
            .iter()
            .find(|e| e.name == "grinning_face_with_smiling_eyes")
            .unwrap();
        assert_eq!(smile.glyph, "🙂");
        assert!(!smile.keywords.is_empty(), "keywords survive the override");
        let vnu = entries.iter().find(|e| e.name == "vnu").unwrap();
        assert_eq!(vnu.glyph, "🇻🇳 custom");
        assert!(vnu.keywords.is_empty(), "user-only rows search by name");
    }
}
