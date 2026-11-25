//! The name an App Rule is written with.
//!
//! A window class cannot spell every app: `onlyoffice-desktopeditors` is
//! "ONLYOFFICE" and `org.gnome.Nautilus` is "Files" in the app's own words. The
//! installed `.desktop` entries know, so they are the first answer; the class
//! is only a fallback for apps that ship no entry (a bare binary).

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::OnceLock;

/// The app's own name, or its window class read as one.
pub fn display_name(target: &str) -> String {
    let key = lookup_key(target);
    if !key.is_empty() {
        // The shell is no desktop app, but it owns the keyboard while its
        // search field or Alt+F2 box is up, and a rule can target it.
        if key == "gnome-shell" {
            return "GNOME Shell".to_string();
        }
        if let Some(name) = index().names.get(&key) {
            return name.clone();
        }
    }
    heuristic(target)
}

/// Every directory that holds `*.desktop` entries, most specific first.
fn application_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(data) = dirs::data_dir() {
        dirs.push(data.join("applications"));
    }
    if let Some(home) = dirs::home_dir() {
        dirs.push(home.join(".local/share/flatpak/exports/share/applications"));
    }
    dirs.push(PathBuf::from("/var/lib/flatpak/exports/share/applications"));
    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());
    for dir in data_dirs.split(':').filter(|dir| !dir.is_empty()) {
        dirs.push(PathBuf::from(dir).join("applications"));
    }
    dirs
}

fn index() -> &'static Index {
    static INDEX: OnceLock<Index> = OnceLock::new();
    INDEX.get_or_init(|| build(&application_dirs()))
}

/// The reported names a display name stands for, for a rule written while the
/// picker still saved the readable name (`Files` for `org.gnome.Nautilus`).
pub fn keys_for(text: &str) -> Vec<String> {
    let want = crate::config::canonical_app_name(text);
    index().keys.get(&want).cloned().unwrap_or_default()
}

#[derive(Default)]
struct Index {
    /// Display name by every key an app can be reported under.
    names: HashMap<String, String>,
    /// Every such key, by the canonical display name it belongs to.
    keys: HashMap<String, Vec<String>>,
}

fn build(dirs: &[PathBuf]) -> Index {
    let mut names: HashMap<String, String> = HashMap::new();
    let mut keys: HashMap<String, Vec<String>> = HashMap::new();
    for dir in dirs {
        let Ok(entries) = std::fs::read_dir(dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|ext| ext.to_str()) != Some("desktop") {
                continue;
            }
            let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            let Ok(text) = std::fs::read_to_string(&path) else {
                continue;
            };
            let Some((name, wm_class)) = parse_entry(&text) else {
                continue;
            };
            let id = id.to_lowercase();
            let mut reported = vec![id.clone()];
            // An id is a reverse-domain name: the product is the last segment,
            // which is also what a window class often reports.
            if let Some(product) = id.rsplit('.').next() {
                reported.push(product.to_string());
            }
            if let Some(wm_class) = wm_class {
                reported.push(wm_class.to_lowercase());
            }
            for key in &reported {
                names.entry(key.clone()).or_insert_with(|| name.clone());
            }
            let by_name = keys
                .entry(crate::config::canonical_app_name(&name))
                .or_default();
            for key in reported {
                if !by_name.contains(&key) {
                    by_name.push(key);
                }
            }
        }
    }
    Index { names, keys }
}

/// `Name` and `StartupWMClass` of the `[Desktop Entry]` group.
fn parse_entry(text: &str) -> Option<(String, Option<String>)> {
    let mut name: Option<String> = None;
    let mut wm_class: Option<String> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            // Only the first group is the entry itself; later groups are actions.
            if name.is_some() {
                break;
            }
            continue;
        }
        if let Some(value) = line.strip_prefix("Name=") {
            if name.is_none() {
                name = Some(value.trim().to_string());
            }
        } else if let Some(value) = line.strip_prefix("StartupWMClass=") {
            wm_class = Some(value.trim().to_string());
        }
    }
    name.filter(|name| !name.is_empty())
        .map(|name| (name, wm_class))
}

/// The reported name folded to the key a desktop entry is filed under.
fn lookup_key(target: &str) -> String {
    let target = crate::config::strip_transport_prefix(target);
    // `gtk3-im:sublime_text` and `/usr/bin/foo` both name the app last.
    let target = target.rsplit([':', '/']).next().unwrap_or(target);
    target.to_lowercase()
}

