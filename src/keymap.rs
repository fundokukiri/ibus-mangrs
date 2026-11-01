//! X11 keysym -> `char` conversion.
//!
//! IBus hands the engine `(keyval, keycode, state)` where `keyval` is an X11
//! keysym. `bamboo-core` wants the character the user typed, so the keysym
//! must be decoded rather than cast: `keyval as char` is only correct for
//! Latin-1 keysyms (0x20..0x7e and 0xa0..0xff) and silently produces garbage
//! for the rest.

// Modifier bits, from ibustypes.h.
pub const SHIFT_MASK: u32 = 1 << 0;
pub const LOCK_MASK: u32 = 1 << 1;
pub const CONTROL_MASK: u32 = 1 << 2;
pub const MOD1_MASK: u32 = 1 << 3;
pub const MOD4_MASK: u32 = 1 << 6;
pub const MOD5_MASK: u32 = 1 << 7;
pub const RELEASE_MASK: u32 = 1 << 30;

/// Every modifier that means "this is a shortcut, not text".
pub const SHORTCUT_MASK: u32 = CONTROL_MASK | MOD1_MASK | MOD4_MASK;

// X11 keysyms we care about. Values are from X11/keysymdef.h.
const XK_BACKSPACE: u32 = 0xff08;
const XK_TAB: u32 = 0xff09;
const XK_RETURN: u32 = 0xff0d;
const XK_ESCAPE: u32 = 0xff1b;
const XK_DELETE: u32 = 0xffff;
const XK_HOME: u32 = 0xff50;
const XK_LEFT: u32 = 0xff51;
const XK_UP: u32 = 0xff52;
const XK_RIGHT: u32 = 0xff53;
const XK_DOWN: u32 = 0xff54;
const XK_PAGE_UP: u32 = 0xff55;
const XK_PAGE_DOWN: u32 = 0xff56;
const XK_END: u32 = 0xff57;

/// Convert an X11 keysym to the character it produces.
///
/// Returns `None` for keys that do not produce text (Backspace, arrows,
/// function keys). Callers handle those separately.
pub fn keysym_to_char(keyval: u32) -> Option<char> {
    match keyval {
        // Latin-1 keysyms map 1:1 onto their code point.
        0x20..=0x7e | 0xa0..=0xff => char::from_u32(keyval),
        // Keypad digits and decimal point (NumLock on): VNI marks must work
        // from the numpad too.
        0xffb0..=0xffb9 => char::from_u32(b'0' as u32 + (keyval - 0xffb0)),
        0xffae => Some('.'),
        // Vietnamese letters reach the engine through dead keys on some
        // layouts; the Unicode keysym range is the general fallback.
        0x0100_0000..=0x0110_ffff => char::from_u32(keyval - 0x0100_0000),
        _ => None,
    }
}

/// Keys that end or move the composition rather than extend it.
pub fn is_editing_key(keyval: u32) -> bool {
    matches!(
        keyval,
        XK_BACKSPACE
            | XK_TAB
            | XK_RETURN
            | XK_ESCAPE
            | XK_DELETE
            | XK_HOME
            | XK_LEFT
            | XK_UP
            | XK_RIGHT
            | XK_DOWN
            | XK_PAGE_UP
            | XK_PAGE_DOWN
            | XK_END
            // Keypad Enter and Delete end the word like their main-block
            // twins; without this a numpad Enter left the preedit pending.
            | 0xff8d // KP_Enter
            | 0xff9f // KP_Delete
    )
}

/// Keys that move the caret. When a preedit commit is pending, these are the
/// keys that can outrun it on relayed contexts (a burst of them moves the
/// caret before the commit lands).
pub fn is_navigation_key(keyval: u32) -> bool {
    matches!(
        keyval,
        XK_HOME | XK_LEFT | XK_UP | XK_RIGHT | XK_DOWN | XK_PAGE_UP | XK_PAGE_DOWN | XK_END
    )
}

pub const BACKSPACE: u32 = XK_BACKSPACE;
pub const RETURN: u32 = XK_RETURN;
pub const ESCAPE: u32 = XK_ESCAPE;
pub const TAB: u32 = XK_TAB;
/// Keysyms the uinput modes inject and must recognise as their own echoes.
pub const LEFT: u32 = XK_LEFT;
pub const DELETE: u32 = XK_DELETE;
pub const V: u32 = 0x0076; // lowercase v (the Ctrl+V paste echo)

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ascii_letters_decode() {
        assert_eq!(keysym_to_char(0x61), Some('a'));
        assert_eq!(keysym_to_char(0x41), Some('A'));
        assert_eq!(keysym_to_char(0x20), Some(' '));
    }

    #[test]
    fn latin1_digits_decode() {
        // 0x39 = '9', 0xb0 = degree sign
        assert_eq!(keysym_to_char(0x39), Some('9'));
        assert_eq!(keysym_to_char(0xb0), Some('°'));
    }

    #[test]
    fn unicode_keysyms_decode() {
        // 0x0100_0111 = U+0111 = đ
        assert_eq!(keysym_to_char(0x0100_0111), Some('đ'));
        // 0x0100_1EA1 = U+1EA1 = ạ
        assert_eq!(keysym_to_char(0x0100_1ea1), Some('ạ'));
    }

    #[test]
    fn keypad_digits_decimal_and_enter_decode() {
        // VNI marks must work from the numpad (NumLock on).
        assert_eq!(keysym_to_char(0xffb0), Some('0'));
        assert_eq!(keysym_to_char(0xffb1), Some('1'));
        assert_eq!(keysym_to_char(0xffb9), Some('9'));
        assert_eq!(keysym_to_char(0xffae), Some('.'));
        // Keypad Enter/Delete end the word like the main-block keys.
        assert!(is_editing_key(0xff8d));
        assert!(is_editing_key(0xff9f));
        // KP_Multiply and friends are not text.
        assert_eq!(keysym_to_char(0xffaa), None);
    }

    #[test]
    fn control_keys_do_not_decode() {
        assert_eq!(keysym_to_char(XK_BACKSPACE), None);
        assert_eq!(keysym_to_char(XK_RETURN), None);
        assert_eq!(keysym_to_char(XK_LEFT), None);
    }

    #[test]
    fn editing_keys_are_flagged() {
        assert!(is_editing_key(XK_BACKSPACE));
        assert!(is_editing_key(XK_RETURN));
        assert!(!is_editing_key(0x61));
    }
}
