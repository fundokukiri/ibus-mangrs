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
/// The inline item and its label only exist once the row is mapped, so
/// the label tweak runs one idle after the first map; item widgets are
/// recycled, so the tweak sticks across selections.
fn widen_value(row: &adw::ComboRow) {
    fn find_value_view(w: &gtk4::Widget) -> Option<gtk4::ListView> {
        if let Some(view) = w.downcast_ref::<gtk4::ListView>() {
            return Some(view.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(view) = find_value_view(&c) {
                return Some(view);
            }
            child = c.next_sibling();
        }
        None
    }
    if let Some(view) = find_value_view(row.upcast_ref()) {
        view.set_hexpand(true);
    }
    // Apply exactly once per row: the item widgets are recycled, and
    // re-poking "the first label" after every map kept touching different
    // widgets — the accumulated expand/align settings are what nudged the
    // selected text toward the center over successive picks (a fresh window
    // reset it). A failed pass (the list not bound yet) retries on the next
    // map.
    let applied = std::rc::Rc::new(std::cell::Cell::new(false));
    row.connect_map(move |row| {
        if applied.get() {
            return;
        }
        let row = row.clone();
        let applied = applied.clone();
        gtk4::glib::idle_add_local_once(move || {
            let Some(view) = find_value_view(row.upcast_ref()) else {
                return;
            };
            let Some(label) = first_label(&view) else {
                return;
            };
            label.set_hexpand(true);
            label.set_xalign(1.0);
            applied.set(true);
        });
    });
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
