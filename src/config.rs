//! TOML configuration, shared between the engine and the GTK4 settings app.
//!
//! Location: `$XDG_CONFIG_HOME/ibus-mangrs/config.toml`, falling back to
//! `~/.config/ibus-mangrs/config.toml`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::bamboo_wrapper::{Bracket, Charset, Method, W2u};
use crate::engine::Mode;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    /// Whether Vietnamese processing is on. The GUI's on/off toggle writes here.
    pub enabled: bool,
    pub method: MethodConfig,
    pub mode: ModeConfig,
    /// Charset used for committed text (`Unicode` by default).
    pub charset: CharsetConfig,
    /// Standard (new) tone placement: `hòa` vs traditional `hoà`.
    pub modern_tone_style: bool,
    /// Allow tone marks typed anywhere in the word (`hoangf` -> `hoàng`).
    pub free_tone_marking: bool,
    /// Fall back to raw characters for invalid syllables.
    pub auto_correct: bool,
    /// Restore English (non-Vietnamese) words to the raw keystrokes typed.
    pub auto_restore: bool,
    /// Keep words raw when they directly follow a URL/email/path character
    /// (`/ : @ . _ ~ = # & % ?`) — no Vietnamese marks, no macros.
    pub bypass_url: bool,
    /// When a plain `w` turns into `ư` (`disabled` / `nonstart` / `everywhere`).
    pub w2u: W2uConfig,
    /// When `[` `]` `{` `}` type `ơ` `ư` (`disabled` / `nonstart` / `everywhere`).
    pub bracket: BracketConfig,
    /// `--` becomes `—`.
    pub em_dash: bool,
    /// Capitalize the first letter after `.`, `!`, `?` and Enter.
    pub auto_capitalize: bool,
    /// Two spaces insert `. ` (experimental).
    pub double_space_period: bool,
    /// Expand macros on word end.
    pub macro_enabled: bool,
    /// Match macro keys case-insensitively and capitalize the expansion.
    pub capitalize_macro: bool,
    /// Expand macros even when Vietnamese processing is off.
    pub macro_in_off_mode: bool,
    /// `$TIME` expansion format (chrono strftime).
    pub time_format: String,
    /// `$DATE` expansion format (chrono strftime).
    pub date_format: String,
    /// Words from `dictionary.txt` are never auto-restored.
    pub dictionary_enabled: bool,
    /// Hotkey cycling the display mode, e.g. `Control+Alt+v`. Empty = unassigned.
    pub cycle_mode_hotkey: String,
    /// Send preedit without the underline attribute (app support permitting).
    pub hide_underline: bool,
    /// Uinput pacing: gap between injected text/commit steps (ms).
    pub commit_interval_ms: u64,
    /// Uinput pacing: gap between injected Backspace keys (ms).
    pub backspace_interval_ms: u64,
    /// Uinput pacing: idle time after a rewrite before more typing (ms).
    pub post_delay_ms: u64,
    /// Preedit idle timeout: a word still pending after this many ms without
    /// a keystroke commits in place. Prevents a later mouse click from
    /// relocating the word (open question 14). 0 = off.
    pub preedit_commit_timeout_ms: u64,
    /// How the Vietnamese word reaches the app in the uinput modes:
    /// `commit` = IBus CommitText, `clipboard` = clipboard paste (Ctrl+V).
    pub unicode_channel: UnicodeChannel,
    /// Append engine decisions (focus, capabilities, surrounding text,
    /// deletes) to `~/.cache/ibus-mangrs/debug.log` for live diagnosis.
    pub debug_log: bool,
    /// Per-application display mode overrides (phase 9).
    pub app_rules: Vec<AppRule>,
}

/// One per-application rule: if the client prgname contains `pattern`
/// (case-insensitive substring), the engine uses `mode` for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    pub pattern: String,
    pub mode: ModeConfig,
}

