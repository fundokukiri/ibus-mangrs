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

/// A dictionary-style card editor: a monospace text view in a rounded
/// card, with a dim placeholder overlay that disappears once text exists.
/// The view drops its own .view background — it would paint over the
/// card's rounded corners, and the card bg is the same color in both
/// themes.
pub(super) fn card_text_editor(view: &TextView, placeholder_text: &str) -> gtk4::ScrolledWindow {
    view.remove_css_class("view");
    view.set_vexpand(true);
    let placeholder = gtk4::Label::new(Some(placeholder_text));
    placeholder.add_css_class("dim-label");
    placeholder.add_css_class("caption");
    placeholder.set_valign(gtk4::Align::Center);
    // A visible overlay label still targets pointer events and would eat
    // the first click into the empty editor.
    placeholder.set_can_target(false);
    let editor = gtk4::Overlay::new();
    editor.set_child(Some(view));
    editor.add_overlay(&placeholder);
    {
        let buf = view.buffer();
        placeholder.set_visible(buf.char_count() == 0);
        buf.connect_changed(move |b| placeholder.set_visible(b.char_count() == 0));
    }
    let scroll = gtk4::ScrolledWindow::new();
    scroll.add_css_class("card");
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_min_content_height(240);
    scroll.set_child(Some(&editor));
    scroll
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
pub(super) fn bounded_card_editor(
    view: &TextView,
    hint_text: &str,
    a11y_label: &str,
) -> gtk4::ScrolledWindow {
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
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_min_content_height(240);
    scroll.set_max_content_height(420);
    scroll.set_propagate_natural_height(true);
    scroll.set_child(Some(&overlay));
    scroll
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

/// A link that opens in the default browser, drawn as a flat icon —
/// send-to (an arrow leaving a box) stands in for external-link, which the
/// shipped Adwaita theme no longer carries.
pub(super) fn link_button(uri: &str) -> gtk4::LinkButton {
    let image = gtk4::Image::from_icon_name("send-to-symbolic");
    let button = gtk4::LinkButton::builder().uri(uri).child(&image).build();
    button.add_css_class("flat");
    button
}
