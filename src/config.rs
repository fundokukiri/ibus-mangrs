//! TOML configuration, shared between the engine and the GTK4 settings app.
//!
//! Location: `$XDG_CONFIG_HOME/ibus-mangrs/config.toml`, falling back to
//! `~/.config/ibus-mangrs/config.toml`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::bamboo_wrapper::{Bracket, Charset, Method, W2u};
use crate::engine::Mode;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// How Backspace treats a tone-marked word — default: the tone goes
    /// first. Never applies to the uinput modes.
    pub backspace_tone: BackspaceToneConfig,
    /// Restore English (non-Vietnamese) words to the raw keystrokes typed.
    pub auto_restore: bool,
    /// Keep raw typing inside a real URL (`://` scheme or `www.` — the
    /// whole address until a sentence boundary) — no Vietnamese marks, no
    /// macros. No built-in character starts a bypass: prose punctuation
    /// never blocks marks; the optional `extra_url_chars` list adds
    /// per-word triggers.
    pub bypass_url: bool,
    /// Extra characters that start a bypass word when typed directly before
    /// it — the only per-word triggers; a real URL (`://`, `www.`) opens
    /// the span regardless. `-` is rejected (it stays out of the start set
    /// so `--` can become an em dash), so are letters, digits and
    /// whitespace.
    pub extra_url_chars: String,
    /// Whether `extra_url_chars` applies at all — the switch off means the
    /// built-in set alone. The string itself stays saved in the file and
    /// reapplies when the switch returns.
    pub extra_url_chars_enabled: bool,
    /// When a plain `w` turns into `ư` (`disabled` / `nonstart` / `everywhere`).
    pub w2u: W2uConfig,
    /// When `[` `]` `{` `}` type `ơ` `ư` (`disabled` / `nonstart` / `everywhere`).
    pub bracket: BracketConfig,
    /// `--` becomes `—`.
    pub em_dash: bool,
    /// Capitalize the first letter after `.`, `!`, `?` and Enter.
    pub auto_capitalize: bool,
    /// Extra characters that turn the capitalize flag on when typed directly
    /// before a word, on top of the built-in `. ! ?` + Enter. Letters,
    /// digits and whitespace are rejected when the string is read.
    pub auto_capitalize_after: String,
    /// Whether `auto_capitalize_after` applies at all — the switch off
    /// means the built-in set alone. The string itself stays saved in the
    /// file and reapplies when the switch returns.
    pub auto_capitalize_after_enabled: bool,
    /// Two spaces insert `. ` (experimental).
    pub double_space_period: bool,
    /// Expand macros on word end.
    pub macro_enabled: bool,
    /// Match macro keys case-insensitively and capitalize the expansion.
    pub capitalize_macro: bool,
    /// Expand macros even when Vietnamese processing is off.
    pub macro_in_off_mode: bool,
    /// Let a macro win even when its key forms a valid Vietnamese word
    /// (`cam` as a shorthand). Off by default: a real word stays a word,
    /// and a colliding shorthand makes that word reachable only by
    /// backspacing one key out.
    pub macro_expand_valid_words: bool,
    /// `$TIME` expansion format (chrono strftime).
    pub time_format: String,
    /// `$DATE` expansion format (chrono strftime).
    pub date_format: String,
    /// Settings-window language ("vi" | "en"); the engine ignores it.
    pub language: String,
    /// Words from `dictionary.txt` are never auto-restored.
    pub dictionary_enabled: bool,
    /// Vietnamese abbreviations keep their đ: bamboo's auto_correct reverts
    /// non-syllable compositions (`ddc`) to the raw keys; with this on the
    /// wrapper re-applies the dd→đ transform (`đc`, `đk`, `đt`, `đg`).
    pub abbreviations: bool,
    /// Hotkey cycling the display mode, e.g. `Control+Alt+v`. Empty = unassigned.
    pub cycle_mode_hotkey: String,
    /// Hotkey toggling Vietnamese on/off, e.g. `Control+Alt+space`.
    /// Empty = unassigned.
    pub enabled_hotkey: String,
    /// Hotkey opening the per-app rule picker. Reserved — the picker
    /// window is a later round; empty = unassigned.
    pub app_rule_hotkey: String,
    /// Hotkey opening the emoji picker. Reserved the same way; empty =
    /// unassigned.
    pub emoji_hotkey: String,
    /// Typing `:name:` rewrites the shortcode to its emoji — the word
    /// after `:` composes like any word, and the closing ':' looks the raw
    /// keys up (a miss keeps the text as typed).
    pub emoji_enabled: bool,
    /// Send preedit without the underline attribute (app support permitting).
    pub hide_underline: bool,
    /// Uinput pacing: gap between injected text/commit steps (ms).
    pub commit_interval_ms: u64,
    /// Uinput pacing: gap between injected Backspace keys (ms).
    pub backspace_interval_ms: u64,
    /// Uinput pacing: idle time after a rewrite before more typing (ms).
    pub post_delay_ms: u64,
    /// How the Vietnamese word reaches the app in the uinput modes:
    /// `commit` = IBus CommitText, `clipboard` = clipboard paste (Ctrl+V).
    pub unicode_channel: UnicodeChannel,
    /// Custom clipboard restore delay after a paste (ms) — the built-in
    /// default applies while the switch is off.
    pub clipboard_restore_delay_enabled: bool,
    pub clipboard_restore_delay_ms: u64,
    /// Custom lost-echo stale window (ms) for the uinput modes — the
    /// built-in default applies while the switch is off. Too small risks
    /// lost characters; too large makes words appear late.
    pub echo_stale_enabled: bool,
    pub echo_stale_ms: u64,
    /// Commit a paused preedit word after `idle_commit_ms` of no keys.
    /// Off by default: the underline then holds until a terminator, which
    /// is the behavior users know.
    pub idle_commit_enabled: bool,
    pub idle_commit_ms: u64,
    /// Show the composed word progressively in the Flow/Turbo modes: every
    /// key that changes the composition erases the committed tail and
    /// commits the new one through the injected transport (`ee` shows
    /// `ê` immediately, marks are editable mid-word) instead of holding
    /// the raw keys until the word ends. On by default: the owner ships
    /// the per-key display as the standard look; the word-end rewrite
    /// stays one switch away and the per-key commit costs ~60 ms per key.
    pub flow_per_key: bool,
    /// How long the per-key display parks each keystroke's commit before
    /// it lands (ms). Per-key pacing only: the two server-side rewrite
    /// parks stay on the engine's built-in grace constant, so a mistuned
    /// value here cannot reintroduce the relay ordering bug.
    pub flow_per_key_delay_ms: u64,
    /// Experimental: a Backspace on a committed word in the uinput modes
    /// re-tracks the shortened screen (the composition is rebuilt from the
    /// re-encoded keys), so the next mark key keeps editing the word
    /// (`tùn` + `s` -> `tún`) instead of landing as a literal letter. Off
    /// keeps today's plain-text behavior byte for byte.
    pub flow_backspace_retrack: bool,
    /// Master gate for every experimental feature: off hides their rows in
    /// the settings window and forces them all off in the engine, while
    /// each switch and value stays saved in this file and comes back when
    /// the gate reopens.
    pub experimental_features: bool,
    /// GUI-only: show the settings window's toast notifications. Off
    /// silences every toast (including the delete-undo ones); errors keep
    /// their dialogs either way. The engine never reads this.
    pub show_toasts: bool,
    /// Append engine decisions (focus, capabilities, surrounding text,
    /// deletes) to `~/.cache/ibus-mangrs/debug.log` for live diagnosis.
    pub debug_log: bool,
    /// Second log tier: record the surrounding-text content and verify diffs
    /// — the typed text itself. Only read while `debug_log` is on, so a
    /// default log stays free of typed text and can be attached to a bug
    /// report as-is. Turn both on only when reproducing a lost/wrong
    /// character bug.
    pub debug_log_text: bool,
    /// Allow the protocol delete for a DIRECT client (never through a mutter
    /// relay) that advertises surrounding-text support but never reported its
    /// text: X11 clients such as Sublime Text apply the delete inside their
    /// own process, where the mutter crash this engine guards against cannot
    /// happen. ibus-bamboo deletes this way for every client. Off by default:
    /// without a report the delete count is unclamped, so a caret the engine
    /// did not see move can lose real text.
    pub x11_delete_without_report: bool,
    /// Keep Preedit for XIM clients (the fixed `xim` id: Java apps such as
    /// JetBrains) instead of falling back to NonPreedit. They report no
    /// preedit capability because the XIM server draws the preedit itself, so
    /// the fallback turned accents into raw keystrokes (`tungf`). Off by
    /// default: an app that negotiated `XIMPreeditNothing` shows nothing
    /// while the word is being typed.
    pub preedit_for_xim: bool,
    /// What the first navigation key does while a preedit commit is pending
    /// on a mutter-relayed context: `swallow` consumes it (the commit lands
    /// first, the caret moves on the next press), `forward` sends the commit
    /// and the key in one reply (ibus-bamboo's sequence), `replay` consumes
    /// the key and re-emits it as ForwardKeyEvent after the commit. Unknown
    /// values behave as `swallow`. Experimental: the gate forces `swallow`.
    pub nav_key_while_preedit: String,
    /// Per-application display mode overrides (phase 9).
    pub app_rules: Vec<AppRule>,
    /// Shortcuts the user assigned beyond the four built-in hotkeys (those
    /// are the defaults). Unknown actions or unusable specs are skipped
    /// when the engine builds its shortcut registry.
    pub custom_hotkeys: Vec<CustomHotkey>,
    /// Which toggles the panel popup shows, in order — the keys of
    /// `ibus_prop::QUICK_TOGGLES`. Unknown keys are skipped when building the
    /// menu, so a hand edit or an older file cannot break the panel.
    pub quick_menu: Vec<String>,
    /// Which display modes the panel popup's "Chế độ gõ" radio group shows,
    /// in order — keys of `ibus_prop::MODES`. Same skip-unknown contract as
    /// `quick_menu`.
    pub menu_modes: Vec<String>,
    /// The user-designed panel menu. `None` (or an empty list) means the
    /// menu is synthesized from `default_menu()` — the shape the engine
    /// shipped before the menu became editable — so an upgrade and a broken
    /// hand edit both fall back to the familiar menu.
    pub menu: Option<Vec<MenuNode>>,
}

