//! Hotkey capture: click a button, press a combination — the recorder
//! writes the combo string that `server::parse_hotkey` understands.

use super::*;

/// Canonical combo string: modifiers first (Shift/Control/Alt/Super), then
/// the key name when one is given. Matches `server::parse_hotkey`.
pub(super) fn hotkey_spec(full: gtk4::gdk::ModifierType, key_name: Option<String>) -> String {
    let mut spec = String::new();
    for (mask, name) in [
        (gtk4::gdk::ModifierType::SHIFT_MASK, "Shift"),
        (gtk4::gdk::ModifierType::CONTROL_MASK, "Control"),
        (gtk4::gdk::ModifierType::ALT_MASK, "Alt"),
        (gtk4::gdk::ModifierType::SUPER_MASK, "Super"),
        (gtk4::gdk::ModifierType::HYPER_MASK, "Super"),
        (gtk4::gdk::ModifierType::META_MASK, "Super"),
    ] {
        if full.contains(mask) && !spec.contains(&format!("{name}+")) {
            spec.push_str(name);
            spec.push('+');
        }
    }
    if let Some(k) = key_name {
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

/// Show a hotkey value on its capture button; empty means unassigned.
pub(super) fn render_hotkey(btn: &gtk4::Button, value: &str) {
    btn.set_label(if value.is_empty() {
        "Chưa gán"
    } else {
        value
    });
}

/// Turn a hotkey button into a recorder: click it, press a combination —
/// the combo is written into `value` and saved. Escape cancels, plain
/// Backspace clears, unusable combinations keep the recorder open. The
/// combo string matches `server::parse_hotkey` (the same parser re-reads
/// it from config).
pub(super) fn hotkey_wire(
    btn: &gtk4::Button,
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
        let btn_c = btn.clone();
        btn.connect_clicked(move |_| {
            if capturing.get() {
                return;
            }
            capturing.set(true);
            pending.set(0);
            btn_c.set_label("Nhấn tổ hợp phím… (Esc hủy)");
        });
    }

    let controller = gtk4::EventControllerKey::new();
    {
        let btn_c = btn.clone();
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
                render_hotkey(&btn_c, &value.borrow());
                return Propagation::Stop;
            }
            // Plain Backspace clears the assignment.
            if key == gtk4::gdk::Key::BackSpace && pending.get() == 0 {
                capturing.set(false);
                *value.borrow_mut() = String::new();
                render_hotkey(&btn_c, "");
                save();
                return Propagation::Stop;
            }
            let spec = hotkey_spec(
                gtk4::gdk::ModifierType::from_bits_truncate(pending.get() | held.bits()),
                Some(key.name().unwrap_or_default().to_string()),
            );
            // Rejected combos keep the recorder open: the engine's parser
            // would silently drop them, so try another key instead.
            if crate::server::parse_hotkey(&spec).is_none() {
                btn_c.set_label("Phím này không dùng được — thử phím khác");
                return Propagation::Stop;
            }
            capturing.set(false);
            pending.set(0);
            *value.borrow_mut() = spec.clone();
            btn_c.set_label(&spec);
            save();
            Propagation::Stop
        });
    }
    btn.add_controller(controller);

    // A modifier-only combo completes on release: hold Control, tap Shift,
    // done. Releasing the only held modifier keeps the recorder open.
    let released = gtk4::EventControllerKey::new();
    {
        let btn_c = btn.clone();
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
                    btn_c.set_label(&spec);
                    save();
                }
            }
        });
    }
    btn.add_controller(released);

    // Leaving the button mid-capture cancels, like Escape.
    let focus = gtk4::EventControllerFocus::new();
    {
        let btn_c = btn.clone();
        let capturing = capturing.clone();
        let pending = pending.clone();
        let value = value.clone();
        focus.connect_leave(move |_| {
            if capturing.get() {
                capturing.set(false);
                pending.set(0);
                render_hotkey(&btn_c, &value.borrow());
            }
        });
    }
    btn.add_controller(focus);
}