/// Channel for Vietnamese text in the uinput modes (`unicode_channel`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum UnicodeChannel {
    /// IBus `CommitText` — safe everywhere, does not touch the clipboard.
    #[default]
    Commit,
    /// Set the clipboard to the word, paste with injected Ctrl+V, restore the
    /// clipboard afterwards. Works in apps that read raw keys (games).
    Clipboard,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CharsetConfig {
    #[default]
    Unicode,
    /// Unicode decomposed (NFD).
    #[serde(rename = "nfd")]
    Nfd,
    Tcvn3,
    #[serde(rename = "vni")]
    Vni,
    Viqr,
    Viscii,
    Vps,
    /// Windows 1258 codepage.
    #[serde(rename = "cp1258")]
    Cp1258,
    /// BKHCM (1).
    #[serde(rename = "bkhcm")]
    Bkhcm,
    VietwareX,
    VietwareFull,
}

impl From<CharsetConfig> for Charset {
    fn from(c: CharsetConfig) -> Self {
        match c {
            CharsetConfig::Unicode => Charset::Unicode,
            CharsetConfig::Nfd => Charset::Nfd,
            CharsetConfig::Tcvn3 => Charset::Tcvn3,
            CharsetConfig::Vni => Charset::Vni,
            CharsetConfig::Viqr => Charset::Viqr,
            CharsetConfig::Viscii => Charset::Viscii,
            CharsetConfig::Vps => Charset::Vps,
            CharsetConfig::Cp1258 => Charset::Cp1258,
            CharsetConfig::Bkhcm => Charset::Bkhcm,
            CharsetConfig::VietwareX => Charset::VietwareX,
            CharsetConfig::VietwareFull => Charset::VietwareFull,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum MethodConfig {
    #[default]
    Telex,
    Vni,
    Viqr,
    /// Microsoft layout preset (`đ` on `d` twice, marks on 1..9).
    Microsoft,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModeConfig {
    #[serde(alias = "non-preedit", alias = "non_preedit")]
    NonPreedit,
    /// Default: the composition renders in the application itself, so even
    /// restricted apps (Sublime, XWayland clients) show text.
    #[default]
    Preedit,
    /// Non-preedit rewritten with the help of `SetSurroundingText`: the engine
    /// verifies the text before the cursor against what it believes it
    /// committed and repairs drift (dropped deletes, manual edits).
    Surrounding,
    /// Smooth (uinput): raw keys are forwarded while typing; at word end the
    /// latin tail is erased with real Backspace keys and the Vietnamese word
    /// is committed. Falls back to Preedit where injection is impossible.
    Smooth,
    /// Smooth without the pre-rewrite verification (minimum latency).
    SuperSmooth,
    /// Select (uinput): Shift+Left selects the typed tail, Delete erases it,
    /// then the Vietnamese word is committed. Needs a surrounding-text report.
    Select,
    /// Vietnamese processing off, but macros and auto-capitalize still run.
    Off,
    /// The engine is inert: every key is forwarded, no processing at all.
    DefaultTyping,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum W2uConfig {
    #[default]
    Disabled,
    NonStart,
    Everywhere,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BracketConfig {
    #[default]
    Disabled,
    NonStart,
    Everywhere,
}

impl From<W2uConfig> for W2u {
    fn from(w: W2uConfig) -> Self {
        match w {
            W2uConfig::Disabled => W2u::Disabled,
            W2uConfig::NonStart => W2u::NonStart,
            W2uConfig::Everywhere => W2u::Everywhere,
        }
    }
}

impl From<BracketConfig> for Bracket {
    fn from(b: BracketConfig) -> Self {
        match b {
            BracketConfig::Disabled => Bracket::Disabled,
            BracketConfig::NonStart => Bracket::NonStart,
            BracketConfig::Everywhere => Bracket::Everywhere,
        }
    }
}

impl From<MethodConfig> for Method {
    fn from(m: MethodConfig) -> Self {
        match m {
            MethodConfig::Telex => Method::Telex,
            MethodConfig::Vni => Method::Vni,
            MethodConfig::Viqr => Method::Viqr,
            MethodConfig::Microsoft => Method::MsLayout,
        }
    }
}

impl From<ModeConfig> for Mode {
    fn from(m: ModeConfig) -> Self {
        match m {
            ModeConfig::NonPreedit => Mode::NonPreedit,
            ModeConfig::Preedit => Mode::Preedit,
            ModeConfig::Surrounding => Mode::Surrounding,
            ModeConfig::Smooth => Mode::Smooth,
            ModeConfig::SuperSmooth => Mode::SuperSmooth,
            ModeConfig::Select => Mode::Select,
            ModeConfig::Off => Mode::Off,
            ModeConfig::DefaultTyping => Mode::DefaultTyping,
        }
    }
}

impl From<Mode> for ModeConfig {
    fn from(m: Mode) -> Self {
        match m {
            Mode::NonPreedit => ModeConfig::NonPreedit,
            Mode::Preedit => ModeConfig::Preedit,
            Mode::Surrounding => ModeConfig::Surrounding,
            Mode::Smooth => ModeConfig::Smooth,
            Mode::SuperSmooth => ModeConfig::SuperSmooth,
            Mode::Select => ModeConfig::Select,
            Mode::Off => ModeConfig::Off,
            Mode::DefaultTyping => ModeConfig::DefaultTyping,
        }
    }
}

/// Names used in the property-menu keys (`charset:vni`, `method:microsoft`,
/// `display:preedit`) — they match the serde names above.
impl CharsetConfig {
    pub fn key(self) -> &'static str {
        match self {
            CharsetConfig::Unicode => "unicode",
            CharsetConfig::Nfd => "nfd",
            CharsetConfig::Tcvn3 => "tcvn3",
            CharsetConfig::Vni => "vni",
            CharsetConfig::Viqr => "viqr",
            CharsetConfig::Viscii => "viscii",
            CharsetConfig::Vps => "vps",
            CharsetConfig::Cp1258 => "cp1258",
            CharsetConfig::Bkhcm => "bkhcm",
            CharsetConfig::VietwareX => "vietwarex",
            CharsetConfig::VietwareFull => "vietwarefull",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "unicode" => CharsetConfig::Unicode,
            "nfd" => CharsetConfig::Nfd,
            "tcvn3" => CharsetConfig::Tcvn3,
            "vni" => CharsetConfig::Vni,
            "viqr" => CharsetConfig::Viqr,
            "viscii" => CharsetConfig::Viscii,
            "vps" => CharsetConfig::Vps,
            "cp1258" => CharsetConfig::Cp1258,
            "bkhcm" => CharsetConfig::Bkhcm,
            "vietwarex" => CharsetConfig::VietwareX,
            "vietwarefull" => CharsetConfig::VietwareFull,
            _ => return None,
        })
    }
}

impl MethodConfig {
    pub fn key(self) -> &'static str {
        match self {
            MethodConfig::Telex => "telex",
            MethodConfig::Vni => "vni",
            MethodConfig::Viqr => "viqr",
            MethodConfig::Microsoft => "microsoft",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "telex" => MethodConfig::Telex,
            "vni" => MethodConfig::Vni,
            "viqr" => MethodConfig::Viqr,
            "microsoft" => MethodConfig::Microsoft,
            _ => return None,
        })
    }
}

impl ModeConfig {
    pub fn key(self) -> &'static str {
        match self {
            ModeConfig::NonPreedit => "nonpreedit",
            ModeConfig::Preedit => "preedit",
            ModeConfig::Surrounding => "surrounding",
            ModeConfig::Smooth => "smooth",
            ModeConfig::SuperSmooth => "supersmooth",
            ModeConfig::Select => "select",
            ModeConfig::Off => "off",
            ModeConfig::DefaultTyping => "defaulttyping",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "nonpreedit" | "non-preedit" => ModeConfig::NonPreedit,
            "preedit" => ModeConfig::Preedit,
            "surrounding" => ModeConfig::Surrounding,
            "smooth" => ModeConfig::Smooth,
            "supersmooth" | "super-smooth" | "super_smooth" => ModeConfig::SuperSmooth,
            "select" => ModeConfig::Select,
            "off" => ModeConfig::Off,
            "defaulttyping" | "default-typing" | "default_typing" => ModeConfig::DefaultTyping,
            _ => return None,
        })
    }
}

