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

use crate::config::Config;
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
pub const KEY_ABBREV: &str = "abbreviations";
pub const KEY_ENABLED: &str = "enabled";
pub const KEY_CHARSET: &str = "charset";
pub const KEY_METHOD: &str = "method";
pub const KEY_DISPLAY: &str = "display";

// Quick-settings group keys (toggles). The popup shows the user's own
// selection from `QUICK_TOGGLES` (`config.quick_menu`); the panel is
// deliberately short, everything else is in the GUI.
pub const KEY_MACROS: &str = "macro";
pub const KEY_MACRO_EDITOR: &str = "macro_editor";
pub const KEY_AUTO_CAPITALIZE: &str = "auto_capitalize";
pub const KEY_AUTO_RESTORE: &str = "auto_restore";
pub const KEY_AUTO_CORRECT: &str = "auto_correct";
pub const KEY_BYPASS_URL: &str = "bypass_url";
pub const KEY_DICTIONARY: &str = "dictionary";
pub const KEY_DOUBLE_SPACE: &str = "double_space";
pub const KEY_EM_DASH: &str = "em_dash";
pub const KEY_FREE_TONE: &str = "free_tone";
pub const KEY_MODERN_TONE: &str = "modern_tone";
pub const KEY_HIDE_UNDERLINE: &str = "hide_underline";

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

/// (menu key, label) for every display mode. The menu popup shows the user's
/// `config.menu_modes` selection (default: the four main entries); the rest
/// are reachable through the GUI — duplicated near-identical choices only
/// widen the popup.
pub const MODES: &[(&str, &str)] = &[
    ("nonpreedit", "Không gạch chân"),
    ("preedit", "Gạch chân (preedit)"),
    ("surrounding", "Surrounding text"),
    ("smooth", "Smooth (uinput)"),
    ("supersmooth", "Super Smooth (uinput)"),
    ("select", "Select (uinput)"),
    ("forwardkeyevent", "ForwardKeyEvent (Backspace)"),
    ("forwardselect", "ForwardKeyEvent II (Shift+Left)"),
    ("off", "OFF"),
    ("defaulttyping", "Gõ mặc định"),
];

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

/// One toggle the panel popup can show. The user picks the set (`config
/// .quick_menu`); every entry here is wired to exactly one config field, so
/// the panel and the GUI always edit the same value.
pub struct QuickToggle {
    pub key: &'static str,
    /// Short label: GNOME sizes the popup to the widest one.
    pub label: &'static str,
    pub tooltip: &'static str,
    pub get: fn(&Config) -> bool,
    pub set: fn(&mut Config, bool),
}

/// Every toggle the popup CAN show, in GUI presentation order. Adding a
/// candidate means one entry here plus the field it edits — the panel menu,
/// its click handling and the GUI section all read this table.
pub const QUICK_TOGGLES: &[QuickToggle] = &[
    QuickToggle {
        key: KEY_MACROS,
        label: "Gõ tắt",
        tooltip: "Mở rộng cụm tắt thành câu đầy đủ",
        get: |c| c.macro_enabled,
        set: |c, on| c.macro_enabled = on,
    },
    QuickToggle {
        key: KEY_AUTO_CAPITALIZE,
        label: "Tự viết hoa",
        tooltip: "Tự động viết hoa sau . ! ? và Enter",
        get: |c| c.auto_capitalize,
        set: |c, on| c.auto_capitalize = on,
    },
    QuickToggle {
        key: KEY_ABBREV,
        label: "Viết tắt",
        tooltip: "Giữ đ trong từ viết tắt: ddc → đc, ddk → đk",
        get: |c| c.abbreviations,
        set: |c, on| c.abbreviations = on,
    },
    QuickToggle {
        key: KEY_AUTO_RESTORE,
        label: "Khôi phục từ nước ngoài",
        tooltip: "Từ không phải âm tiết Việt trả về chuỗi phím gốc",
        get: |c| c.auto_restore,
        set: |c, on| c.auto_restore = on,
    },
    QuickToggle {
        key: KEY_AUTO_CORRECT,
        label: "Sửa từ sai",
        tooltip: "Từ không hợp lệ trả về chuỗi phím gốc",
        get: |c| c.auto_correct,
        set: |c, on| c.auto_correct = on,
    },
    QuickToggle {
        key: KEY_BYPASS_URL,
        label: "Không gõ URL",
        tooltip: "Từ ngay sau / : @ . _ ~ = # & % ? giữ nguyên, không thêm dấu",
        get: |c| c.bypass_url,
        set: |c, on| c.bypass_url = on,
    },
    QuickToggle {
        key: KEY_DICTIONARY,
        label: "Từ điển của tôi",
        tooltip: "Từ trong từ điển không bị khôi phục về chuỗi phím",
        get: |c| c.dictionary_enabled,
        set: |c, on| c.dictionary_enabled = on,
    },
    QuickToggle {
        key: KEY_DOUBLE_SPACE,
        label: "2 dấu cách = chấm",
        tooltip: "Hai dấu cách chèn '. ' (thử nghiệm)",
        get: |c| c.double_space_period,
        set: |c, on| c.double_space_period = on,
    },
    QuickToggle {
        key: KEY_EM_DASH,
        label: "Gạch ngang dài",
        tooltip: "-- thành —",
        get: |c| c.em_dash,
        set: |c, on| c.em_dash = on,
    },
    QuickToggle {
        key: KEY_FREE_TONE,
        label: "Dấu tự do",
        tooltip: "Cho phép dấu gõ sai vị trí trong từ: hoangf → hoàng",
        get: |c| c.free_tone_marking,
        set: |c, on| c.free_tone_marking = on,
    },
    QuickToggle {
        key: KEY_MODERN_TONE,
        label: "Kiểu dấu mới",
        tooltip: "hòa (chuẩn mới) thay vì hoà (truyền thống)",
        get: |c| c.modern_tone_style,
        set: |c, on| c.modern_tone_style = on,
    },
    QuickToggle {
        key: KEY_HIDE_UNDERLINE,
        label: "Ẩn gạch chân",
        tooltip: "Preedit không có gạch chân (tùy app có nhận không)",
        get: |c| c.hide_underline,
        set: |c, on| c.hide_underline = on,
    },
];

