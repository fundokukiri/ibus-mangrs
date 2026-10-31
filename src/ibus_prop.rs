//! `IBusProperty` encoding for the input-source menu that GNOME Shell and
//! other panels show for the active engine.
//!
//! The wire shapes were captured from ibus 1.5.34 (`dbus-monitor` on an
//! ibus-bamboo session):
//!
//! ```text
//! IBusPropList ('IBusPropList', a{sv} {}, av [ property... ])
//! IBusProperty ('IBusProperty', a{sv} {}, key, type, label: v, icon, tooltip: v,
//!               sensitive, visible, state, sub_props: v, symbol: v)
//! ```
//!
//! `RegisterProperties` and `UpdateProperty` are signals `(v)` emitted by the
//! engine; the panel receives them through ibus-daemon. Clicks come back as
//! the `PropertyActivate(s name, u state)` method.

use dbus::arg::{RefArg, Variant};

use crate::config::{ClickActionConfig, Config};
use crate::ibus_text::Props;

pub const TYPE_NORMAL: u32 = 0;
pub const TYPE_TOGGLE: u32 = 1;
pub const TYPE_RADIO: u32 = 2;
pub const TYPE_MENU: u32 = 3;
pub const TYPE_SEPARATOR: u32 = 4;

pub const STATE_UNCHECKED: u32 = 0;
pub const STATE_CHECKED: u32 = 1;

/// Project homepage — must match `data/ibus-mangrs.xml`.
pub const HOMEPAGE: &str = "https://github.com/fundokukiri/ibus-mangrs";

pub const KEY_ABOUT: &str = "about";
pub const KEY_SHORTCUTS: &str = "shortcuts";
pub const KEY_SPELL_CHECK: &str = "spell_check";
pub const KEY_ABBREV: &str = "abbreviations";
pub const KEY_CLICK_ACTION: &str = "click";
pub const KEY_ENABLED: &str = "enabled";
pub const KEY_CHARSET: &str = "charset";
pub const KEY_METHOD: &str = "method";
pub const KEY_DISPLAY: &str = "display";

// Quick-settings group keys (toggles):
pub const KEY_MACROS: &str = "macro";
pub const KEY_MACRO_OFF: &str = "macro_off";
pub const KEY_MACRO_EDITOR: &str = "macro_editor";
pub const KEY_AUTO_CAPITALIZE: &str = "auto_capitalize";
pub const KEY_DICTIONARY: &str = "dictionary";
pub const KEY_FREE_TONE: &str = "free_tone";
pub const KEY_MODERN_TONE: &str = "modern_tone";
pub const KEY_HIDE_UNDERLINE: &str = "hide_underline";
pub const KEY_AUTO_RESTORE: &str = "auto_restore";
pub const KEY_EM_DASH: &str = "em_dash";
pub const KEY_DOUBLE_SPACE: &str = "double_space";

/// All toggle keys `activate_property` understands, with the config field
/// they map to — the single source of truth for menu + handler.
pub fn toggle_label(key: &str) -> Option<(&'static str, bool)> {
    Some(match key {
        KEY_ENABLED => ("Bật tiếng Việt", true),
        KEY_MACROS => ("Gõ tắt", true),
        KEY_AUTO_CAPITALIZE => ("Tự động viết hoa sau . ! ?", false),
        KEY_ABBREV => ("Giữ đ trong từ viết tắt (ddc → đc)", true),
        KEY_AUTO_RESTORE => ("Tự khôi phục từ tiếng Anh", true),
        _ => return None,
    })
}

/// (menu key, label) for every output charset.
pub const CHARSETS: &[(&str, &str)] = &[
    ("unicode", "Unicode"),
    ("nfd", "Unicode tổ hợp"),
    ("tcvn3", "TCVN3"),
    ("vni", "VNI"),
    ("viqr", "VIQR"),
    ("viscii", "VISCII"),
    ("vps", "VPS"),
    ("cp1258", "Windows 1258"),
    ("bkhcm", "BKHCM"),
    ("vietwarex", "Vietware X"),
    ("vietwarefull", "Vietware Full"),
];

/// (menu key, label) for every input method.
pub const METHODS: &[(&str, &str)] = &[
    ("telex", "Telex"),
    ("vni", "VNI"),
    ("viqr", "VIQR"),
    ("microsoft", "Microsoft layout"),
];

/// (menu key, label) for every display mode. The menu popup only shows the
/// four main entries (MAIN_MODES); the uinput and default-typing modes stay
/// in the GUI — duplicated near-identical choices only widen the popup.
pub const MODES: &[(&str, &str)] = &[
    ("nonpreedit", "Không gạch chân"),
    ("preedit", "Gạch chân (preedit)"),
    ("surrounding", "Surrounding text"),
    ("smooth", "Smooth (uinput)"),
    ("supersmooth", "Super Smooth (uinput)"),
    ("select", "Select (uinput)"),
    ("off", "OFF"),
    ("defaulttyping", "Gõ mặc định"),
];

