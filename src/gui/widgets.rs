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

/// Refresh the focused-app chip. The D-Bus call is sync with a 300 ms
/// timeout on the main thread — GTK types are not Send, so a worker thread
/// cannot touch the label — but the callers are startup and focus-leave,
/// moments when the window is idle anyway; with the extension installed the
/// call is near-instant.
pub(super) fn refresh_focused_app(label: &gtk4::Label, value: &Rc<RefCell<String>>) {
    match focused_app_name() {
        Ok((class, title)) => {
            label.set_text(&class);
            label.set_tooltip_text(Some(&title));
            *value.borrow_mut() = class;
        }
        Err(e) => {
            label.set_text(&e);
            label.set_tooltip_text(None);
            value.borrow_mut().clear();
        }
    }
}

pub(super) fn drop_down(items: &[&str], selected: u32) -> DropDown {
    let dd = DropDown::new(Some(StringList::new(items)), gtk4::Expression::NONE);
    dd.set_selected(selected);
    dd
}

/// A boxed-list boolean row (title + dim subtitle). Replaces the plain
/// Switch + pref_row pair of the old pages.
pub(super) fn switch_row(title: &str, subtitle: &str) -> adw::SwitchRow {
    adw::SwitchRow::builder()
        .title(title)
        .subtitle(subtitle)
        .build()
}

/// A boxed-list choice row; `selected` indexes into `items`.
pub(super) fn combo_row(title: &str, subtitle: &str, items: &[&str]) -> adw::ComboRow {
    adw::ComboRow::builder()
        .title(title)
        .subtitle(subtitle)
        .model(&StringList::new(items))
        .selected(0)
        .build()
}

/// A boxed-list number row; `step` is both the arrow increment and the
/// page increment (x10).
pub(super) fn spin_row(title: &str, subtitle: &str, min: f64, max: f64, step: f64) -> adw::SpinRow {
    let adjustment = gtk4::Adjustment::new(min, min, max, step, step * 10.0, 0.0);
    adw::SpinRow::builder()
        .title(title)
        .subtitle(subtitle)
        .adjustment(&adjustment)
        .build()
}

/// An info row for a boxed list (title + dim subtitle, no control). Not
/// activatable — the hover highlight on an activation-shaped row that does
/// nothing reads as a broken button.
pub(super) fn action_row(title: &str, subtitle: &str) -> adw::ActionRow {
    adw::ActionRow::builder()
        .title(title)
        .subtitle(subtitle)
        .activatable(false)
        .build()
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
    labels
        .iter()
        .find(|l| l.css_classes().iter().any(|c| c.as_str() == "subtitle"))
        .cloned()
        .or_else(|| labels.get(1).cloned())
}

/// The "(?)" marker: a small flat about icon — the same glyph the About
/// page carries — that explains the setting on hover (tooltip) or click
/// (dialog). The long text comes from the content file, so the owner tunes
/// it without code.
pub(super) fn attach_info<W: IsA<gtk4::Widget> + IsA<adw::ActionRow> + IsA<adw::PreferencesRow>>(
    row: &W,
    text: &str,
) {
    let btn = gtk4::Button::from_icon_name("help-about-symbolic");
    btn.add_css_class("flat");
    btn.set_tooltip_text(Some(text));
    btn.set_valign(gtk4::Align::Center);
    {
        let title = row.upcast_ref::<adw::PreferencesRow>().title().to_string();
        let text = text.to_string();
        btn.connect_clicked(move |b| {
            let dialog = adw::AlertDialog::builder()
                .heading(&title)
                .body(&text)
                .build();
            dialog.add_response("ok", &t("misc.info_ok", "Đã hiểu"));
            dialog.present(b.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }
    row.add_suffix(&btn);
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