/// One node of the user-designed panel menu. Folders hold ONE level of
/// items: a folder inside a folder is skipped when the menu is built, so a
/// hand edit cannot push the popup into unusable nesting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum MenuNode {
    /// A quick toggle — a `ibus_prop::QUICK_TOGGLES` key, or "enabled".
    Toggle {
        key: String,
    },
    /// A radio group: "charset", "method" or "display". Its children are the
    /// entry keys it shows (`ibus_prop::CHARSETS` / `METHODS` / `MODES`
    /// keys), in order — the group is an editable folder menu. Unknown keys
    /// are skipped when the menu is built, and an emptied group disappears.
    Radios {
        group: String,
        #[serde(default)]
        children: Vec<String>,
    },
    /// A fixed action: "macro_editor", "settings" or "about".
    Action {
        key: String,
    },
    Separator,
    /// A submenu.
    Folder {
        label: String,
        #[serde(default)]
        children: Vec<MenuNode>,
    },
}

impl Config {
    /// The panel menu shape the engine shipped before `menu` existed,
    /// expressed as a node tree: everything in `properties()` used to be
    /// fixed, with the quick folder built from `quick_menu` and the radio
    /// groups showing every entry (display: the `menu_modes` selection).
    pub fn default_menu(&self) -> Vec<MenuNode> {
        let mut quick = vec![MenuNode::Action {
            key: "macro_editor".into(),
        }];
        quick.extend(
            self.quick_menu
                .iter()
                .map(|key| MenuNode::Toggle { key: key.clone() }),
        );
        let all_keys = |entries: &[(&str, &str, &str)]| {
            entries
                .iter()
                .map(|(key, ..)| key.to_string())
                .collect::<Vec<_>>()
        };
        vec![
            MenuNode::Action {
                key: "about".into(),
            },
            MenuNode::Separator,
            MenuNode::Toggle {
                key: "enabled".into(),
            },
            MenuNode::Radios {
                group: "charset".into(),
                children: all_keys(crate::ibus_prop::CHARSETS),
            },
            MenuNode::Radios {
                group: "method".into(),
                children: all_keys(crate::ibus_prop::METHODS),
            },
            MenuNode::Radios {
                group: "display".into(),
                children: self.menu_modes.clone(),
            },
            MenuNode::Folder {
                label: "Cấu hình nhanh".into(),
                children: quick,
            },
            MenuNode::Action {
                key: "settings".into(),
            },
        ]
    }
}

