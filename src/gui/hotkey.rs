//! Hotkey capture and display: the combo recorder writes the string that
//! `server::parse_hotkey` understands, the row suffix renders it as keycaps,
//! and the capture dialog replaces the old pencil-button recorder.

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

/// The stored spec ("Control+Alt+v") as a GTK accelerator string for
/// `ShortcutLabel`. `None` when there is nothing to render as keycaps —
/// a modifier-only combo or an unusable key — the raw text shows instead.
pub(super) fn accelerator_label(spec: &str) -> Option<String> {
    let (mods, key) = crate::server::parse_hotkey(spec)?;
    if key == 0 {
        return None;
    }
    let mut out = String::new();
    for (mask, tag) in [
        (crate::keymap::SHIFT_MASK, "Shift"),
        (crate::keymap::CONTROL_MASK, "Control"),
        (crate::keymap::MOD1_MASK, "Alt"),
        (crate::keymap::MOD4_MASK, "Super"),
    ] {
        if mods & mask != 0 {
            out.push_str(&format!("<{tag}>"));
        }
    }
    // The parser accepts exactly three key shapes: one printable ASCII
    // character, `space`, and `f1`..`f12`.
    let name = match char::from_u32(key).filter(|c| c.is_ascii_graphic()) {
        Some(c) => c.to_string(),
        None => match key {
            0x20 => "space".to_string(),
            k @ 0xffbe..=0xffc9 => format!("F{}", k - 0xffbe + 1),
            _ => return None,
        },
    };
    out.push_str(&name);
    // GtkShortcutLabel would warn on (and drop) an unparseable keycap, e.g.
    // punctuation whose keyval name is not the character itself ("~" is
    // "asciitilde"): the dim raw text shows instead.
    gtk4::accelerator_parse(out.as_str())?;
    Some(out)
}

/// Do two specs name the same physical combo? Compared on the engine's own
/// parse, so "Alt+Control+v" equals "Control+Alt+v"; unparseable specs fall
/// back to the literal string.
pub(super) fn same_combo(a: &str, b: &str) -> bool {
    match (
        crate::server::parse_hotkey(a),
        crate::server::parse_hotkey(b),
    ) {
        (Some(x), Some(y)) => x == y,
        _ => a == b,
    }
}

/// One keycap-or-dim-label pair: `ShortcutLabel` when the value renders as
/// an accelerator, the dim fallback otherwise (also for unassigned). The
/// pair carries the row's accessible description ("<title>, <keys>").
/// GObject clones share the widgets.
#[derive(Clone)]
pub(super) struct ShortcutDisplay {
    stack: gtk4::Stack,
    keys: gtk4::ShortcutLabel,
    raw: gtk4::Label,
    title: String,
}

impl ShortcutDisplay {
    pub(super) fn new(title: &str) -> Self {
        let keys = gtk4::ShortcutLabel::new("");
        keys.set_valign(gtk4::Align::Center);
        let raw = gtk4::Label::new(Some(&t("misc.unassigned", "Chưa gán")));
        raw.add_css_class("dim-label");
        raw.set_valign(gtk4::Align::Center);
        let stack = gtk4::Stack::new();
        // No transitions: the two states swap, they do not animate.
        stack.set_transition_type(gtk4::StackTransitionType::None);
        stack.add_named(&raw, Some("raw"));
        stack.add_named(&keys, Some("keys"));
        let this = Self {
            stack,
            keys,
            raw,
            title: title.to_string(),
        };
        this.render("");
        this
    }

    pub(super) fn widget(&self) -> &gtk4::Stack {
        &self.stack
    }

    /// Render a stored spec; empty means unassigned.
    pub(super) fn render(&self, value: &str) {
        let description = if value.is_empty() {
            self.raw.set_text(&t("misc.unassigned", "Chưa gán"));
            self.stack.set_visible_child(&self.raw);
            t("misc.a11y_unassigned", "chưa gán").to_string()
        } else {
            match accelerator_label(value) {
                Some(acc) => {
                    self.keys.set_accelerator(&acc);
                    self.stack.set_visible_child(&self.keys);
                }
                None => {
                    self.raw.set_text(value);
                    self.stack.set_visible_child(&self.raw);
                }
            }
            value.to_string()
        };
        self.stack
            .update_property(&[gtk4::accessible::Property::Description(&format!(
                "{}, {}",
                self.title, description
            ))]);
    }
}

