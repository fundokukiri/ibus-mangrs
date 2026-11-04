//! Editable GUI content. The user-visible texts live in
//! `data/gui-content.toml`, compiled in with `include_str!` — the owner
//! edits that file and rebuilds without touching code. Every lookup goes
//! through `t(key, default)`: a missing or mistyped key yields the code
//! default, so the interface can never go blank.

use std::sync::OnceLock;

const CONTENT: &str = include_str!("../../data/gui-content.toml");

static TABLE: OnceLock<toml::Table> = OnceLock::new();

fn table() -> &'static toml::Table {
    TABLE.get_or_init(|| {
        toml::from_str(CONTENT).unwrap_or_else(|e| {
            // Not the engine path: the GUI can afford an eprintln, and an
            // empty table keeps every row on its code default.
            eprintln!("gui-content.toml: {e}");
            toml::Table::new()
        })
    })
}

/// The text for `key` ("section.entry.sub") or `default` when the content
/// file does not define it.
pub(crate) fn t(key: &str, default: &str) -> String {
    let mut node: &toml::Table = table();
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
