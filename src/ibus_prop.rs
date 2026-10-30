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
pub const KEY_SPELL_CHECK: &str = "spell_check";
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
        KEY_SPELL_CHECK => ("Kiểm tra chính tả", true),
        KEY_MACROS => ("Gõ tắt", true),
        KEY_MACRO_OFF => ("Gõ tắt khi tắt tiếng Việt", false),
        KEY_AUTO_CAPITALIZE => ("Tự động viết hoa sau . ! ?", false),
        KEY_DICTIONARY => ("Dùng từ điển của tôi", false),
        KEY_FREE_TONE => ("Bỏ dấu tự do (hoangf → hoàng)", true),
        KEY_MODERN_TONE => ("Dấu thanh chuẩn (hòa thay vì hoà)", true),
        KEY_HIDE_UNDERLINE => ("Ẩn gạch chân khi gõ", false),
        KEY_AUTO_RESTORE => ("Tự khôi phục từ tiếng Anh", true),
        KEY_EM_DASH => ("Gõ -- thành —", false),
        KEY_DOUBLE_SPACE => ("Hai dấu cách thành dấu chấm", false),
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

/// (menu key, label) for every display mode.
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
        let state = if checked { STATE_CHECKED } else { STATE_UNCHECKED };
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
            true,  // sensitive
            true,  // visible
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
            Prop::radio(&format!("{KEY_METHOD}:{key}"), label, config.method.key() == *key)
        })
        .collect();
    let display_items = MODES
        .iter()
        .map(|(key, label)| {
            Prop::radio(&format!("{KEY_DISPLAY}:{key}"), label, config.mode.key() == *key)
        })
        .collect();

    let spell_group = vec![
        Prop::toggle(KEY_SPELL_CHECK, "Kiểm tra chính tả", config.auto_correct),
        Prop::toggle(KEY_DICTIONARY, "Dùng từ điển của tôi", config.dictionary_enabled),
    ];
    // GNOME sizes the popup to the widest label, so the quick settings stay
    // short; the details live in the tooltips.
    let mut quick = vec![
        Prop::toggle(KEY_MACROS, "Gõ tắt", config.macro_enabled),
        Prop::toggle(KEY_AUTO_CAPITALIZE, "Tự viết hoa", config.auto_capitalize),
        Prop::toggle(KEY_MACRO_OFF, "Gõ tắt khi OFF", config.macro_in_off_mode),
        Prop::normal(KEY_MACRO_EDITOR, "Mở bảng gõ tắt…"),
        Prop::separator(),
        Prop::toggle(KEY_FREE_TONE, "Bỏ dấu tự do", config.free_tone_marking),
        Prop::toggle(KEY_MODERN_TONE, "Dấu thanh chuẩn", config.modern_tone_style),
        Prop::toggle(KEY_HIDE_UNDERLINE, "Ẩn gạch chân khi gõ", config.hide_underline),
        Prop::toggle(KEY_AUTO_RESTORE, "Khôi phục T.Anh", config.auto_restore),
        Prop::toggle(KEY_EM_DASH, "Gõ -- thành —", config.em_dash),
        Prop::toggle(KEY_DOUBLE_SPACE, "Hai cách → chấm", config.double_space_period),
    ];
    let quick_hints = [
        (KEY_AUTO_CAPITALIZE, "Tự động viết hoa sau . ! ? và Enter"),
        (KEY_MACRO_OFF, "Vẫn gõ tắt khi tiếng Việt đang tắt"),
        (KEY_FREE_TONE, "Dấu đặt anywhere trong từ: hoangf → hoàng"),
        (KEY_MODERN_TONE, "hòa (chuẩn mới) thay vì hoà (chuẩn cũ)"),
        (KEY_HIDE_UNDERLINE, "Gửi preedit không kèm thuộc tính gạch chân"),
        (KEY_AUTO_RESTORE, "Từ không phải âm tiết Việt trả về chuỗi phím gốc"),
        (KEY_EM_DASH, "Hai dấu trừ liên tiếp thành dấu gạch dài"),
        (KEY_DOUBLE_SPACE, "Thử nghiệm: hai dấu cách liên tiếp thành . "),
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
        Prop::menu(KEY_DISPLAY, "Hiển thị", display_items),
        Prop::menu(KEY_SPELL_CHECK, "Chính tả & từ điển", spell_group),
        Prop::menu("quick", "Cấu hình nhanh", quick),
        Prop::normal(KEY_SHORTCUTS, "Phím tắt…"),
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
        assert_eq!(props[0].label, format!("MăngRS {}", env!("CARGO_PKG_VERSION")));
        assert_eq!(props[1].kind, TYPE_SEPARATOR);
        assert_eq!(props[2].key, KEY_ENABLED);
        assert_eq!(props[2].kind, TYPE_TOGGLE);
        assert_eq!(props[3].key, KEY_CHARSET);
        assert_eq!(props[3].sub.len(), CHARSETS.len());
        assert_eq!(props[4].key, KEY_METHOD);
        assert_eq!(props[4].sub.len(), METHODS.len());
        assert_eq!(props[5].key, KEY_DISPLAY);
        assert_eq!(props[5].sub.len(), MODES.len());
        assert_eq!(props[6].key, KEY_SPELL_CHECK);
        assert_eq!(props[6].kind, TYPE_MENU);
        assert_eq!(props[7].key, "quick");
        assert_eq!(props[7].sub.len(), 11);
        assert_eq!(props[8].key, KEY_SHORTCUTS);
        assert_eq!(props[8].kind, TYPE_NORMAL);
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
        // Spell group: spell check off, dictionary off.
        assert_eq!(checked(&props[6].sub), Vec::<&str>::new());
        // Quick group: free tone + modern tone + auto-restore on (defaults),
        // the rest off.
        assert_eq!(
            checked(&props[7].sub),
            vec!["macro", "free_tone", "modern_tone", "auto_restore"]
        );
    }

    #[test]
    fn serialized_types_match_ibus() {
        let props = properties(&Config::default());
        assert_eq!(props[0].variant().0.signature().to_string(), "(sa{sv}suvsvbbuvv)");
        assert_eq!(prop_list_variant(&props).0.signature().to_string(), "(sa{sv}av)");
    }
}