/// One per-application rule: if the client prgname contains `pattern`
/// (case-insensitive substring), the engine uses `mode` for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRule {
    pub pattern: String,
    pub mode: ModeConfig,
}

/// One user-assigned shortcut: a `parse_hotkey` spec and the action key it
/// fires (`CUSTOM_HOTKEY_ACTIONS`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CustomHotkey {
    pub spec: String,
    pub action: String,
}

/// The functions a custom shortcut can be assigned to, by action key. The
/// engine maps the key onto its registry actions; the GUI builds the
/// picker dropdown from it (labels come from gui-content.toml). The quick
/// toggles reuse the panel registry's own keys (`ibus_prop::QUICK_TOGGLES`).
/// Keys the GUI does not offer ("enabled", "rule"...) keep matching in the
/// engine, so old configs keep working.
pub const CUSTOM_HOTKEY_ACTIONS: &[&str] = &[
    "mode",
    "emoji",
    "charset",
    "method",
    "auto_capitalize",
    "abbreviations",
    "auto_restore",
    "auto_correct",
    "bypass_url",
    "dictionary",
    "double_space",
    "em_dash",
    "macro_editor",
    "settings",
];

/// One rule pattern against one rule target (`app_name` when present, else
/// the IBus client string, `None` when the client has neither) — the shared
/// predicate behind `app_rule_mode` and the GUI's rule preview.
/// Case-insensitive substring on the part after the transport prefix
/// (`wayland:Firefox` → `Firefox`); the special pattern `none` matches only
/// a client with no name at all, the only way to target one.
pub fn app_rule_matches(pattern: &str, target: Option<&str>) -> bool {
    // A pattern may carry the transport prefix the focus extension adds
    // (`wayland:foo` / `x11:foo`) — targets are matched bare (the prefix is
    // stripped below), so strip it from the pattern too or such a rule can
    // never fire.
    let pattern = pattern
        .strip_prefix("wayland:")
        .or_else(|| pattern.strip_prefix("x11:"))
        .unwrap_or(pattern)
        .to_lowercase();
    if pattern == "none" {
        return target.is_none();
    }
    let Some(name) = target else {
        return false;
    };
    let name = name.split_once(':').map_or(name, |(_, p)| p).to_lowercase();
    !pattern.is_empty() && name.contains(&pattern)
}

/// First `AppRule` matching the target, in config order.
pub fn app_rule_mode(rules: &[AppRule], target: Option<&str>) -> Option<ModeConfig> {
    rules
        .iter()
        .find(|rule| app_rule_matches(&rule.pattern, target))
        .map(|rule| rule.mode)
}

impl Config {
    /// Apply a picker choice: the highest-priority rule matching `target`
    /// (same predicate the engine uses) gets `mode` in place — an app may
    /// match several patterns (`chrome` and `chromium`), and the pick must
    /// change the rule it currently rides on, not pile a new one on top.
    /// Only an app no rule covers gets a fresh rule, at the top.
    pub fn set_rule_mode_for_target(&mut self, target: &str, mode: ModeConfig) {
        if let Some(rule) = self
            .app_rules
            .iter_mut()
            .find(|r| app_rule_matches(&r.pattern, Some(target)))
        {
            rule.mode = mode;
            return;
        }
        self.app_rules.insert(
            0,
            AppRule {
                pattern: target.to_string(),
                mode,
            },
        );
    }