/// The display modes shown in the menu popup; the rest are GUI-only.
const MAIN_MODES: &[&str] = &["preedit", "nonpreedit", "surrounding", "off"];

/// One item in the engine's property list.
pub struct Prop {
    pub key: String,
    pub kind: u32,
    pub label: String,
    pub tooltip: String,
    pub state: u32,
    pub sub: Vec<Prop>,
}

impl Prop {
    fn new(key: impl Into<String>, kind: u32, label: impl Into<String>, state: u32) -> Self {
        let label = label.into();
        Self {
            key: key.into(),
            kind,
            label: label.clone(),
            tooltip: label,
            state,
            sub: Vec::new(),
        }
    }

    pub fn normal(key: &str, label: impl Into<String>) -> Self {
        Self::new(key, TYPE_NORMAL, label, STATE_UNCHECKED)
    }

    pub fn separator() -> Self {
        Self::new("-", TYPE_SEPARATOR, "", STATE_UNCHECKED)
    }

    pub fn toggle(key: &str, label: &str, on: bool) -> Self {
        let state = if on { STATE_CHECKED } else { STATE_UNCHECKED };
        Self::new(key, TYPE_TOGGLE, label, state)
    }

    pub fn radio(key: &str, label: &str, checked: bool) -> Self {
        let state = if checked {
            STATE_CHECKED
        } else {
            STATE_UNCHECKED
        };
        Self::new(key, TYPE_RADIO, label, state)
    }

    pub fn menu(key: &str, label: &str, sub: Vec<Prop>) -> Self {
        let mut p = Self::new(key, TYPE_MENU, label, STATE_UNCHECKED);
        p.sub = sub;
        p
    }

    /// The serialized `IBusProperty`, ready for a `v` slot.
    pub fn variant(&self) -> Variant<Box<dyn RefArg>> {
        Variant(Box::new((
            "IBusProperty".to_string(),
            Props::new(),
            self.key.clone(),
            self.kind,
            crate::ibus_text::plain_text(&self.label),
            String::new(), // icon: unused
            crate::ibus_text::plain_text(&self.tooltip),
            true, // sensitive
            true, // visible
            self.state,
            prop_list_variant(&self.sub),
            crate::ibus_text::plain_text(""), // symbol
        )) as Box<dyn RefArg>)
    }
}

/// The serialized `IBusPropList`, ready for a `v` slot.
pub fn prop_list_variant(props: &[Prop]) -> Variant<Box<dyn RefArg>> {
    Variant(Box::new((
        "IBusPropList".to_string(),
        Props::new(),
        props.iter().map(Prop::variant).collect::<Vec<_>>(),
    )) as Box<dyn RefArg>)
}