/// The name read out of a window class, for apps with no desktop entry.
fn heuristic(target: &str) -> String {
    let name = crate::config::strip_transport_prefix(target);
    let name = name.rsplit('/').next().unwrap_or(name);
    const TLDS: [&str; 8] = ["org", "com", "io", "net", "dev", "app", "co", "me"];
    const VENDORS: [&str; 5] = ["gnome", "kde", "freedesktop", "github", "gitlab"];
    // Filler words an id carries after the app's own name.
    const GENERIC: [&str; 7] = [
        "client", "app", "main", "desktop", "gui", "window", "server",
    ];
    let mut segments: Vec<&str> = name
        .split('.')
        .filter(|segment| !segment.is_empty())
        .collect();
    if segments.len() > 1 && TLDS.contains(&segments[0]) {
        segments.remove(0);
    }
    let mut words: Vec<String> = Vec::new();
    for segment in segments {
        let chars: Vec<char> = segment.chars().collect();
        let mut word = String::new();
        let mut was_lower = false;
        for (index, &c) in chars.iter().enumerate() {
            if !c.is_alphanumeric() {
                if !word.is_empty() {
                    words.push(std::mem::take(&mut word));
                }
                was_lower = false;
                continue;
            }
            // `TextEditor` -> `Text Editor`, but `IntelliJ` and `IDEA` stay one
            // word: a capital that closes or opens an acronym belongs to it.
            let next = chars.get(index + 1);
            let keeps_word = next.is_none_or(|n| !n.is_alphanumeric() || n.is_uppercase());
            if was_lower && c.is_uppercase() && !word.is_empty() && !keeps_word {
                words.push(std::mem::take(&mut word));
            }
            was_lower = c.is_lowercase();
            word.push(c);
        }
        if !word.is_empty() {
            words.push(word);
        }
    }
    // `com.spotify.Client` and `org.telegram.desktop` end on filler: the app is
    // the word in front of it.
    if words.len() > 1
        && GENERIC.contains(
            &words
                .last()
                .map(|w| w.to_lowercase())
                .unwrap_or_default()
                .as_str(),
        )
    {
        words.pop();
    }
    // A CamelCase product name carries the app; the lowercase segment in front
    // of it is the vendor (`gnome.Nautilus`, `discordapp.Discord`). An id with
    // no case at all is one name, not vendor plus product (`telegram.desktop`).
    let named = words
        .last()
        .is_some_and(|word| word.chars().any(char::is_uppercase));
    while words.len() > 1
        && (VENDORS.contains(&words[0].to_lowercase().as_str())
            || (named && words[0].chars().all(|c| !c.is_uppercase())))
    {
        words.remove(0);
    }
    // `org.localsend.localsend_app` names the app twice: an id repeats its own
    // name before the filler word.
    words.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let mut out = String::new();
    for word in &words {
        if !out.is_empty() {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    if out.is_empty() {
        name.to_string()
    } else {
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_desktop_entry_names_the_app_its_class_cannot() {
        let dir = crate::test_support::TempDir::new("mangrs-appname");
        std::fs::write(
            dir.join("onlyoffice-desktopeditors.desktop"),
            "[Desktop Entry]\nName=ONLYOFFICE\nStartupWMClass=onlyoffice-desktopeditors\n\
             \n[Desktop Action new]\nName=New document\n",
        )
        .expect("write entry");
        let names = build(&[dir.path().to_path_buf()]).names;
        assert_eq!(
            names.get("onlyoffice-desktopeditors").map(String::as_str),
            Some("ONLYOFFICE")
        );
        // The action's own Name never wins over the entry's.
        assert_ne!(
            names.get("onlyoffice-desktopeditors").map(String::as_str),
            Some("New document")
        );
    }

    #[test]
    fn a_desktop_entry_is_found_through_the_product_segment() {
        let dir = crate::test_support::TempDir::new("mangrs-appname2");
        std::fs::write(
            dir.join("org.gnome.Nautilus.desktop"),
            "[Desktop Entry]\nName=Files\n",
        )
        .expect("write entry");
        let names = build(&[dir.path().to_path_buf()]).names;
        assert_eq!(names.get("nautilus").map(String::as_str), Some("Files"));
        assert_eq!(
            names.get("org.gnome.nautilus").map(String::as_str),
            Some("Files")
        );
    }

    #[test]
    fn a_class_with_no_desktop_entry_is_read_as_a_name() {
        for (raw, want) in [
            ("org.gnome.TextEditor", "Text Editor"),
            ("gnome-text-editor", "Text Editor"),
            ("org.telegram.desktop", "Telegram"),
            ("com.discordapp.Discord", "Discord"),
            ("com.spotify.Client", "Spotify"),
            ("com.jetbrains.IntelliJ-IDEA", "IntelliJ IDEA"),
            ("org.kde.kdenlive", "Kdenlive"),
            ("gnome-terminal-server", "Terminal"),
            ("org.localsend.localsend_app", "Localsend"),
            ("Alacritty", "Alacritty"),
            ("wayland:firefox", "Firefox"),
            ("x11:Sublime_text", "Sublime Text"),
        ] {
            assert_eq!(heuristic(raw), want, "raw: {raw}");
        }
    }

    #[test]
    fn the_shell_names_itself() {
        assert_eq!(display_name("gnome-shell"), "GNOME Shell");
    }

    #[test]
    fn a_reported_route_is_folded_to_the_entry_key() {
        assert_eq!(
            lookup_key("wayland:org.gnome.TextEditor"),
            "org.gnome.texteditor"
        );
        assert_eq!(lookup_key("gtk3-im:sublime_text"), "sublime_text");
        assert_eq!(lookup_key("/usr/bin/firefox"), "firefox");
    }
}
