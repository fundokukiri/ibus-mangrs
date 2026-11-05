//! Hotkey capture: click a button, press a combination — the recorder
//! writes the combo string that `server::parse_hotkey` understands.

use super::*;

/// Canonical combo string: modifiers first (Shift/Control/Alt/Super), then
/// the key name when one is given — joined, so a modifier-only combo has
/// no trailing separator ("Shift+Control", not "Shift+Control+" which the
/// parser rejects). Matches `server::parse_hotkey`.
pub(super) fn hotkey_spec(full: gtk4::gdk::ModifierType, key_name: Option<String>) -> String {
    let mut mods: Vec<&str> = Vec::new();
    for (mask, name) in [
        (gtk4::gdk::ModifierType::SHIFT_MASK, "Shift"),
        (gtk4::gdk::ModifierType::CONTROL_MASK, "Control"),
        (gtk4::gdk::ModifierType::ALT_MASK, "Alt"),
        (gtk4::gdk::ModifierType::SUPER_MASK, "Super"),
        (gtk4::gdk::ModifierType::HYPER_MASK, "Super"),
        (gtk4::gdk::ModifierType::META_MASK, "Super"),
    ] {
        if full.contains(mask) && !mods.contains(&name) {
            mods.push(name);
        }
    }
    let mut spec = mods.join("+");
    if let Some(k) = key_name {
        if !spec.is_empty() {
            spec.push('+');
        }
        spec.push_str(&k);
    }
    spec
}

/// The modifier a pressed/released key belongs to, in gdk mask terms.
pub(super) fn gdk_modifier_of_key(key: gtk4::gdk::Key) -> Option<gtk4::gdk::ModifierType> {
    Some(match key {
        gtk4::gdk::Key::Shift_L | gtk4::gdk::Key::Shift_R => gtk4::gdk::ModifierType::SHIFT_MASK,
        gtk4::gdk::Key::Control_L | gtk4::gdk::Key::Control_R => {
            gtk4::gdk::ModifierType::CONTROL_MASK
        }
        gtk4::gdk::Key::Alt_L | gtk4::gdk::Key::Alt_R => gtk4::gdk::ModifierType::ALT_MASK,
        gtk4::gdk::Key::Super_L | gtk4::gdk::Key::Super_R => gtk4::gdk::ModifierType::SUPER_MASK,
        gtk4::gdk::Key::Hyper_L | gtk4::gdk::Key::Hyper_R => gtk4::gdk::ModifierType::HYPER_MASK,
        gtk4::gdk::Key::Meta_L | gtk4::gdk::Key::Meta_R => gtk4::gdk::ModifierType::META_MASK,
        _ => return None,
    })
}

/// Show a hotkey value as the row's dim text; empty means unassigned.
pub(super) fn render_hotkey(label: &gtk4::Label, value: &str) {
    let unassigned = t("misc.unassigned", "Chưa gán");
    label.set_text(if value.is_empty() { &unassigned } else { value });
}

