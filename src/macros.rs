//! Macro table and custom dictionary, stored in their own files next to
//! `config.toml`:
//!
//! * `macros.toml` — `key = "expansion"` pairs. `$TIME` and `$DATE` inside an
//!   expansion are replaced with the formatted current time/date.
//! * `dictionary.txt` — one word per line; words listed here are never
//!   auto-restored to raw keystrokes.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use chrono::Local;

pub struct Macros {
    map: BTreeMap<String, String>,
}

impl Macros {
    pub(crate) fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ibus-mangrs").join("macros.toml"))
    }

    pub fn load() -> Self {
        let map = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .and_then(|text| toml::from_str::<BTreeMap<String, String>>(&text).ok())
            .unwrap_or_default();
        Self { map }
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

    /// Expand `raw` if it is a macro key. With `capitalize` the key is matched
    /// case-insensitively and the expansion gets its first letter capitalized
    /// (`DD` -> `Đại đội`); an all-uppercase key uppercases the expansion.
    pub fn expand(
        &self,
        raw: &str,
        capitalize: bool,
        time_format: &str,
        date_format: &str,
    ) -> Option<String> {
        let (exp, cap_mode) = if let Some(exp) = self.map.get(raw) {
            (exp, CapMode::None)
        } else if capitalize {
            let lower = raw.to_lowercase();
            let exp = self.map.get(&lower)?;
            let cap_mode = if raw.chars().all(|c| c.is_uppercase() || !c.is_alphabetic()) {
                CapMode::All
            } else {
                CapMode::First
            };
            (exp, cap_mode)
        } else {
            return None;
        };

        let now = Local::now();
        let exp = exp
            .replace("$TIME", &now.format(time_format).to_string())
            .replace("$DATE", &now.format(date_format).to_string());

        Some(match cap_mode {
            CapMode::None => exp,
            CapMode::First => exp
                .char_indices()
                .next()
                .map(|(i, c)| {
                    let mut s = String::new();
                    s.extend(c.to_uppercase());
                    s + &exp[i + c.len_utf8()..]
                })
                .unwrap_or(exp),
            CapMode::All => exp.to_uppercase(),
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CapMode {
    None,
    First,
    All,
}

/// The custom dictionary: lowercase words that must never be auto-restored.
pub struct Dictionary {
    pub(crate) words: HashSet<String>,
}

impl Dictionary {
    pub(crate) fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ibus-mangrs").join("dictionary.txt"))
    }

    pub fn load() -> Self {
        let words = Self::path()
            .and_then(|p| std::fs::read_to_string(p).ok())
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

    pub fn words(&self) -> impl Iterator<Item = &str> {
        self.words.iter().map(String::as_str)
    }

    pub fn set_words(&mut self, words: impl Iterator<Item = String>) {
        self.words = words
            .map(|w| w.trim().to_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
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
        let text = self.words.iter().cloned().collect::<Vec<_>>().join("\n");
        std::fs::write(path, text)
    }

    pub fn contains(&self, word: &str) -> bool {
        self.words.contains(&word.to_lowercase())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn macros(entries: &[(&str, &str)]) -> Macros {
        Macros {
            map: entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect(),
        }
    }

    #[test]
    fn plain_macro_expands() {
        let m = macros(&[("ddq", "đại đội trưởng")]);
        assert_eq!(
            m.expand("ddq", false, "%H:%M", "%d/%m/%Y").as_deref(),
            Some("đại đội trưởng")
        );
    }

    #[test]
    fn capitalize_macro_uppercases_first_letter() {
        let m = macros(&[("dđ", "đại đội")]);
        // Mixed-case key: first letter of the expansion is capitalized.
        assert_eq!(
            m.expand("Dđ", true, "%H:%M", "%d/%m/%Y").as_deref(),
            Some("Đại đội")
        );
    }

    #[test]
    fn all_caps_macro_uppercases_everything() {
        let m = macros(&[("vn", "việt nam")]);
        assert_eq!(
            m.expand("VN", true, "%H:%M", "%d/%m/%Y").as_deref(),
            Some("VIỆT NAM")
        );
    }

    #[test]
    fn capitalize_disabled_matches_exactly() {
        let m = macros(&[("dd", "đại đội")]);
        assert_eq!(m.expand("DD", false, "%H:%M", "%d/%m/%Y"), None);
    }

    #[test]
    fn time_and_date_expand() {
        let m = macros(&[("now", "t=$TIME d=$DATE")]);
        // The formats contain no literal `$`, so `$TIME`/`$DATE` must be gone.
        let exp = m.expand("now", false, "%H", "%d").unwrap();
        assert!(!exp.contains('$'), "{exp}");
        assert!(exp.starts_with("t="));
        assert!(exp.contains(" d="));
    }

    #[test]
    fn dictionary_is_case_insensitive() {
        let mut d = Dictionary {
            words: HashSet::new(),
        };
        d.set_words(["GitHub"].iter().map(|s| s.to_string()));
        assert!(d.contains("github"));
        assert!(d.contains("GitHub"));
        assert!(!d.contains("gitlab"));
    }
}