    /// Drop every rule matching `pattern` (case-insensitive).
    pub fn remove_app_rule(&mut self, pattern: &str) {
        let lower = pattern.to_lowercase();
        self.app_rules.retain(|r| r.pattern.to_lowercase() != lower);
    }
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
    /// Telex with `[`/`]` typing the horn vowels (ơ/ư) like the Microsoft
    /// layout, so bracket keys reach bamboo-core even with brackets off.
    Telex2,
    /// Telex marks and VNI digits both type accents.
    TelexVni,
    /// Telex marks, VNI digits and VIQR punctuation all type accents.
    TelexVniViqr,
    /// VNI for AZERTY/French physical layouts.
    VniFrench,
    /// Telex with `w` as the mark key and `z` removing the tone.
    TelexW,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModeConfig {
    #[serde(alias = "non-preedit", alias = "non_preedit")]
    NonPreedit,
    /// Default: the composition renders in the application itself, so even
    /// restricted apps (Sublime, XWayland clients) show text. The removed
    /// Auto mode migrates here (the serde alias and `from_key` below, for
    /// old config files and app rules).
    #[default]
    #[serde(alias = "auto")]
    Preedit,
    /// Non-preedit rewritten with the help of `SetSurroundingText`: the engine
    /// verifies the text before the cursor against what it believes it
    /// committed and repairs drift (dropped deletes, manual edits).
    Surrounding,
    /// Flow (uinput): raw keys are forwarded while typing; at word end the
    /// latin tail is erased with real Backspace keys and the Vietnamese word
    /// is committed. Falls back to Preedit where injection is impossible.
    Flow,
    /// Turbo (uinput): Flow without the pre-rewrite verification (minimum
    /// latency).
    Turbo,
    /// Select (uinput): Shift+Left selects the typed tail, Delete erases it,
    /// then the Vietnamese word is committed. Needs a surrounding-text report.
    Select,
    /// Non-preedit whose tail rewrite deletes by forwarding Backspace key
    /// events through IBus instead of `DeleteSurroundingText` — works in
    /// clients that drop the protocol delete and where `/dev/uinput` cannot
    /// reach (mutter-relayed Wayland). ibus-bamboo's ForwardKeyEvent I.
    ForwardKeyEvent,
    /// The same, but the tail is selected with forwarded Shift+Left and the
    /// commit replaces the selection — for apps that mishandle a forwarded
    /// Backspace. ibus-bamboo's ForwardKeyEvent II.
    ForwardSelect,
    /// Vietnamese processing off, but macros and auto-capitalize still run.
    /// The removed Default Typing mode ("engine inert") migrates here: an
    /// inert engine is what switching to another input method already does.
    #[serde(
        alias = "defaulttyping",
        alias = "default-typing",
        alias = "default_typing"
    )]
    Off,
}

/// How Backspace treats a word that carries a tone — the engine maps the
/// config onto this per keystroke (the uinput modes force `UnikeyWalk`:
/// their screen shows the raw keys, which a skipped tone would desync from
/// the wrapper).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BackspaceToneConfig {
    /// The tone goes first, the letters stay (`hạ` -> `ha` -> `h`).
    #[default]
    ToneFirst,
    /// The marked character goes with its tone (`hạ` -> `h`); the screen
    /// loses exactly one character per Backspace on long words too
    /// (`việt` -> `việ` -> `vi` -> `v`).
    WholeChar,
    /// Unikey convention: Backspace undoes the last raw key — a cycled tone
    /// walks back through every tone it passed (`hạ` `hã` `hả` `hà` `há` `ha`).
    #[serde(alias = "off")]
    UnikeyWalk,
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
            MethodConfig::Telex2 => Method::Telex2,
            MethodConfig::TelexVni => Method::TelexVni,
            MethodConfig::TelexVniViqr => Method::TelexVniViqr,
            MethodConfig::VniFrench => Method::VniFrench,
            MethodConfig::TelexW => Method::TelexW,
        }
    }
}

impl From<ModeConfig> for Mode {
    fn from(m: ModeConfig) -> Self {
        match m {
            ModeConfig::NonPreedit => Mode::NonPreedit,
            ModeConfig::Preedit => Mode::Preedit,
            ModeConfig::Surrounding => Mode::Surrounding,
            ModeConfig::Flow => Mode::Flow,
            ModeConfig::Turbo => Mode::Turbo,
            ModeConfig::Select => Mode::Select,
            ModeConfig::ForwardKeyEvent => Mode::ForwardKeyEvent,
            ModeConfig::ForwardSelect => Mode::ForwardSelect,
            ModeConfig::Off => Mode::Off,
        }
    }
}