/// The registered toggle with this menu key, if any.
pub fn quick_toggle(key: &str) -> Option<&'static QuickToggle> {
    QUICK_TOGGLES.iter().find(|t| t.key == key)
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
    // The display group is also user-defined (`config.menu_modes`, edited in
    // the GUI): the listed modes in list order, unknown keys skipped. The
    // default is the four main entries, so an upgrade shows the same group.
    let display_items = config
        .menu_modes
        .iter()
        .filter_map(|key| {
            let (_, label) = MODES.iter().find(|(k, _)| k == key)?;
            Some(Prop::radio(
                &format!("{KEY_DISPLAY}:{key}"),
                label,
                config.mode.key() == key.as_str(),
            ))
        })
        .collect();
    // The quick group is the user's own selection (`config.quick_menu`,
    // edited in the GUI): each key that names a registered toggle shows up in
    // list order, unknown keys are skipped. The editor action stays fixed —
    // it is not a toggle, so it is not selectable.
    let quick = std::iter::once(Prop::normal(KEY_MACRO_EDITOR, "Mở bảng gõ tắt…"))
        .chain(config.quick_menu.iter().filter_map(|key| {
            let t = quick_toggle(key)?;
            let mut p = Prop::toggle(t.key, t.label, (t.get)(config));
            p.tooltip = t.tooltip.into();
            Some(p)
        }))
        .collect();

    let mut about = Prop::normal(KEY_ABOUT, format!("MăngRS {}", env!("CARGO_PKG_VERSION")));
    about.tooltip = "Mở trang GitHub của MăngRS".into();

    vec![
        about,
        Prop::separator(),
        Prop::toggle(KEY_ENABLED, "Bật tiếng Việt", config.enabled),
        Prop::menu(KEY_CHARSET, "Bảng mã", charset_items),
        Prop::menu(KEY_METHOD, "Kiểu gõ", method_items),
        Prop::menu(KEY_DISPLAY, "Chế độ gõ", display_items),
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
        // The default selection is the four main modes; the rest are one
        // checkbox away in the GUI.
        assert_eq!(props[5].sub.len(), Config::default().menu_modes.len());
        assert_eq!(props[6].key, "quick");
        assert_eq!(props[6].sub.len(), 5);
        assert_eq!(props[7].key, KEY_SHORTCUTS);
        assert_eq!(props[7].kind, TYPE_NORMAL);
    }

    #[test]
    fn checked_states_follow_the_config() {
        let config = Config {
            charset: crate::config::CharsetConfig::Vni,
            method: crate::config::MethodConfig::Viqr,
            mode: crate::config::ModeConfig::Preedit,
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
        assert_eq!(props[2].state, STATE_CHECKED);
        // Quick group: macros, abbreviations and auto-restore on (defaults).
        assert_eq!(
            checked(&props[6].sub),
            vec!["macro", "abbreviations", "auto_restore"]
        );
    }

    #[test]
    fn quick_menu_is_user_defined() {
        let config = Config {
            quick_menu: vec![
                "bypass_url".into(),
                "macro".into(),
                "not_a_toggle".into(),
                "em_dash".into(),
            ],
            bypass_url: false,
            em_dash: true,
            ..Config::default()
        };
        let props = properties(&config);
        let quick = &props[6];
        // Unknown keys vanish; the listed order is the menu order; the fixed
        // editor action stays first.
        assert_eq!(
            quick.sub.iter().map(|p| p.key.as_str()).collect::<Vec<_>>(),
            vec!["macro_editor", "bypass_url", "macro", "em_dash"]
        );
        // Check marks come from the config through the registry.
        let on: Vec<&str> = quick
            .sub
            .iter()
            .filter(|p| p.state == STATE_CHECKED)
            .map(|p| p.key.as_str())
            .collect();
        assert_eq!(on, vec!["macro", "em_dash"]);
    }

    #[test]
    fn empty_quick_menu_leaves_only_the_fixed_action() {
        let config = Config {
            quick_menu: Vec::new(),
            ..Config::default()
        };
        let props = properties(&config);
        let quick = &props[6];
        assert_eq!(quick.sub.len(), 1);
        assert_eq!(quick.sub[0].key, KEY_MACRO_EDITOR);
    }

    #[test]
    fn display_group_is_user_defined() {
        let config = Config {
            menu_modes: vec![
                "off".into(),
                "smooth".into(),
                "not_a_mode".into(),
                "preedit".into(),
            ],
            ..Config::default()
        };
        let props = properties(&config);
        let display = &props[5];
        // Unknown keys vanish; the listed order is the menu order — a mode
        // outside the default selection shows up once it is listed.
        assert_eq!(
            display
                .sub
                .iter()
                .map(|p| p.key.as_str())
                .collect::<Vec<_>>(),
            vec!["display:off", "display:smooth", "display:preedit"]
        );
        // The check mark follows the active mode even when it is not listed.
        let config = Config {
            mode: crate::config::ModeConfig::Smooth,
            menu_modes: vec!["preedit".into(), "off".into()],
            ..Config::default()
        };
        let props = properties(&config);
        assert!(
            props[5].sub.iter().all(|p| p.state == STATE_UNCHECKED),
            "the active mode absent from the list marks nothing"
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
