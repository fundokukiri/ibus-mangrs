//! Page "Chung": typing on/off, input style and the preedit underline —
//! the things users touch every day. The keyboard shortcuts live on their
//! own "Phím tắt" page (see hotkeys.rs). The rows come from `build`; this
//! file groups them — no group repeats a row's title — and caps the combo
//! subtitles at two lines so they can never squeeze the selected value.

use super::*;

/// The selected value of a combo row must not be squeezed to "Gạch
/// chân…": Adwaita lets the title box take every spare pixel and shrinks
/// the inline value view to its minimum (an ellipsized label's minimum).
/// Making the value view expand splits the spare space with the title
/// box, and the value label right-aligns inside its half, so long values
/// stay whole at the default width and only ellipsize on a narrow window.
/// Alignment must come from a factory-owned label: the item widgets are
/// rebuilt for every pick, so poking "the first label" of the internal
/// ListView either raced the item bind or targeted another pooled item —
/// the value then sat at the default alignment until the window reopened.
fn widen_value(row: &adw::ComboRow) {
    if let Some(view) = combo_value_view(row) {
        view.set_hexpand(true);
    }
    // Two factories: the display label right-aligns; the popup keeps the
    // default look and rebuilds the selected-item checkmark that lives in
    // the default factory ours replaces.
    row.set_factory(Some(&value_factory()));
    row.set_list_factory(Some(&popup_factory()));
}

/// The item factory for the row's inline value: the label is born
/// right-aligned, so every pick shows an aligned label without racing
/// the item bind.
fn value_factory() -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::builder()
            .hexpand(true)
            .xalign(1.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .build();
        item.set_child(Some(&label));
    });
    factory.connect_bind(|_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item.child().and_downcast::<gtk4::Label>() else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|s| s.string().to_string())
            .unwrap_or_default();
        label.set_text(&text);
    });
    factory
}

/// The item factory for the popup list: a rebuild of the upstream default
/// factory's rows — label plus the selected-item checkmark it carries.
/// The checkmark exists only in the default factory, so replacing the
/// factory loses it unless rebuilt here. Opacity follows `item.selected`:
/// the popup model is a single selection mirroring the row's selection
/// (adw_combo_row.c), so it marks exactly the combo's chosen item.
fn popup_factory() -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    factory.connect_setup(|_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let label = gtk4::Label::builder()
            .xalign(0.0)
            .ellipsize(gtk4::pango::EllipsizeMode::End)
            .max_width_chars(20)
            .width_chars(1)
            .valign(gtk4::Align::Center)
            .build();
        let icon = gtk4::Image::builder()
            .accessible_role(gtk4::AccessibleRole::Presentation)
            .icon_name("object-select-symbolic")
            .build();
        let item_box = gtk4::Box::new(gtk4::Orientation::Horizontal, 0);
        item_box.append(&label);
        item_box.append(&icon);
        item.set_child(Some(&item_box));
        // Setup runs once per item widget: one binding per item, no
        // unbind bookkeeping. The transform captures nothing — the icon
        // comes off the binding — because notify handlers must be
        // Send + Sync and widgets are not.
        item.bind_property("selected", &icon, "opacity")
            .sync_create()
            .transform_to(|_, selected: bool| {
                Some(gtk4::glib::Value::from(if selected { 1.0f64 } else { 0.0 }))
            })
            .build();
    });
    factory.connect_bind(|_, obj| {
        let Some(item) = obj.downcast_ref::<gtk4::ListItem>() else {
            return;
        };
        let Some(label) = item
            .child()
            .and_downcast::<gtk4::Box>()
            .and_then(|item_box| item_box.first_child())
            .and_downcast::<gtk4::Label>()
        else {
            return;
        };
        let text = item
            .item()
            .and_downcast::<gtk4::StringObject>()
            .map(|s| s.string().to_string())
            .unwrap_or_default();
        label.set_text(&text);
    });
    factory
}

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // The on/off switch leads the page alone; a "Trạng thái / Bật tiếng
    // Việt" group header would repeat the row title.
    let status = adw::PreferencesGroup::new();
    status.add(&ui.enabled);
    page.add(&status);

    let style = adw::PreferencesGroup::builder()
        .title(t("groups.typing.style.title", "Cách gõ"))
        .build();
    style.add(&ui.method);
    style.add(&ui.charset);
    style.add(&ui.mode);
    page.add(&style);

    let display = adw::PreferencesGroup::builder()
        .title(t("groups.typing.display.title", "Hiển thị"))
        .build();
    display.add(&ui.hide_underline);
    page.add(&display);

    // Two subtitle lines max, full value width: a long subtitle can never
    // squeeze the selected value again.
    for row in [&ui.method, &ui.charset, &ui.mode] {
        row.set_subtitle_lines(2);
        widen_value(row);
    }

    page
}