impl From<Mode> for ModeConfig {
    fn from(m: Mode) -> Self {
        match m {
            Mode::NonPreedit => ModeConfig::NonPreedit,
            Mode::Preedit => ModeConfig::Preedit,
            Mode::Surrounding => ModeConfig::Surrounding,
            Mode::Flow => ModeConfig::Flow,
            Mode::Turbo => ModeConfig::Turbo,
            Mode::Select => ModeConfig::Select,
            Mode::ForwardKeyEvent => ModeConfig::ForwardKeyEvent,
            Mode::ForwardSelect => ModeConfig::ForwardSelect,
            Mode::Off => ModeConfig::Off,
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
            MethodConfig::Telex2 => "telex2",
            MethodConfig::TelexVni => "telexvni",
            MethodConfig::TelexVniViqr => "telexvniviqr",
            MethodConfig::VniFrench => "vnifrench",
            MethodConfig::TelexW => "telexw",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        // Aliases mirror bamboo-core's `InputMethodPreset::from_str` so a
        // hand-edited or migrated file using its spellings still loads.
        Some(match key {
            "telex" => MethodConfig::Telex,
            "vni" => MethodConfig::Vni,
            "viqr" => MethodConfig::Viqr,
            "microsoft" | "microsoft layout" | "microsoft_layout" => MethodConfig::Microsoft,
            "telex2" | "telex 2" | "telex_2" => MethodConfig::Telex2,
            "telexvni" | "telex+vni" | "telex + vni" | "telex_vni" => MethodConfig::TelexVni,
            "telexvniviqr" | "telex+vni+viqr" | "telex + vni + viqr" | "telex_vni_viqr" => {
                MethodConfig::TelexVniViqr
            }
            "vnifrench" | "vni french" | "vni_french" | "vni ban phim tieng phap" => {
                MethodConfig::VniFrench
            }
            "telexw" | "telex w" | "telex_w" => MethodConfig::TelexW,
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
            ModeConfig::Flow => "flow",
            ModeConfig::Turbo => "turbo",
            ModeConfig::Select => "select",
            ModeConfig::ForwardKeyEvent => "forwardkeyevent",
            ModeConfig::ForwardSelect => "forwardselect",
            ModeConfig::Off => "off",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Some(match key {
            "nonpreedit" | "non-preedit" => ModeConfig::NonPreedit,
            "preedit" => ModeConfig::Preedit,
            "surrounding" => ModeConfig::Surrounding,
            "flow" => ModeConfig::Flow,
            "turbo" => ModeConfig::Turbo,
            "select" => ModeConfig::Select,
            "forwardkeyevent" | "forwardkey" | "forward-key-event" | "forward_key_event" => {
                ModeConfig::ForwardKeyEvent
            }
            "forwardselect"
            | "forwardkeyevent2"
            | "forwardkeyeventii"
            | "forward-key-event-2"
            | "forward_key_event_2" => ModeConfig::ForwardSelect,
            "off" => ModeConfig::Off,
            // The removed Default Typing mode migrates to Off (both here and
            // through the serde aliases on the enum, for old config files).
            "defaulttyping" | "default-typing" | "default_typing" => ModeConfig::Off,
            // The removed Auto mode migrates to the factory default.
            "auto" => ModeConfig::Preedit,
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
        .map(|t| {
            t.lines()
                .map(str::to_string)
                .filter(|l| !l.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

/// Keys of the quick toggles the panel popup shows by default — the menu the
/// engine shipped before the list became user-editable.
fn default_quick_menu() -> Vec<String> {
    ["macro", "auto_capitalize", "abbreviations", "auto_restore"]
        .into_iter()
        .map(str::to_string)
        .collect()
}

/// Display modes the panel popup showed before the radio group became
/// editable — the four main entries, not the uinput/technical ones.
fn default_menu_modes() -> Vec<String> {
    ["preedit", "nonpreedit", "surrounding", "off"]
        .into_iter()
        .map(str::to_string)
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
            backspace_tone: BackspaceToneConfig::ToneFirst,
            auto_restore: true,
            bypass_url: true,
            extra_url_chars: String::new(),
            extra_url_chars_enabled: false,
            w2u: W2uConfig::Disabled,
            bracket: BracketConfig::Disabled,
            em_dash: false,
            auto_capitalize: false,
            auto_capitalize_after: String::new(),
            auto_capitalize_after_enabled: false,
            double_space_period: false,
            macro_enabled: true,
            capitalize_macro: true,
            macro_in_off_mode: false,
            macro_expand_valid_words: false,
            time_format: "%H:%M".into(),
            date_format: "%d/%m/%Y".into(),
            language: "vi".into(),
            dictionary_enabled: false,
            abbreviations: true,
            cycle_mode_hotkey: String::new(),
            enabled_hotkey: String::new(),
            app_rule_hotkey: String::new(),
            emoji_hotkey: String::new(),
            emoji_enabled: false,
            hide_underline: false,
            commit_interval_ms: 20,
            backspace_interval_ms: 5,
            post_delay_ms: 10,
            unicode_channel: UnicodeChannel::Commit,
            clipboard_restore_delay_enabled: false,
            clipboard_restore_delay_ms: 600,
            echo_stale_enabled: false,
            echo_stale_ms: 150,
            idle_commit_enabled: false,
            idle_commit_ms: 700,
            flow_per_key: true,
            flow_per_key_delay_ms: 60,
            flow_backspace_retrack: false,
            experimental_features: false,
            show_toasts: true,
            debug_log: false,
            debug_log_text: false,
            x11_delete_without_report: false,
            preedit_for_xim: false,
            nav_key_while_preedit: "swallow".into(),
            app_rules: Vec::new(),
            custom_hotkeys: Vec::new(),
            quick_menu: default_quick_menu(),
            menu_modes: default_menu_modes(),
            menu: None,
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
        let Some(dir) = dirs::config_dir() else {
            return;
        };
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
        let Some(path) = Self::path() else {
            return Self::default();
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Self::default();
        };
        Self::from_toml(&text)
    }

    /// Parse the file. One bad value (a typo in `w2u`, say) must not discard
    /// every other setting — the GUI reads through here and would write the
    /// defaults back over the user's file.
    fn from_toml(text: &str) -> Self {
        if let Ok(config) = toml::from_str(text) {
            return config;
        }
        // Slow path, only for a broken file: keep the keys that parse.
        let Ok(table) = text.parse::<toml::Table>() else {
            crate::debug_log::log("config: unparsable TOML, using defaults");
            return Self::default();
        };
        let mut kept = toml::Table::new();
        for (key, value) in table {
            let one = toml::Value::Table([(key.clone(), value.clone())].into_iter().collect());
            match one.try_into::<Config>() {
                Ok(_) => {
                    kept.insert(key, value);
                }
                Err(e) => crate::debug_log::log(&format!("config: ignoring `{key}`: {e}")),
            }
        }
        toml::Value::Table(kept).try_into().unwrap_or_default()
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
        // Keep keys this build does not know about (an older/newer version or a
        // hand edit) instead of silently dropping them on the next save.
        let existing = std::fs::read_to_string(&path).unwrap_or_default();
        let text = merge(&existing, self).map_err(std::io::Error::other)?;
        std::fs::write(path, text)
    }

    /// Overwrite the file with this config verbatim — no merge. The
    /// restore-defaults button needs it: `save` merges, so an old `menu`
    /// design and keys this version no longer knows would survive a reset.
    pub fn save_fresh(&self) -> std::io::Result<()> {
        let Some(path) = Self::path() else {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "no config dir",
            ));
        };
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let text = toml::to_string_pretty(self).map_err(std::io::Error::other)?;
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
        assert!(c.enabled_hotkey.is_empty());
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
    fn forward_key_event_mode_key_round_trips() {
        let c: Config = toml::from_str("mode = \"forwardkeyevent\"").unwrap();
        assert_eq!(c.mode, ModeConfig::ForwardKeyEvent);
        assert_eq!(c.mode.key(), "forwardkeyevent");
        assert_eq!(
            ModeConfig::from_key("forwardkey"),
            Some(ModeConfig::ForwardKeyEvent),
            "the short alias must keep working"
        );
        // Both compatibility switches ship off: an old config keeps today's
        // behaviour, and only the settings window turns them on.
        let c = Config::default();
        assert!(!c.x11_delete_without_report);
        assert!(!c.preedit_for_xim);
        let c: Config =
            toml::from_str("x11_delete_without_report = true\npreedit_for_xim = true\n").unwrap();
        assert!(c.x11_delete_without_report);
        assert!(c.preedit_for_xim);
        let c = Config::from_toml("mode = \"preedit\"\n");
        assert!(!c.x11_delete_without_report && !c.preedit_for_xim);

        let c: Config = toml::from_str("mode = \"forwardselect\"").unwrap();
        assert_eq!(c.mode, ModeConfig::ForwardSelect);
        assert_eq!(c.mode.key(), "forwardselect");
        assert_eq!(
            ModeConfig::from_key("forwardkeyevent2"),
            Some(ModeConfig::ForwardSelect)
        );
    }

    #[test]
    fn nav_key_while_preedit_key_round_trips() {
        // The shipped behavior stays the default: an old config without the
        // key keeps swallowing the first navigation key.
        assert_eq!(Config::default().nav_key_while_preedit, "swallow");
        let c: Config = toml::from_str("nav_key_while_preedit = \"forward\"").unwrap();
        assert_eq!(c.nav_key_while_preedit, "forward");
        let c = Config::from_toml("mode = \"preedit\"\n");
        assert_eq!(c.nav_key_while_preedit, "swallow");
        // The raw string is stored as-is; the engine normalizes unknown
        // values to swallow (same tolerance as the GUI language value).
        let c: Config = toml::from_str("nav_key_while_preedit = \"weird\"").unwrap();
        assert_eq!(c.nav_key_while_preedit, "weird");
    }

    #[test]
    fn quick_menu_round_trips_and_defaults_to_the_old_menu() {
        // The default is the menu the engine shipped before the list became
        // editable, so an upgrade changes nothing on the panel.
        assert_eq!(
            Config::default().quick_menu,
            vec![
                "macro".to_string(),
                "auto_capitalize".to_string(),
                "abbreviations".to_string(),
                "auto_restore".to_string(),
            ]
        );
        let c: Config = toml::from_str("quick_menu = [\"bypass_url\", \"em_dash\"]").unwrap();
        assert_eq!(
            c.quick_menu,
            vec!["bypass_url".to_string(), "em_dash".to_string()]
        );
        // A file without the key still parses (the engine must not drop
        // settings it does not recognise on the next save).
        let c: Config = toml::from_str("mode = \"preedit\"\n").unwrap();
        assert_eq!(c.quick_menu.len(), 4);
    }

    #[test]
    fn menu_modes_round_trips_and_defaults_to_the_main_four() {
        assert_eq!(
            Config::default().menu_modes,
            vec![
                "preedit".to_string(),
                "nonpreedit".to_string(),
                "surrounding".to_string(),
                "off".to_string(),
            ]
        );
        let c: Config = toml::from_str("menu_modes = [\"flow\", \"off\", \"preedit\"]").unwrap();
        assert_eq!(
            c.menu_modes,
            vec!["flow".to_string(), "off".to_string(), "preedit".to_string()]
        );
        let c: Config = toml::from_str("mode = \"preedit\"\n").unwrap();
        assert_eq!(c.menu_modes.len(), 4);
    }

    #[test]
    fn menu_tree_round_trips_through_toml() {
        // Absent key: None — the engine synthesizes the shipped menu shape.
        let c: Config = toml::from_str("mode = \"preedit\"\n").unwrap();
        assert_eq!(c.menu, None);
        assert!(!Config::default().default_menu().is_empty());
        // The synthesized default: full charset/method groups, the display
        // group from `menu_modes` (so an upgrade shows the same four modes).
        let d = Config::default().default_menu();
        let radios = |g: &str| {
            d.iter().find_map(|n| match n {
                MenuNode::Radios { group, children } if group == g => Some(children.clone()),
                _ => None,
            })
        };
        assert_eq!(
            radios("charset"),
            Some(
                crate::ibus_prop::CHARSETS
                    .iter()
                    .map(|(k, ..)| k.to_string())
                    .collect::<Vec<_>>()
            )
        );
        assert_eq!(radios("display"), Some(Config::default().menu_modes));

        let toml_text = r#"
[[menu]]
type = "folder"
label = "Cá nhân"
children = [ { type = "toggle", key = "em_dash" }, { type = "separator" } ]

[[menu]]
type = "radios"
group = "display"
children = ["preedit", "flow"]
"#;
        let c: Config = toml::from_str(toml_text).unwrap();
        let menu = c.menu.expect("menu key present");
        assert_eq!(
            menu,
            vec![
                MenuNode::Folder {
                    label: "Cá nhân".into(),
                    children: vec![
                        MenuNode::Toggle {
                            key: "em_dash".into()
                        },
                        MenuNode::Separator,
                    ],
                },
                MenuNode::Radios {
                    group: "display".into(),
                    children: vec!["preedit".into(), "flow".into()],
                },
            ]
        );
        // A broken menu node is dropped key-wise by from_toml's salvage path
        // (bad TOML value → default for that key), never fatal.
        let c = Config::from_toml("menu = 42\n");
        assert_eq!(c.menu, None);
    }

    #[test]
    fn save_keeps_keys_this_build_does_not_know() {
        let config = Config {
            method: MethodConfig::Vni,
            ..Config::default()
        };
        let text = merge("method = \"telex\"\nw2u_mode = \"disabled\"\n", &config).unwrap();
        let back: toml::Table = toml::from_str(&text).unwrap();
        assert_eq!(back["method"], toml::Value::String("vni".into()));
        assert_eq!(back["w2u_mode"], toml::Value::String("disabled".into()));
    }

    #[test]
    fn one_bad_value_does_not_discard_the_rest() {
        let c =
            Config::from_toml("w2u = \"everywhre\"\nmode = \"preedit\"\nauto_restore = false\n");
        assert_eq!(c.mode, ModeConfig::Preedit, "the good keys must survive");
        assert!(!c.auto_restore);
        assert_eq!(c.w2u, W2uConfig::Disabled, "the bad key falls back");
        assert!(c.enabled, "missing keys take defaults");
    }

    #[test]
    fn app_rule_matches_substring_case_insensitive_first_win() {
        let rules = vec![
            AppRule {
                pattern: "fire".into(),
                mode: ModeConfig::Preedit,
            },
            AppRule {
                pattern: "fox".into(),
                mode: ModeConfig::Off,
            },
            AppRule {
                pattern: String::new(),
                mode: ModeConfig::Surrounding,
            },
        ];
        // The transport prefix is stripped before matching.
        assert_eq!(
            app_rule_mode(&rules, Some("wayland:Firefox")),
            Some(ModeConfig::Preedit)
        );
        // The first rule misses `nightfox`, the second hits; the empty
        // pattern must not catch everything after them.
        assert_eq!(
            app_rule_mode(&rules, Some("gtk3-im:nightfox")),
            Some(ModeConfig::Off)
        );
        assert_eq!(app_rule_mode(&rules, Some("Chromium")), None);
        assert_eq!(app_rule_mode(&rules, None), None);
    }

    #[test]
    fn app_rule_none_matches_only_unnamed_clients() {
        let rules = vec![AppRule {
            pattern: "none".into(),
            mode: ModeConfig::Surrounding,
        }];
        assert_eq!(app_rule_mode(&rules, None), Some(ModeConfig::Surrounding));
        assert_eq!(app_rule_mode(&rules, Some("xim")), None);
        assert_eq!(app_rule_mode(&rules, Some("wayland:Firefox")), None);
    }

    #[test]
    fn roundtrips_through_toml() {
        let c = Config {
            method: MethodConfig::Vni,
            mode: ModeConfig::Preedit,
            modern_tone_style: false,
            app_rules: vec![AppRule {
                pattern: "kate".into(),
                mode: ModeConfig::Preedit,
            }],
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
        // Missing app_rules stay empty — nothing ships pre-added.
        assert!(c.app_rules.is_empty());
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

    #[test]
    fn flow_per_key_round_trips_and_defaults_on() {
        let c: Config = toml::from_str("flow_per_key = false").unwrap();
        assert!(!c.flow_per_key);
        assert!(Config::default().flow_per_key);
    }

    #[test]
    fn flow_per_key_delay_round_trips_and_defaults_to_60() {
        let c: Config = toml::from_str("flow_per_key_delay_ms = 120").unwrap();
        assert_eq!(c.flow_per_key_delay_ms, 120);
        assert_eq!(Config::default().flow_per_key_delay_ms, 60);
    }

    #[test]
    fn flow_backspace_retrack_round_trips_and_defaults_off() {
        let c: Config = toml::from_str("flow_backspace_retrack = true").unwrap();
        assert!(c.flow_backspace_retrack);
        assert!(!Config::default().flow_backspace_retrack);
    }

    /// The old mode names are gone for good: an old file resets the mode to
    /// the default instead of failing to load — the fresh-start contract.
    #[test]
    fn old_mode_value_falls_back_to_default() {
        // Assembled at runtime so no rename sweep rewrites the old literal.
        let old = format!("smo{}oth", "o");
        let c = Config::from_toml(&format!("mode = \"{old}\"\nauto_restore = false\n"));
        assert_eq!(c.mode, ModeConfig::Preedit);
        assert!(!c.auto_restore);
    }

    #[test]
    fn new_method_keys_round_trip_with_aliases() {
        for (key, m) in [
            ("telex2", MethodConfig::Telex2),
            ("telexvni", MethodConfig::TelexVni),
            ("telexvniviqr", MethodConfig::TelexVniViqr),
            ("vnifrench", MethodConfig::VniFrench),
            ("telexw", MethodConfig::TelexW),
        ] {
            let c: Config = toml::from_str(&format!("method = \"{key}\"")).unwrap();
            assert_eq!(c.method, m, "{key}");
            assert_eq!(m.key(), key);
            assert_eq!(MethodConfig::from_key(key), Some(m));
        }
        // bamboo-core preset spellings load too, so a hand-edited file
        // using them still works.
        assert_eq!(
            MethodConfig::from_key("telex+vni+viqr"),
            Some(MethodConfig::TelexVniViqr)
        );
        assert_eq!(
            MethodConfig::from_key("vni french"),
            Some(MethodConfig::VniFrench)
        );
        assert_eq!(
            MethodConfig::from_key("telex w"),
            Some(MethodConfig::TelexW)
        );
        // The four shipped keys keep their exact spelling.
        assert_eq!(MethodConfig::from_key("telex"), Some(MethodConfig::Telex));
        assert_eq!(
            MethodConfig::from_key("microsoft"),
            Some(MethodConfig::Microsoft)
        );
        assert_eq!(MethodConfig::from_key("nope"), None);
    }

    #[test]
    fn picker_choice_updates_the_winning_rule() {
        let mut c = Config {
            app_rules: vec![
                AppRule {
                    pattern: "chrome".into(),
                    mode: ModeConfig::Preedit,
                },
                AppRule {
                    pattern: "chromium".into(),
                    mode: ModeConfig::Preedit,
                },
            ],
            ..Config::default()
        };
        // chrome is covered by an existing rule; a pick for a chrome target
        // must flip THAT rule in place.
        assert_eq!(
            crate::config::app_rule_mode(&c.app_rules, Some("wayland:chrome")),
            Some(ModeConfig::Preedit)
        );
        c.set_rule_mode_for_target("chrome", ModeConfig::Off);
        assert_eq!(
            crate::config::app_rule_mode(&c.app_rules, Some("wayland:chrome")),
            Some(ModeConfig::Off)
        );
        // Substring overlap: "chromium" matches the "chromium" rule first —
        // the pick must update the winning one, not the "chrome" prefix rule.
        c.set_rule_mode_for_target("chromium", ModeConfig::Flow);
        let chromium = c
            .app_rules
            .iter()
            .find(|r| r.pattern == "chromium")
            .expect("net rule");
        assert_eq!(chromium.mode, ModeConfig::Flow);
        let chrome = c
            .app_rules
            .iter()
            .find(|r| r.pattern == "chrome")
            .expect("net rule");
        assert_eq!(
            chrome.mode,
            ModeConfig::Off,
            "the chrome rule stays untouched"
        );
        // An app no rule covers gets a fresh rule at the top.
        let before = c.app_rules.len();
        c.set_rule_mode_for_target("e2epick", ModeConfig::Surrounding);
        assert_eq!(c.app_rules.len(), before + 1);
        assert_eq!(c.app_rules[0].pattern, "e2epick");
        assert_eq!(
            crate::config::app_rule_mode(&c.app_rules, Some("e2epick")),
            Some(ModeConfig::Surrounding)
        );
        // Re-picking updates in place instead of stacking.
        c.set_rule_mode_for_target("e2epick", ModeConfig::Off);
        assert_eq!(c.app_rules.len(), before + 1);
        assert_eq!(
            crate::config::app_rule_mode(&c.app_rules, Some("e2epick")),
            Some(ModeConfig::Off)
        );
    }

    #[test]
    fn rule_patterns_tolerate_transport_prefixes() {
        // The app-rule picker once saved patterns with the focus
        // extension's transport prefix; those rules must still fire.
        assert!(app_rule_matches("wayland:chrome", Some("wayland:chrome")));
        assert!(app_rule_matches("x11:chrome", Some("x11:chrome")));
        assert!(app_rule_matches("chrome", Some("wayland:chrome")));
    }
}