/// The dialog's live capture preview: keycaps while the combo is held, one
/// dim "waiting" label before anything is pressed. The widgets clone
/// separately, so key handlers can update the preview after the dialog
/// moved into place.
#[derive(Clone)]
pub(super) struct CapturePreview {
    stack: gtk4::Stack,
    keys: gtk4::ShortcutLabel,
    raw: gtk4::Label,
}

impl CapturePreview {
    fn new() -> Self {
        let keys = gtk4::ShortcutLabel::new("");
        let raw = gtk4::Label::new(Some(&t("misc.capture_waiting", "Đang chờ phím…")));
        raw.add_css_class("dim-label");
        let stack = gtk4::Stack::new();
        stack.set_transition_type(gtk4::StackTransitionType::None);
        stack.set_focusable(true);
        stack.add_named(&raw, Some("raw"));
        stack.add_named(&keys, Some("keys"));
        let this = Self { stack, keys, raw };
        this.show("");
        this
    }

    /// Grab key focus back from the function picker (the add dialog).
    fn grab(&self) {
        self.stack.grab_focus();
    }

    /// Show a held or committed combo; empty shows the waiting state.
    fn show(&self, spec: &str) {
        if spec.is_empty() {
            self.raw
                .set_text(&t("misc.capture_waiting", "Đang chờ phím…"));
            self.stack.set_visible_child(&self.raw);
            return;
        }
        match accelerator_label(spec) {
            Some(acc) => {
                self.keys.set_accelerator(&acc);
                self.stack.set_visible_child(&self.keys);
            }
            None => {
                self.raw.set_text(spec);
                self.stack.set_visible_child(&self.raw);
            }
        }
    }
}

/// Build the shortcut-capture dialog (not presented, for tests): the
/// heading and body as given, one live keycap preview, an inline note for
/// rejections, and `extra` (the add dialog's function picker) above it.
/// Responses: only "Hủy" — Escape and the button both take it.
pub(super) fn build_capture_dialog(
    heading: &str,
    body: &str,
    extra: Option<&gtk4::Widget>,
) -> (adw::AlertDialog, CapturePreview, gtk4::Label) {
    let preview = CapturePreview::new();
    let note = gtk4::Label::new(None);
    note.add_css_class("error");
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.set_visible(false);
    let content = gtk4::Box::new(Orientation::Vertical, 12);
    if let Some(extra) = extra {
        content.append(extra);
    }
    content.append(&preview.stack);
    content.append(&note);
    let dialog = adw::AlertDialog::builder()
        .heading(heading)
        .body(body)
        .build();
    dialog.add_response("cancel", &t("misc.cancel", "Hủy"));
    dialog.set_close_response("cancel");
    dialog.set_extra_child(Some(&content));
    (dialog, preview, note)
}

