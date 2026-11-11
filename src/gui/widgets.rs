//! Small widget builders shared by the pages, plus save/import feedback
//! (alert dialog, focused-app chip).

use super::*;

// --- small builders ---

/// Surface a save/import failure in the window: an `eprintln!` is invisible
/// from a GUI, and a silent failure looks exactly like a broken feature.
pub(super) fn alert(window: &impl IsA<gtk4::Widget>, text: &str) {
    adw::AlertDialog::builder()
        .heading(t("misc.save_alert_heading", "Không lưu được cấu hình"))
        .body(text)
        .build()
        .present(Some(window));
}

/// Ask the focus extension for the focused window — the same session-bus
/// call the engine makes on unnamed focus-ins (`(ss)` wm_class, title).
pub(super) fn focused_app_name() -> Result<(String, String), String> {
    let conn =
        gtk4::gio::bus_get_sync(gtk4::gio::BusType::Session, None::<&gtk4::gio::Cancellable>)
            .map_err(|e| {
                t("misc.dbus_error", "Không nối được D-Bus: {e}").replace("{e}", &e.to_string())
            })?;
    let reply = conn
        .call_sync(
            Some(crate::server::FOCUS_EXT_NAME),
            crate::server::FOCUS_EXT_PATH,
            crate::server::FOCUS_EXT_IFACE,
            "GetFocusedWindow",
            None::<&gtk4::glib::Variant>,
            None,
            gtk4::gio::DBusCallFlags::NONE,
            300,
            None::<&gtk4::gio::Cancellable>,
        )
        .map_err(|_| {
            t(
                "misc.focus_ext_error",
                "Không hỏi được extension MăngRS Focus (data/gnome-shell-extension)",
            )
        })?;
    let (class, title) = reply
        .get::<(String, String)>()
        .ok_or("Extension trả dữ liệu không đúng dạng")?;
    if class.is_empty() {
        return Err(t("misc.focus_none", "Không có cửa sổ nào đang focus"));
    }
    Ok((class, title))
}

/// A boxed-list boolean row (title + dim subtitle). Replaces the plain
/// Switch + pref_row pair of the old pages. Every row builder switches
/// markup off: Adw rows parse Pango markup in title/subtitle by default,
/// and a bare `&` in a text (the URL rule's subtitle) blanked it.
pub(super) fn switch_row(title: &str, subtitle: &str) -> adw::SwitchRow {
    // Markup is turned off BEFORE the text: the builder applies its stored
    // properties in declaration order (text first), and a bare `&` in a
    // subtitle parsed as Pango would warn on every startup.
    let row = adw::SwitchRow::new();
    row.set_use_markup(false);
    row.set_title(title);
    row.set_subtitle(subtitle);
    row
}

/// The experimental row carries a small "Thử nghiệm" badge UNDER its
/// subtitle — the description's own line, out of the control area — and the
/// switch (its own or the merged row's) gets a spoken name mentioning the
/// trial status. The subtitle label's parent is the row's internal vertical
/// title box, reached through the same private-child walk as `row_subtitle`;
/// a row without a subtitle falls back to the internal suffixes box. Works
/// for every row type: they all render a title box.
pub(super) fn decorate_experimental<W>(row: &W)
where
    W: IsA<gtk4::Widget> + IsA<adw::PreferencesRow>,
{
    fn find(w: &gtk4::Widget) -> (Option<gtk4::Box>, Option<gtk4::Switch>) {
        let mut box_out = None;
        let mut switch_out = None;
        fn walk(w: &gtk4::Widget, box_out: &mut Option<gtk4::Box>, sw: &mut Option<gtk4::Switch>) {
            if box_out.is_none() {
                if let Some(b) = w.downcast_ref::<gtk4::Box>() {
                    if b.css_classes().iter().any(|c| c.as_str() == "suffixes") {
                        *box_out = Some(b.clone());
                    }
                }
            }
            if sw.is_none() {
                if let Some(s) = w.downcast_ref::<gtk4::Switch>() {
                    *sw = Some(s.clone());
                }
            }
            let mut child = w.first_child();
            while let Some(c) = child {
                walk(&c, box_out, sw);
                child = c.next_sibling();
            }
        }
        walk(w, &mut box_out, &mut switch_out);
        (box_out, switch_out)
    }
    let badge_text = t("misc.experimental_badge", "Thử nghiệm");
    let badge = gtk4::Label::new(Some(&badge_text));
    badge.add_css_class("caption");
    badge.add_css_class("warning");
    badge.set_xalign(0.0);
    let subtitle_box = row_subtitle(row).and_then(|l| l.parent().and_downcast::<gtk4::Box>());
    let (suffixes, switch) = find(row.upcast_ref());
    match subtitle_box {
        Some(b) => b.append(&badge),
        // No subtitle to sit under: keep the badge inside the control area.
        None => {
            if let Some(suffixes) = suffixes {
                suffixes.prepend(&badge);
                badge.set_valign(gtk4::Align::Center);
            }
        }
    }
    let spoken = format!("{}, {}", row.title(), badge_text);
    match switch {
        Some(sw) => sw.update_property(&[gtk4::accessible::Property::Label(&spoken)]),
        None => row
            .upcast_ref::<gtk4::Widget>()
            .update_property(&[gtk4::accessible::Property::Label(&spoken)]),
    }
}