/// Client prgnames the engine has seen (the server records them in
/// `~/.cache/ibus-mangrs/clients`), newest first — offered by the GUI as App
/// Rules targets.
pub fn recent_clients() -> Vec<String> {
    dirs::cache_dir()
        .map(|d| d.join("ibus-mangrs").join("clients"))
        .and_then(|p| std::fs::read_to_string(p).ok())
        .map(|t| t.lines().map(str::to_string).filter(|l| !l.is_empty()).collect())
        .unwrap_or_default()
}

/// Built-in rules for clients known to drop `DeleteSurroundingText`.
fn default_app_rules() -> Vec<AppRule> {
    let preedit = [
        "chrome", "chromium", "brave", "msedge", "vivaldi", "electron", "code", "slack",
        "discord", "kgx", "gnome-console",
    ];
    let surrounding = ["sublime_text"];
    preedit
        .into_iter()
        .map(|pattern| AppRule { pattern: pattern.into(), mode: ModeConfig::Preedit })
        .chain(
            surrounding
                .into_iter()
                .map(|pattern| AppRule { pattern: pattern.into(), mode: ModeConfig::Surrounding }),
        )
        .collect()
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            method: MethodConfig::Telex,
            mode: ModeConfig::Preedit,
            charset: CharsetConfig::Unicode,
            modern_tone_style: true,
            free_tone_marking: true,
            auto_correct: true,
            auto_restore: true,
            bypass_url: true,
            w2u: W2uConfig::Disabled,
            bracket: BracketConfig::Disabled,
            em_dash: false,
            auto_capitalize: false,
            double_space_period: false,
            macro_enabled: true,
            capitalize_macro: true,
            macro_in_off_mode: false,
            time_format: "%H:%M".into(),
            date_format: "%d/%m/%Y".into(),
            dictionary_enabled: false,
            cycle_mode_hotkey: String::new(),
            hide_underline: false,
            commit_interval_ms: 20,
            backspace_interval_ms: 5,
            post_delay_ms: 10,
            preedit_commit_timeout_ms: 800,
            unicode_channel: UnicodeChannel::Commit,
            debug_log: false,
            app_rules: default_app_rules(),
        }
    }
}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|d| d.join("ibus-mangrs").join("config.toml"))
    }

    /// Copy a pre-rename `unikey`/`r-unikey` config.toml next to the new
    /// file, once. The old file is left alone so a downgrade still finds
    /// its settings.
    fn migrate_old_config() {
        let Some(dir) = dirs::config_dir() else { return };
        let new = dir.join("ibus-mangrs").join("config.toml");
        if new.exists() {
            return;
        }
        let old = ["unikey", "r-unikey"]
            .iter()
            .map(|name| dir.join(name).join("config.toml"))
            .find(|path| path.exists());
        let Some(old) = old else { return };
        if let Some(parent) = new.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let _ = std::fs::copy(old, new);
    }

    /// Load the config, falling back to defaults on any error. A broken config
    /// must never stop the engine from starting.
    pub fn load() -> Self {
        Self::migrate_old_config();
        let Some(path) = Self::path() else { return Self::default() };
        let Ok(text) = std::fs::read_to_string(&path) else { return Self::default() };
        toml::from_str(&text).unwrap_or_default()
    }

    pub fn save(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no config dir"));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        // Keep keys this build does not know about (an older/newer version or a
        // hand edit) instead of silently dropping them on the next save.
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let text = merge(&existing, self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Translate to the subset of bamboo-core options this engine exposes.
    pub fn bamboo_config(&self) -> bamboo_core::Config {
        bamboo_core::Config::builder()
            .free_tone_marking(self.free_tone_marking)
            .tone_style(if self.modern_tone_style {
                bamboo_core::ToneStyle::Modern
            } else {
                bamboo_core::ToneStyle::Traditional
            })
            .auto_correct(self.auto_correct)
            .w2u_mode(match self.w2u {
                W2uConfig::Disabled => bamboo_core::W2uMode::Disabled,
                W2uConfig::NonStart => bamboo_core::W2uMode::NonStart,
                W2uConfig::Everywhere => bamboo_core::W2uMode::Everywhere,
            })
            .bracket_mode(match self.bracket {
                BracketConfig::Disabled => bamboo_core::BracketMode::Disabled,
                BracketConfig::NonStart => bamboo_core::BracketMode::NonStart,
                BracketConfig::Everywhere => bamboo_core::BracketMode::Everywhere,
            })
            .build()
    }

    /// Method and display mode in the form the engine wants.
    pub fn engine_settings(&self) -> (Method, Mode) {
        (self.method.into(), self.mode.into())
    }
}

/// Serialize `config` over `existing`, leaving unknown keys in place.
fn merge(existing: &str, config: &Config) -> Result<String, toml::ser::Error> {
    let mut table: toml::Table = toml::from_str(existing).unwrap_or_default();
    if let toml::Value::Table(new) = toml::Value::try_from(config)? {
        for (key, value) in new {
            table.insert(key, value);
        }
    }
    toml::to_string_pretty(&table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_preedit_telex() {
        let c = Config::default();
        assert!(c.enabled);
        assert_eq!(c.method, MethodConfig::Telex);
        // Preedit renders in the application itself — the most compatible
        // default (requested for install defaults).
        assert_eq!(c.mode, ModeConfig::Preedit);
        assert_eq!(c.charset, CharsetConfig::Unicode);
        assert!(c.auto_restore);
        assert!(c.macro_enabled);
        assert!(!c.em_dash);
        assert!(c.cycle_mode_hotkey.is_empty());
    }

    #[test]
    fn charset_is_read_from_toml() {
        let c: Config = toml::from_str("charset = \"vni\"").unwrap();
        assert_eq!(c.charset, CharsetConfig::Vni);
        let c: Config = toml::from_str("charset = \"nfd\"").unwrap();
        assert_eq!(c.charset, CharsetConfig::Nfd);
        let c: Config = toml::from_str("charset = \"cp1258\"").unwrap();
        assert_eq!(c.charset, CharsetConfig::Cp1258);
    }

    #[test]
    fn save_keeps_keys_this_build_does_not_know() {
        let config = Config { method: MethodConfig::Vni, ..Config::default() };
        let text = merge("method = \"telex\"\nw2u_mode = \"disabled\"\n", &config).unwrap();
        let back: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(back["method"], toml::Value::String("vni".into()));
        assert_eq!(back["w2u_mode"], toml::Value::String("disabled".into()));
    }

    #[test]
    fn roundtrips_through_toml() {
        let c = Config {
            method: MethodConfig::Vni,
            mode: ModeConfig::Preedit,
            modern_tone_style: false,
            app_rules: vec![AppRule { pattern: "kate".into(), mode: ModeConfig::Preedit }],
            ..Config::default()
        };
        let text = toml::to_string_pretty(&c).unwrap();
        let back: Config = toml::from_str(&text).unwrap();
        assert_eq!(back.method, MethodConfig::Vni);
        assert_eq!(back.mode, ModeConfig::Preedit);
        assert!(!back.modern_tone_style);
        assert_eq!(back.app_rules, c.app_rules);
    }

    #[test]
    fn partial_file_uses_defaults_for_missing_keys() {
        let c: Config = toml::from_str("mode = \"preedit\"").unwrap();
        assert_eq!(c.mode, ModeConfig::Preedit);
        assert!(c.enabled, "missing keys take defaults");
        // Missing app_rules fall back to the built-in safety net.
        assert!(c.app_rules.iter().any(|r| r.pattern == "chrome"));
    }

    #[test]
    fn hyphenated_mode_alias_is_accepted() {
        let c: Config = toml::from_str("mode = \"non-preedit\"").unwrap();
        assert_eq!(c.mode, ModeConfig::NonPreedit);
    }

    #[test]
    fn mode_surrounding_roundtrips() {
        let c: Config = toml::from_str("mode = \"surrounding\"").unwrap();
        assert_eq!(c.mode, ModeConfig::Surrounding);
    }

    #[test]
    fn method_microsoft_roundtrips() {
        let c: Config = toml::from_str("method = \"microsoft\"").unwrap();
        assert_eq!(c.method, MethodConfig::Microsoft);
        let (method, _) = c.engine_settings();
        assert_eq!(method, Method::MsLayout);
    }

    #[test]
    fn w2u_and_bracket_map_to_bamboo() {
        let c: Config = toml::from_str("w2u = \"everywhere\"\nbracket = \"nonstart\"").unwrap();
        let b = c.bamboo_config();
        assert_eq!(b.w2u_mode, bamboo_core::W2uMode::Everywhere);
        assert_eq!(b.bracket_mode, bamboo_core::BracketMode::NonStart);
    }
}
