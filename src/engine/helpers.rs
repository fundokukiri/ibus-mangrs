//! Free helper functions shared across the engine submodules.

use std::time::SystemTime;

use super::Mode;

/// Modes whose tail rewrite deletes with key events forwarded through IBus
/// (`ForwardKeyEvent` signals) instead of the protocol delete or `/dev/uinput`.
/// They need neither a surrounding-text report nor the virtual device, which
/// makes them the only workable delete transport in XIM clients (their input
/// context reports no capabilities at all) and in mutter-relayed Wayland
/// contexts. Keep this the single place the two variants are recognised, or
/// one of them silently stops deleting.
pub(super) fn forwards_delete(mode: Mode) -> bool {
    matches!(mode, Mode::ForwardKeyEvent | Mode::ForwardSelect)
}

/// mtime of `path`, or None when it does not exist — the reload gate for
/// the auxiliary settings files.
pub(super) fn file_time(path: Option<&std::path::Path>) -> Option<SystemTime> {
    path.and_then(|p| std::fs::metadata(p).ok())
        .and_then(|m| m.modified().ok())
}

/// Uppercase the first character, leaving the rest untouched (Vietnamese
/// words may start with a transformed letter: `ă` → `Ă`).
pub(super) fn capitalize_first(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) => c.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// Drop `delete` characters from the end of `tail` and append `insert`.
pub(super) fn replace_tail(tail: &str, delete: usize, insert: &str) -> String {
    let keep = tail.chars().count().saturating_sub(delete);
    let mut s: String = tail.chars().take(keep).collect();
    s.push_str(insert);
    s
}

pub(super) fn starts_with(haystack: &[char], needle: &[char]) -> bool {
    haystack.len() >= needle.len() && &haystack[..needle.len()] == needle
}

pub(super) fn ends_with(haystack: &[char], needle: &[char]) -> bool {
    haystack.len() >= needle.len() && &haystack[haystack.len() - needle.len()..] == needle
}

pub(super) fn contains_sub(haystack: &[char], needle: &[char]) -> bool {
    haystack.windows(needle.len().max(1)).any(|w| w == needle)
}

/// Distance from the end of the last occurrence of `needle` to the end of
/// `haystack`, or `None` when it does not occur.
pub(super) fn suffix_gap(haystack: &[char], needle: &[char]) -> Option<usize> {
    if needle.is_empty() || haystack.len() < needle.len() {
        return None;
    }
    haystack
        .windows(needle.len())
        .rposition(|w| w == needle)
        .map(|i| haystack.len() - (i + needle.len()))
}

pub(super) fn common_suffix_len(a: &[char], b: &[char]) -> usize {
    a.iter()
        .rev()
        .zip(b.iter().rev())
        .take_while(|(x, y)| x == y)
        .count()
}
