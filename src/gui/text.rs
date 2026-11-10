//! Editable GUI content. The user-visible texts live in
//! `data/gui-content.toml` (Vietnamese, the install default) and
//! `data/gui-content.en.toml` (English), compiled in with `include_str!` —
//! the owner edits those files and rebuilds without touching code. Every
//! lookup goes through `t(key, default)`: the table of the per-process
//! language first, then the code default, so the interface can never go
//! blank.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::OnceLock;

const CONTENT: &str = include_str!("../../data/gui-content.toml");
const CONTENT_EN: &str = include_str!("../../data/gui-content.en.toml");

static TABLE: OnceLock<toml::Table> = OnceLock::new();
static TABLE_EN: OnceLock<toml::Table> = OnceLock::new();
/// 0 = Vietnamese, 1 = English. Anything the config carries besides "en"
/// maps back to Vietnamese.
static LANG: AtomicU8 = AtomicU8::new(0);

fn load_table(content: &str, cell: &'static OnceLock<toml::Table>) -> &'static toml::Table {
    cell.get_or_init(|| {
        toml::from_str(content).unwrap_or_else(|e| {
            // Not the engine path: the GUI can afford an eprintln, and an
            // empty table keeps every row on its code default.
            eprintln!("gui-content: {e}");
            toml::Table::new()
        })
    })
}

fn table() -> &'static toml::Table {
    load_table(CONTENT, &TABLE)
}

fn table_en() -> &'static toml::Table {
    load_table(CONTENT_EN, &TABLE_EN)
}

/// The process language: the settings window sets it from the config at
/// startup and again after a language pick, before the rebuild.
pub(crate) fn set_language(lang: &str) {
    LANG.store(u8::from(lang == "en"), Ordering::Relaxed);
}

#[cfg(test)]
fn parsed(lang: usize) -> &'static toml::Table {
    if lang == 1 {
        table_en()
    } else {
        table()
    }
}

/// The text for `key` ("section.entry.sub") or `default` when the content
/// file does not define it.
pub(crate) fn t(key: &str, default: &str) -> String {
    let mut node: &toml::Table = if LANG.load(Ordering::Relaxed) == 1 {
        table_en()
    } else {
        table()
    };
    let parts: Vec<&str> = key.split('.').collect();
    for (i, part) in parts.iter().enumerate() {
        match node.get(*part) {
            Some(toml::Value::Table(next)) if i + 1 < parts.len() => node = next,
            Some(toml::Value::String(s)) if i + 1 == parts.len() => return s.clone(),
            _ => return default.to_string(),
        }
    }
    default.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::Config;

    #[test]
    fn t_respects_the_language_and_falls_back_to_vi() {
        set_language("en");
        assert_eq!(t("pages.other", ""), "Other settings");
        set_language("vi");
        assert_eq!(t("pages.other", ""), "Cài đặt khác");
        // Anything unknown ("fr", "", garbage) behaves as Vietnamese.
        set_language("fr");
        assert_eq!(t("pages.other", ""), "Cài đặt khác");
        set_language("vi"); // restore for other tests; global state
    }

    #[test]
    fn config_defaults_to_vietnamese() {
        assert_eq!(Config::default().language, "vi");
    }

    /// The parity contract: every key the Vietnamese file defines must have
    /// an English entry, so switching languages can never strand a string.
    #[test]
    fn every_vi_key_has_an_en_entry() {
        fn keys(t: &toml::Table, prefix: &str, out: &mut Vec<String>) {
            for (k, v) in t {
                let path = format!("{prefix}{k}");
                if let Some(sub) = v.as_table() {
                    keys(sub, &format!("{path}."), out);
                } else {
                    out.push(path);
                }
            }
        }
        let (mut vi_keys, mut en_keys) = (Vec::new(), Vec::new());
        keys(parsed(0), "", &mut vi_keys);
        keys(parsed(1), "", &mut en_keys);
        vi_keys.sort();
        en_keys.sort();
        let missing: Vec<&String> = vi_keys
            .iter()
            .filter(|k| en_keys.binary_search(k).is_err())
            .collect();
        assert!(
            missing.is_empty(),
            "keys missing from the EN file: {missing:?}"
        );
    }
}