/// Open the shortcut-capture dialog: heading, body, a live keycap preview
/// and the press/release logic the old pencil recorder had, now on the
/// dialog itself. Escape (the dialog's close response) cancels without
/// saving; plain Backspace clears (the result comes back as `None`) and
/// closes; a non-modifier key together with the held modifiers commits —
/// but only after `conflicts` accepts it, otherwise the inline note names
/// the function that already holds the combo and the dialog stays open.
/// Modifier-only combos still complete on releasing one of two-or-more
/// held modifiers, and plain Tab (with or without Shift) is left to focus
/// navigation.
pub(super) fn open_capture_dialog(
    parent: &impl IsA<gtk4::Widget>,
    heading: &str,
    body: &str,
    extra: Option<&gtk4::Widget>,
    refocus: Option<&adw::ComboRow>,
    conflicts: impl Fn(&str) -> Option<String> + 'static,
    on_result: impl Fn(Option<String>) + 'static,
) {
    let (dialog, preview, note) = build_capture_dialog(heading, body, extra);
    let pending = Rc::new(Cell::new(0u32));
    // Both key handlers commit and reject: shared, so cloned per handler.
    let conflicts = Rc::new(conflicts);
    let on_result = Rc::new(on_result);

    // Changing the function must not eat the next key press: give the
    // capture area focus back so the dialog's controllers see the keys.
    if let Some(refocus) = refocus {
        let preview = preview.clone();
        refocus.connect_selected_notify(move |_| preview.grab());
    }

    let pressed = gtk4::EventControllerKey::new();
    {
        let dialog = dialog.clone();
        let preview = preview.clone();
        let note = note.clone();
        let pending = pending.clone();
        let conflicts = conflicts.clone();
        let on_result = on_result.clone();
        pressed.connect_key_pressed(move |_, key, _keycode, state| {
            use gtk4::glib::Propagation;

            // A modifier held before the dialog opened is in the event
            // state, so it joins the pending mask here.
            let held = state
                & (gtk4::gdk::ModifierType::SHIFT_MASK
                    | gtk4::gdk::ModifierType::CONTROL_MASK
                    | gtk4::gdk::ModifierType::ALT_MASK
                    | gtk4::gdk::ModifierType::SUPER_MASK
                    | gtk4::gdk::ModifierType::HYPER_MASK
                    | gtk4::gdk::ModifierType::META_MASK);
            if let Some(mask) = gdk_modifier_of_key(key) {
                pending.set(pending.get() | mask.bits() | held.bits());
                // Live preview while the modifiers are held; a rejection
                // note from an earlier attempt stops being true.
                note.set_visible(false);
                preview.show(&hotkey_spec(
                    gtk4::gdk::ModifierType::from_bits_truncate(pending.get()),
                    None,
                ));
                return Propagation::Stop;
            }
            // Plain Backspace clears the assignment.
            if key == gtk4::gdk::Key::BackSpace && pending.get() == 0 {
                on_result(None);
                dialog.close();
                return Propagation::Stop;
            }
            // Focus navigation keeps plain Tab (and Shift+Tab); the combo
            // Tab needs one of the real modifiers.
            if key == gtk4::gdk::Key::Tab
                && state
                    & (gtk4::gdk::ModifierType::CONTROL_MASK
                        | gtk4::gdk::ModifierType::ALT_MASK
                        | gtk4::gdk::ModifierType::SUPER_MASK)
                    == gtk4::gdk::ModifierType::empty()
            {
                return Propagation::Proceed;
            }
            // Escape reaches the dialog's close response ("Hủy") — the
            // capture stays out of its way.
            if key == gtk4::gdk::Key::Escape {
                return Propagation::Proceed;
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
            // Rejected combos keep the dialog open: the engine's parser
            // would silently drop them, so try another key instead.
            if crate::server::parse_hotkey(&spec).is_none() {
                note.set_text(&t(
                    "misc.capture_bad_key",
                    "Phím này không dùng được — thử phím khác",
                ));
                note.set_visible(true);
                return Propagation::Stop;
            }
            match conflicts(&spec) {
                Some(function) => {
                    note.set_text(
                        &t("misc.conflict_used", "Đã dùng cho: {f}").replace("{f}", &function),
                    );
                    note.set_visible(true);
                    Propagation::Stop
                }
                None => {
                    on_result(Some(spec));
                    dialog.close();
                    Propagation::Stop
                }
            }
        });
    }
    dialog.add_controller(pressed);

    // A modifier-only combo completes on release: hold Control, tap Shift,
    // done. Releasing the only held modifier keeps the dialog open.
    let released = gtk4::EventControllerKey::new();
    {
        let dialog = dialog.clone();
        let note = note.clone();
        let pending = pending.clone();
        released.connect_key_released(move |_, key, _keycode, _state| {
            let Some(mask) = gdk_modifier_of_key(key) else {
                return;
            };
            let before = pending.get();
            pending.set(before & !mask.bits());
            if before.count_ones() >= 2 {
                let spec = hotkey_spec(gtk4::gdk::ModifierType::from_bits_truncate(before), None);
                if crate::server::parse_hotkey(&spec).is_some() {
                    match conflicts(&spec) {
                        Some(function) => {
                            note.set_text(
                                &t("misc.conflict_used", "Đã dùng cho: {f}")
                                    .replace("{f}", &function),
                            );
                            note.set_visible(true);
                        }
                        None => {
                            on_result(Some(spec));
                            dialog.close();
                        }
                    }
                }
            }
        });
    }
    dialog.add_controller(released);

    dialog.present(Some(parent));
}