/// Wire a hotkey row GNOME-style: the edit (pencil) button hosts the
/// capture — a click starts it and, since a button keeps key focus after
/// clicking (focus-on-click), the key controllers on the button receive
/// the pressed keys. The combo is written into `value` and saved. Escape
/// cancels, plain Backspace clears, unusable combinations keep the
/// recorder open, and leaving the button mid-capture cancels like Escape.
/// The combo string matches `server::parse_hotkey` (the same parser
/// re-reads it from config).
pub(super) fn hotkey_wire(
    edit: &gtk4::Button,
    label: &gtk4::Label,
    value: &Rc<RefCell<String>>,
    save: impl Fn() + Clone + 'static,
) {
    let capturing = Rc::new(Cell::new(false));
    // Modifiers held during the capture, as a gdk mask. The combo is
    // completed by a regular key press (any number of modifiers) or by
    // releasing one of two-or-more held modifiers (modifier-only combos) —
    // both stay recordable.
    let pending = Rc::new(Cell::new(0u32));

    {
        let capturing = capturing.clone();
        let pending = pending.clone();
        let label_c = label.clone();
        edit.connect_clicked(move |_| {
            if capturing.get() {
                return;
            }
            capturing.set(true);
            pending.set(0);
            label_c.set_text(&t("misc.capture_prompt", "Nhấn tổ hợp phím… (Esc hủy)"));
            // The button must stay sensitive while focused: disabling the
            // focus widget makes GTK move key focus off it, and the
            // focus-leave handler below would cancel the capture
            // immediately — the recorder would swallow nothing.
        });
    }

    let controller = gtk4::EventControllerKey::new();
    {
        let label_c = label.clone();
        let capturing = capturing.clone();
        let pending = pending.clone();
        let value = value.clone();
        let save = save.clone();
        controller.connect_key_pressed(move |_, key, _keycode, state| {
            use gtk4::glib::Propagation;

            if !capturing.get() {
                return Propagation::Proceed;
            }
            // A modifier already held before the button was clicked is in
            // the event state, so it joins the pending mask here.
            let held = state
                & (gtk4::gdk::ModifierType::SHIFT_MASK
                    | gtk4::gdk::ModifierType::CONTROL_MASK
                    | gtk4::gdk::ModifierType::ALT_MASK
                    | gtk4::gdk::ModifierType::SUPER_MASK
                    | gtk4::gdk::ModifierType::HYPER_MASK
                    | gtk4::gdk::ModifierType::META_MASK);
            if let Some(mask) = gdk_modifier_of_key(key) {
                pending.set(pending.get() | mask.bits() | held.bits());
                return Propagation::Stop;
            }
            // Escape cancels the capture; the stored value stays as it was.
            if key == gtk4::gdk::Key::Escape {
                capturing.set(false);
                pending.set(0);
                capturing_end(&label_c, &value.borrow());
                return Propagation::Stop;
            }
            // Plain Backspace clears the assignment.
            if key == gtk4::gdk::Key::BackSpace && pending.get() == 0 {
                capturing.set(false);
                *value.borrow_mut() = String::new();
                capturing_end(&label_c, "");
                save();
                return Propagation::Stop;
            }
            let spec = hotkey_spec(
                gtk4::gdk::ModifierType::from_bits_truncate(pending.get() | held.bits()),
                // Punctuation keys are the ones people actually want
                // (Shift+~, Ctrl+~): their GDK names are words like
                // "asciitilde" which the parser cannot read, so a key that
                // maps to a printable ASCII character is recorded as that
                // character instead.
                key.to_unicode()
                    .filter(|c| c.is_ascii_graphic())
                    .map(|c| c.to_string())
                    .or_else(|| key.name().map(|n| n.to_string())),
            );
            // Rejected combos keep the recorder open: the engine's parser
            // would silently drop them, so try another key instead.
            if crate::server::parse_hotkey(&spec).is_none() {
                label_c.set_text(&t(
                    "misc.capture_bad_key",
                    "Phím này không dùng được — thử phím khác",
                ));
                return Propagation::Stop;
            }
            capturing.set(false);
            pending.set(0);
            *value.borrow_mut() = spec.clone();
            capturing_end(&label_c, &spec);
            save();
            Propagation::Stop
        });
    }
    edit.add_controller(controller);

    // A modifier-only combo completes on release: hold Control, tap Shift,
    // done. Releasing the only held modifier keeps the recorder open.
    let released = gtk4::EventControllerKey::new();
    {
        let label_c = label.clone();
        let capturing = capturing.clone();
        let pending = pending.clone();
        let value = value.clone();
        let save = save.clone();
        released.connect_key_released(move |_, key, _keycode, _state| {
            if !capturing.get() {
                return;
            }
            let Some(mask) = gdk_modifier_of_key(key) else {
                return;
            };
            let before = pending.get();
            pending.set(before & !mask.bits());
            if before.count_ones() >= 2 {
                let spec = hotkey_spec(gtk4::gdk::ModifierType::from_bits_truncate(before), None);
                if crate::server::parse_hotkey(&spec).is_some() {
                    capturing.set(false);
                    pending.set(0);
                    *value.borrow_mut() = spec.clone();
                    capturing_end(&label_c, &spec);
                    save();
                }
            }
        });
    }
    edit.add_controller(released);

    // Leaving the button mid-capture cancels, like Escape.
    let focus = gtk4::EventControllerFocus::new();
    {
        let label_c = label.clone();
        let capturing = capturing.clone();
        let pending = pending.clone();
        let value = value.clone();
        focus.connect_leave(move |_| {
            if capturing.get() {
                capturing.set(false);
                pending.set(0);
                capturing_end(&label_c, &value.borrow());
            }
        });
    }
    edit.add_controller(focus);
}

/// End a capture: show `text` in the row.
fn capturing_end(label: &gtk4::Label, text: &str) {
    render_hotkey(label, text);
}
