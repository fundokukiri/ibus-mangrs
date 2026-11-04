//! Small widget builders shared by the pages, plus save/import feedback
//! (alert dialog, focused-app chip).

use super::*;

// --- small builders ---

/// Surface a save/import failure in the window: an `eprintln!` is invisible
/// from a GUI, and a silent failure looks exactly like a broken feature.
pub(super) fn alert(window: &impl IsA<gtk4::Widget>, text: &str) {
    adw::AlertDialog::builder()
        .heading("Không lưu được cấu hình")
        .body(text)
        .build()
        .present(Some(window));
}

/// Ask the focus extension for the focused window — the same session-bus
/// call the engine makes on unnamed focus-ins (`(ss)` wm_class, title).
pub(super) fn focused_app_name() -> Result<(String, String), String> {
    let conn =
        gtk4::gio::bus_get_sync(gtk4::gio::BusType::Session, None::<&gtk4::gio::Cancellable>)
            .map_err(|e| format!("Không nối được D-Bus: {e}"))?;
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
            "Không hỏi được extension MăngRS Focus (data/gnome-shell-extension)".to_string()
        })?;
    let (class, title) = reply
        .get::<(String, String)>()
        .ok_or("Extension trả dữ liệu không đúng dạng")?;
    if class.is_empty() {
        return Err("Không có cửa sổ nào đang focus".to_string());
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

pub(super) fn text_view(content: &str) -> TextView {
    let view = TextView::builder()
        .hexpand(true)
        .monospace(true)
        .left_margin(10)
        .right_margin(10)
        .top_margin(8)
        .bottom_margin(8)
        .build();
    view.buffer().set_text(content);
    view
}

/// Wrap a text view in a fixed-height scrolled window so a long list cannot
/// stretch (or collapse) the whole tab.
pub(super) fn scrolled(view: &TextView, height: i32) -> gtk4::ScrolledWindow {
    gtk4::ScrolledWindow::builder()
        .child(view)
        .height_request(height)
        .hexpand(true)
        .vscrollbar_policy(gtk4::PolicyType::Automatic)
        .build()
}

/// A link that opens in the default browser.
pub(super) fn link_button(label_text: &str, uri: &str) -> gtk4::LinkButton {
    gtk4::LinkButton::builder()
        .label(label_text)
        .uri(uri)
        .build()
}

/// Section header: bold title + dim subtitle (one size larger than rows).
pub(super) fn section(box_: &gtk4::Box, title: &str, subtitle: &str) {
    let t = gtk4::Label::new(Some(title));
    t.add_css_class("section-title");
    t.set_halign(gtk4::Align::Start);
    t.set_margin_top(20);
    t.set_margin_bottom(2);
    box_.append(&t);
    if !subtitle.is_empty() {
        let s = gtk4::Label::new(Some(subtitle));
        s.add_css_class("section-sub");
        s.set_halign(gtk4::Align::Start);
        s.set_xalign(0.0);
        s.set_wrap(true);
        s.set_margin_bottom(10);
        box_.append(&s);
    }
}

/// A section whose card starts collapsed: clicking the title toggles it.
/// GTK4 dropped Expander, so this is a flat button over a Revealer. Used for
/// the long customization lists — they would otherwise dominate the page.
pub(super) fn collapsible_section(
    parent: &gtk4::Box,
    title: &str,
    subtitle: &str,
    rows: Vec<gtk4::Box>,
) {
    let header = gtk4::Button::new();
    header.add_css_class("flat");
    let h = gtk4::Box::new(Orientation::Horizontal, 8);
    let arrow = gtk4::Label::new(Some("▸"));
    let t = gtk4::Label::new(Some(title));
    t.add_css_class("section-title");
    h.set_halign(gtk4::Align::Start);
    h.append(&arrow);
    h.append(&t);
    header.set_child(Some(&h));
    header.set_halign(gtk4::Align::Start);
    let revealer = Revealer::builder()
        .transition_type(gtk4::RevealerTransitionType::SlideDown)
        .transition_duration(150)
        .reveal_child(false)
        .build();
    revealer.set_child(Some(&card(rows)));
    {
        let revealer = revealer.clone();
        let arrow = arrow.clone();
        header.connect_clicked(move |_| {
            // The arrow must follow the revealer, not the click count: the
            // search handler also opens and closes it.
            let open = !revealer.reveals_child();
            revealer.set_reveal_child(open);
            arrow.set_text(if open { "▼" } else { "▸" });
        });
    }
    parent.append(&header);
    if !subtitle.is_empty() {
        let s = gtk4::Label::new(Some(subtitle));
        s.add_css_class("section-sub");
        s.set_halign(gtk4::Align::Start);
        s.set_xalign(0.0);
        s.set_wrap(true);
        s.set_margin_bottom(10);
        parent.append(&s);
    }
    parent.append(&revealer);
    COLLAPSIBLES.with(|c| c.borrow_mut().push((revealer, arrow)));
}

/// A grouped card: rows separated by hairline separators, rounded background.
pub(super) fn card(rows: Vec<gtk4::Box>) -> gtk4::Box {
    let c = gtk4::Box::new(Orientation::Vertical, 0);
    c.add_css_class("group-card");
    c.set_margin_bottom(6);
    for (i, r) in rows.iter().enumerate() {
        if i > 0 {
            c.append(&gtk4::Separator::new(Orientation::Horizontal));
        }
        c.append(r);
    }
    c
}

/// A card holding a single control (text views, the quick-add row).
pub(super) fn card_widget(widget: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    let c = gtk4::Box::new(Orientation::Vertical, 0);
    c.add_css_class("group-card");
    c.set_margin_bottom(6);
    c.append(widget);
    c
}

/// Generic settings row: optional leading icon, title + small dim subtitle,
/// optional control on the right. Registered for the search bar.
pub(super) fn row_impl(
    icon: Option<&str>,
    title: &str,
    subtitle: &str,
    widget: Option<&gtk4::Widget>,
) -> gtk4::Box {
    let h = gtk4::Box::new(Orientation::Horizontal, 12);
    h.set_margin_start(16);
    h.set_margin_end(16);
    h.set_margin_top(10);
    h.set_margin_bottom(10);
    if let Some(name) = icon {
        let img = gtk4::Image::from_icon_name(name);
        img.set_pixel_size(20);
        img.add_css_class("row-icon");
        img.set_valign(gtk4::Align::Center);
        h.append(&img);
    }
    let v = gtk4::Box::new(Orientation::Vertical, 2);
    v.set_valign(gtk4::Align::Center);
    v.set_hexpand(true);
    let t = gtk4::Label::new(Some(title));
    t.add_css_class("row-title");
    t.set_xalign(0.0);
    t.set_wrap(true);
    v.append(&t);
    if !subtitle.is_empty() {
        let sub = gtk4::Label::new(Some(subtitle));
        sub.add_css_class("row-sub");
        sub.set_xalign(0.0);
        sub.set_wrap(true);
        v.append(&sub);
    }
    h.append(&v);
    if let Some(w) = widget {
        w.set_valign(gtk4::Align::Center);
        h.append(w);
    }
    // Hovering anywhere on the row explains the feature.
    h.set_tooltip_text(Some(&format!("{title}: {subtitle}")));
    h
}

/// A settings row: title + small dim subtitle on the left, control right.
pub(super) fn pref_row(title: &str, subtitle: &str, widget: &impl IsA<gtk4::Widget>) -> gtk4::Box {
    row_impl(None, title, subtitle, Some(widget.upcast_ref()))
}

/// Icon + title + subtitle, no control — feature/project lists on About.
pub(super) fn info_row(icon: &str, title: &str, subtitle: &str) -> gtk4::Box {
    row_impl(Some(icon), title, subtitle, None)
}

pub(super) fn hint(box_: &gtk4::Box, text: &str) {
    // Small and dimmed: hints explain the row above, they must not compete
    // with it. markup_escape_text because some hints contain < > &.
    let l = gtk4::Label::new(None);
    l.set_use_markup(true);
    l.set_markup(&format!(
        "<small>{}</small>",
        gtk4::glib::markup_escape_text(text)
    ));
    l.set_halign(gtk4::Align::Start);
    l.set_xalign(0.0);
    l.set_wrap(true);
    l.add_css_class("dim-label");
    l.set_margin_start(4);
    box_.append(&l);
}