/// A boxed-list choice row; `selected` indexes into `items`.
pub(super) fn combo_row(title: &str, subtitle: &str, items: &[&str]) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_use_markup(false);
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.set_model(Some(&StringList::new(items)));
    row.set_selected(0);
    row
}

/// A boxed-list number row; `step` is both the arrow increment and the
/// page increment (x10).
pub(super) fn spin_row(title: &str, subtitle: &str, min: f64, max: f64, step: f64) -> adw::SpinRow {
    let adjustment = gtk4::Adjustment::new(min, min, max, step, step * 10.0, 0.0);
    let row = adw::SpinRow::new(Some(&adjustment), 0.0, 0);
    row.set_use_markup(false);
    row.set_title(title);
    row.set_subtitle(subtitle);
    row
}

/// An info row for a boxed list (title + dim subtitle, no control). Not
/// activatable — the hover highlight on an activation-shaped row that does
/// nothing reads as a broken button.
pub(super) fn action_row(title: &str, subtitle: &str) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_use_markup(false);
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.set_activatable(false);
    row
}

/// A merged feature row: the feature's title and subtitle on the left, its
/// on/off switch as the first suffix and the value control as the second —
/// one row per feature instead of a switch row plus a separate value row
/// below it. Returns the switch for the save/twin-lock wiring; the row
/// itself goes into the gate's lock list (locking the row greys both
/// suffixes, since sensitivity cascades).
pub(super) fn switch_value_row<W: IsA<gtk4::Widget>>(
    title: &str,
    subtitle: &str,
    value: &W,
) -> (adw::ActionRow, gtk4::Switch) {
    let row = action_row(title, subtitle);
    let switch = gtk4::Switch::new();
    switch.set_valign(gtk4::Align::Center);
    value.set_valign(gtk4::Align::Center);
    // A vertical divider draws the boundary between the two halves — the
    // switch half and the value half — instead of an invisible gap.
    let divider = gtk4::Separator::new(gtk4::Orientation::Vertical);
    divider.set_margin_start(8);
    divider.set_margin_end(8);
    row.add_suffix(&switch);
    row.add_suffix(&divider);
    row.add_suffix(value);
    (row, switch)
}

/// One width for every merged-row value control: the rows must read as a
/// balanced column, not some boxes short and some long.
const VALUE_WIDTH_CHARS: i32 = 12;

/// The value half of a merged row: a bare SpinButton without SpinRow's
/// title — the merged row's title carries the meaning. Same adjustment
/// shape as `spin_row`.
pub(super) fn spin_button(min: f64, max: f64, step: f64) -> gtk4::SpinButton {
    let adjustment = gtk4::Adjustment::new(min, min, max, step, step * 10.0, 0.0);
    let spin = gtk4::SpinButton::new(Some(&adjustment), step, 0);
    spin.set_width_chars(VALUE_WIDTH_CHARS);
    spin
}

/// The text half of a merged row: a plain entry sized to the shared value
/// width (a short character list, `: " ').
pub(super) fn chars_entry() -> gtk4::Entry {
    let entry = gtk4::Entry::new();
    entry.set_width_chars(VALUE_WIDTH_CHARS);
    entry
}