/// The whole menu for the current settings. Called on `Enable` and on every
/// `FocusIn`, so the check marks always match the saved config.
pub fn properties(config: &Config) -> Vec<Prop> {
    let charset_items = CHARSETS
        .iter()
        .map(|(key, label)| {
            Prop::radio(
                &format!("{KEY_CHARSET}:{key}"),
                label,
                config.charset.key() == *key,
            )
        })
        .collect();
    let method_items = METHODS
        .iter()
        .map(|(key, label)| {
            Prop::radio(
                &format!("{KEY_METHOD}:{key}"),
                label,
                config.method.key() == *key,
            )
        })
        .collect();
    let display_items = MODES
        .iter()
        .filter(|(key, _)| MAIN_MODES.contains(key))
        .map(|(key, label)| {
            Prop::radio(
                &format!("{KEY_DISPLAY}:{key}"),
                label,
                config.mode.key() == *key,
            )
        })
        .collect();
    let click_items = [
        (
            ClickActionConfig::Bamboo,
            "click:bamboo",
            "Bamboo (mặc định)",
        ),
        (ClickActionConfig::Keep, "click:keep", "Giữ từ khi click"),
        (
            ClickActionConfig::Relocate,
            "click:relocate",
            "Về chỗ cũ khi click",
        ),
    ]
    .iter()
    .map(|(value, key, label)| Prop::radio(key, label, config.click_action == *value))
    .collect();

    // GNOME sizes the popup to the widest label, so the quick settings stay
    // short; the details live in the tooltips. Only the essentials stay in
    // the popup — everything else is in the control panel (GUI).
    let mut quick = vec![
        Prop::toggle(KEY_MACROS, "Gõ tắt", config.macro_enabled),
        Prop::toggle(KEY_AUTO_CAPITALIZE, "Tự viết hoa", config.auto_capitalize),
        Prop::normal(KEY_MACRO_EDITOR, "Mở bảng gõ tắt…"),
        Prop::toggle(KEY_ABBREV, "Viết tắt", config.abbreviations),
        Prop::toggle(
            KEY_AUTO_RESTORE,
            "Khôi phục từ nước ngoài",
            config.auto_restore,
        ),
    ];
    let quick_hints = [
        (KEY_AUTO_CAPITALIZE, "Tự động viết hoa sau . ! ? và Enter"),
        (KEY_ABBREV, "Giữ đ trong từ viết tắt: ddc → đc, ddk → đk"),
        (
            KEY_AUTO_RESTORE,
            "Từ không phải âm tiết Việt trả về chuỗi phím gốc",
        ),
    ];
    for (key, hint) in quick_hints {
        if let Some(p) = quick.iter_mut().find(|p| p.key == key) {
            p.tooltip = hint.into();
        }
    }

    let mut about = Prop::normal(KEY_ABOUT, format!("MăngRS {}", env!("CARGO_PKG_VERSION")));
    about.tooltip = "Mở trang GitHub của MăngRS".into();

    vec![
        about,
        Prop::separator(),
        Prop::toggle(KEY_ENABLED, "Bật tiếng Việt", config.enabled),
        Prop::menu(KEY_CHARSET, "Bảng mã", charset_items),
        Prop::menu(KEY_METHOD, "Kiểu gõ", method_items),
        Prop::menu(KEY_DISPLAY, "Chế độ gõ", display_items),
        Prop::menu(KEY_CLICK_ACTION, "Hành vi khi click", click_items),
        Prop::menu("quick", "Cấu hình nhanh", quick),
        Prop::normal(KEY_SHORTCUTS, "Cài đặt…"),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use dbus::arg::RefArg;

    #[test]
    fn menu_has_the_expected_shape() {
        let props = properties(&Config::default());
        assert_eq!(props[0].kind, TYPE_NORMAL);
        assert_eq!(
            props[0].label,
            format!("MăngRS {}", env!("CARGO_PKG_VERSION"))
        );
        assert_eq!(props[1].kind, TYPE_SEPARATOR);
        assert_eq!(props[2].key, KEY_ENABLED);
        assert_eq!(props[2].kind, TYPE_TOGGLE);
        assert_eq!(props[3].key, KEY_CHARSET);
        assert_eq!(props[3].sub.len(), CHARSETS.len());
        assert_eq!(props[4].key, KEY_METHOD);
        assert_eq!(props[4].sub.len(), METHODS.len());
        assert_eq!(props[5].key, KEY_DISPLAY);
        // Only the main modes are in the popup; the rest live in the GUI.
        assert_eq!(props[5].sub.len(), MAIN_MODES.len());
        assert_eq!(props[6].key, KEY_CLICK_ACTION);
        assert_eq!(props[6].sub.len(), 3);
        assert_eq!(props[7].key, "quick");
        assert_eq!(props[7].sub.len(), 5);
        assert_eq!(props[8].key, KEY_SHORTCUTS);
        assert_eq!(props[8].kind, TYPE_NORMAL);
    }

    #[test]
    fn checked_states_follow_the_config() {
        let config = Config {
            charset: crate::config::CharsetConfig::Vni,
            method: crate::config::MethodConfig::Viqr,
            mode: crate::config::ModeConfig::Preedit,
            click_action: crate::config::ClickActionConfig::Keep,
            auto_correct: false,
            ..Config::default()
        };
        let props = properties(&config);
        fn checked(p: &[Prop]) -> Vec<&str> {
            p.iter()
                .filter(|c| c.state == STATE_CHECKED)
                .map(|c| c.key.as_str())
                .collect()
        }
        assert_eq!(checked(&props[3].sub), vec!["charset:vni"]);
        assert_eq!(checked(&props[4].sub), vec!["method:viqr"]);
        assert_eq!(checked(&props[5].sub), vec!["display:preedit"]);
        assert_eq!(checked(&props[6].sub), vec!["click:keep"]);
        assert_eq!(props[2].state, STATE_CHECKED);
        // Quick group: macros, abbreviations and auto-restore on (defaults).
        assert_eq!(
            checked(&props[7].sub),
            vec!["macro", "abbreviations", "auto_restore"]
        );
    }

    #[test]
    fn serialized_types_match_ibus() {
        let props = properties(&Config::default());
        assert_eq!(
            props[0].variant().0.signature().to_string(),
            "(sa{sv}suvsvbbuvv)"
        );
        assert_eq!(
            prop_list_variant(&props).0.signature().to_string(),
            "(sa{sv}av)"
        );
    }
}
