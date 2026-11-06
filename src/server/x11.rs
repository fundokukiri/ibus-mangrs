//! Focused-app naming on X11 (xprop) and the seen-client log the GUI
//! rules dropdown reads back.

use super::*;

/// Best-effort name for X11/XWayland clients that never send FocusInId: the
/// active window's WM_CLASS from the X server. Wayland-native windows are
/// invisible to XWayland, so this never names them (they stay relayed).
/// Two `xprop` spawns per unnamed focus-in — focus changes are rare, and this
/// never runs on the key path.
pub(super) fn x11_active_window_name() -> Option<String> {
    std::env::var_os("DISPLAY")?;
    let root = std::process::Command::new("xprop")
        .arg("-root")
        .arg("_NET_ACTIVE_WINDOW")
        .output()
        .ok()?;
    if !root.status.success() {
        return None;
    }
    let id = active_window_id(std::str::from_utf8(&root.stdout).ok()?)?;
    let out = std::process::Command::new("xprop")
        .args(["-id", &id, "WM_CLASS"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    window_class(std::str::from_utf8(&out.stdout).ok()?)
}

/// `"_NET_ACTIVE_WINDOW(WINDOW): window id # 0x2c00007"` → `"0x2c00007"`.
pub(super) fn active_window_id(root_output: &str) -> Option<String> {
    root_output
        .split("window id # ")
        .nth(1)?
        .split_whitespace()
        .next()
        .map(str::to_string)
}

/// `WM_CLASS(STRING) = "code", "Code"` → `"Code"` (the class, not the
/// instance); a single-string value is used as-is.
pub(super) fn window_class(xprop_output: &str) -> Option<String> {
    let value = xprop_output.split('=').nth(1)?;
    let strings: Vec<String> = value
        .split(',')
        .map(|p| p.trim().trim_matches('"').to_string())
        .filter(|s| !s.is_empty())
        .collect();
    strings.get(1).or(strings.first()).cloned()
}

/// Record a seen client prgname in `~/.cache/ibus-mangrs/clients` so the GUI can
/// offer it as an App Rules target. Best effort; keeps the newest 50.
pub(super) fn note_client(msg: &Message) {
    let Ok((_, client)) = msg.read2::<String, String>() else {
        return;
    };
    let Some(prgname) = client.split_once(':').map(|(_, p)| p.to_string()) else {
        return;
    };
    note_prgname(&prgname);
}

pub(super) fn note_prgname(prgname: &str) {
    if prgname == "fake" || prgname.is_empty() {
        return;
    }
    let Some(dir) = dirs::cache_dir().map(|d| d.join("ibus-mangrs")) else {
        return;
    };
    let _ = std::fs::create_dir_all(&dir);
    let path = dir.join("clients");
    let mut names: Vec<String> = std::fs::read_to_string(&path)
        .map(|t| t.lines().map(str::to_string).collect())
        .unwrap_or_default();
    names.retain(|n| n != prgname);
    names.insert(0, prgname.to_string());
    names.truncate(50);
    let _ = std::fs::write(path, names.join("\n"));
}