pub(super) fn text_view(content: &str) -> TextView {
    let view = TextView::builder()
        .hexpand(true)
        .monospace(true)
        .left_margin(12)
        .right_margin(12)
        .top_margin(12)
        .bottom_margin(12)
        .build();
    view.buffer().set_text(content);
    view
}

/// The internal subtitle label of an Adw row: it is not a public child, so
/// a caption/monospace look for file paths reaches it through the tree.
/// Falls back to the second label (title, then subtitle order) when the
/// theme class is absent.
pub(super) fn row_subtitle<W: IsA<gtk4::Widget>>(row: &W) -> Option<gtk4::Label> {
    row_labels(row).1
}

/// The internal title label of an Adw row, for the same reason — e.g. an
/// app id that must ellipsize at its middle instead of its end.
pub(super) fn row_title<W: IsA<gtk4::Widget>>(row: &W) -> Option<gtk4::Label> {
    row_labels(row).0
}

/// The first GtkLabel below `w` in reading order. GtkDropDown hides its
/// selected-item label behind a button and a list-item wrapper with no
/// public accessor — this reaches it to ellipsize long mode names.
pub(super) fn first_label<W: IsA<gtk4::Widget>>(w: &W) -> Option<gtk4::Label> {
    fn walk(w: &gtk4::Widget) -> Option<gtk4::Label> {
        if let Some(l) = w.downcast_ref::<gtk4::Label>() {
            return Some(l.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(l) = walk(&c) {
                return Some(l);
            }
            child = c.next_sibling();
        }
        None
    }
    walk(w.upcast_ref())
}

/// The inline value view of an AdwComboRow: a ListView showing the
/// selected item, hidden behind internals with no public accessor — this
/// reaches it so the value label can be aligned inside it.
pub(super) fn combo_value_view<W: IsA<gtk4::Widget>>(row: &W) -> Option<gtk4::ListView> {
    fn walk(w: &gtk4::Widget) -> Option<gtk4::ListView> {
        if let Some(view) = w.downcast_ref::<gtk4::ListView>() {
            return Some(view.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(view) = walk(&c) {
                return Some(view);
            }
            child = c.next_sibling();
        }
        None
    }
    walk(row.upcast_ref())
}

/// All labels inside an Adw row: (title, subtitle) by their theme classes,
/// with positional fallback (first label = title, second = subtitle).
fn row_labels<W: IsA<gtk4::Widget>>(row: &W) -> (Option<gtk4::Label>, Option<gtk4::Label>) {
    fn walk(w: &gtk4::Widget, labels: &mut Vec<gtk4::Label>) {
        if let Some(l) = w.downcast_ref::<gtk4::Label>() {
            labels.push(l.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            walk(&c, labels);
            child = c.next_sibling();
        }
    }
    let mut labels = Vec::new();
    walk(row.upcast_ref(), &mut labels);
    let has = |l: &gtk4::Label, class: &str| l.css_classes().iter().any(|c| c.as_str() == class);
    let title = labels
        .iter()
        .find(|l| has(l, "title"))
        .cloned()
        .or_else(|| labels.first().cloned());
    let subtitle = labels
        .iter()
        .find(|l| has(l, "subtitle"))
        .cloned()
        .or_else(|| labels.get(1).cloned());
    (title, subtitle)
}

/// A bounded monospace card editor: the TextView drops its own `.view`
/// background so the card fill shows through unchanged, an example hint
/// sits top-left while the buffer is empty (decorative: no click, no
/// focus), and the card is 240px minimum, 420px maximum so the page never
/// becomes one giant block — the content scrolls inside.
/// The dim-on-disabled rule for every bounded editor, loaded once per
/// display: the theme keeps TextView text at full opacity when the widget
/// is insensitive, so a switched-off table would not read as dead.
fn load_editor_css() {
    static LOADED: std::sync::OnceLock<()> = std::sync::OnceLock::new();
    if LOADED.get().is_some() {
        return;
    }
    let _ = LOADED.set(());
    const EDITOR_CSS: &str = "
/* insensitive editor content dims; the theme skips TextViews */
.raw-editor textview:disabled {
  opacity: 0.45;
}
";
    if let Some(display) = gtk4::gdk::Display::default() {
        let provider = gtk4::CssProvider::new();
        provider.load_from_data(EDITOR_CSS);
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }
}

pub(super) fn bounded_card_editor(
    view: &TextView,
    hint_text: &str,
    a11y_label: &str,
) -> gtk4::ScrolledWindow {
    load_editor_css();
    view.remove_css_class("view");
    view.set_left_margin(16);
    view.set_right_margin(16);
    view.set_top_margin(12);
    view.set_bottom_margin(12);
    view.update_property(&[gtk4::accessible::Property::Label(a11y_label)]);
    let hint = gtk4::Label::new(Some(hint_text));
    hint.add_css_class("dim-label");
    hint.add_css_class("monospace");
    hint.set_xalign(0.0);
    hint.set_valign(gtk4::Align::Start);
    hint.set_halign(gtk4::Align::Start);
    hint.set_margin_start(16);
    hint.set_margin_top(12);
    hint.set_can_target(false);
    hint.set_focusable(false);
    let overlay = gtk4::Overlay::new();
    overlay.set_child(Some(view));
    overlay.add_overlay(&hint);
    {
        let hint_into = hint.clone();
        view.buffer().connect_changed(move |buf| {
            hint_into.set_visible(buf.char_count() == 0);
        });
        hint.set_visible(view.buffer().char_count() == 0);
    }
    let scroll = gtk4::ScrolledWindow::new();
    scroll.add_css_class("card");
    // The dim-on-disabled rule above is scoped to this class.
    scroll.add_css_class("raw-editor");
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_min_content_height(240);
    scroll.set_max_content_height(420);
    scroll.set_propagate_natural_height(true);
    scroll.set_child(Some(&overlay));
    scroll
}

/// Gate a raw-table editor on its switch: insensitive while the feature is
/// off (the bounded editor dims via CSS), and — when it turns off while
/// the user was typing, e.g. through a live config reload — the caret must
/// not stay behind in the dead editor, so focus is released.
pub(crate) fn gate_editor(switch: &adw::SwitchRow, editor: &gtk4::ScrolledWindow) {
    switch
        .bind_property("active", editor, "sensitive")
        .sync_create()
        .build();
    {
        let editor = editor.clone();
        switch.connect_active_notify(move |sw| {
            if sw.is_active() {
                return;
            }
            let Some(window) = editor.root().and_downcast::<gtk4::Window>() else {
                return;
            };
            let inside = gtk4::prelude::GtkWindowExt::focus(&window)
                .is_some_and(|focused| focused.is_ancestor(editor.upcast_ref::<gtk4::Widget>()));
            if inside {
                // GtkWindowExt::set_focus — the Widget method of the same
                // name means "take focus" and would fight this one.
                gtk4::prelude::GtkWindowExt::set_focus(&window, None::<&gtk4::Widget>);
            }
        });
    }
}

/// The raw-table line stats, by the parsers' own drop rule (`macros_from_
/// text` / `emoji_from_text`): a pair needs a `=` and non-empty key and
/// value. The error line only reports non-blank lines without a `=` — the
/// other drops stay silent, exactly as they always were. `section` picks
/// the message keys ("<section>.error_one"/"error_many").
pub(crate) fn line_stats(text: &str, section: &str) -> (usize, Option<String>) {
    let lines: Vec<&str> = text.lines().collect();
    let valid = lines
        .iter()
        .filter(|l| {
            l.split_once('=')
                .is_some_and(|(k, v)| !k.trim().is_empty() && !v.trim().is_empty())
        })
        .count();
    let bad: Vec<usize> = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && l.split_once('=').is_none())
        .map(|(i, _)| i + 1)
        .collect();
    let error = match bad.as_slice() {
        [] => None,
        [n] => Some(
            t(&format!("{section}.error_one"), "Dòng {n} thiếu dấu =")
                .replace("{n}", &n.to_string()),
        ),
        [first, ..] => Some(
            t(
                &format!("{section}.error_many"),
                "{k} dòng thiếu dấu =, bắt đầu từ dòng {n}",
            )
            .replace("{k}", &bad.len().to_string())
            .replace("{n}", &first.to_string()),
        ),
    };
    (valid, error)
}
