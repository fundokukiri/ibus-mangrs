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
                "Không hỏi được extension MăngRS Focus (data/extension)",
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

/// The experimental row carries a small "[Thử nghiệm]" badge in front of its
/// title, and its switch gets a spoken name mentioning the trial status.
/// The title label is not a public child either, so the badge is spliced into
/// the row's internal title box: the label moves into a horizontal pair that
/// replaces it. Every row type renders a title box; a row without one keeps
/// the badge in its suffixes area.
pub(super) fn decorate_experimental<W>(row: &W)
where
    W: IsA<gtk4::Widget> + IsA<adw::PreferencesRow>,
{
    let badge_text = t("misc.experimental_badge", "Thử nghiệm");
    let badge = gtk4::Label::new(Some(&format!("[{badge_text}]")));
    badge.add_css_class("caption");
    badge.add_css_class("warning");
    badge.add_css_class("experimental-badge");
    badge.set_valign(gtk4::Align::Center);
    uniform_title_width(row);
    let (suffixes, switch) = find_suffixes_and_switch(row.upcast_ref());
    if !badge_beside_title(row, &badge) {
        if let Some(suffixes) = suffixes {
            suffixes.prepend(&badge);
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

/// Move the row's title into a horizontal pair led by `badge`, so the tag
/// rides the title's own line instead of a second one.
fn badge_beside_title<W>(row: &W, badge: &gtk4::Label) -> bool
where
    W: IsA<gtk4::Widget> + IsA<adw::PreferencesRow>,
{
    let (Some(title), Some(title_box)) = (
        row_title(row),
        row_title(row).and_then(|l| l.parent().and_downcast::<gtk4::Box>()),
    ) else {
        return false;
    };
    let line = gtk4::Box::new(gtk4::Orientation::Horizontal, 8);
    title.unparent();
    line.append(badge);
    line.append(&title);
    title_box.prepend(&line);
    true
}

/// A flat popover-list button whose label starts at the left edge. The stock
/// button centers its label, which makes a list of app names or menu items
/// read as a column of pills instead of a list.
pub(super) fn list_button(label: &str) -> gtk4::Button {
    let text = gtk4::Label::new(Some(label));
    text.set_xalign(0.0);
    text.set_halign(gtk4::Align::Start);
    text.set_margin_start(6);
    let btn = gtk4::Button::new();
    btn.add_css_class("flat");
    btn.set_child(Some(&text));
    btn
}

/// A boxed-list choice row; `selected` indexes into `items`. Every combo in
/// the app goes through here so the popup interaction stays uniform: click
/// picks, hover only highlights (see `combo_popup_factory`).
pub(super) fn combo_row(title: &str, subtitle: &str, items: &[&str]) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_use_markup(false);
    row.set_title(title);
    row.set_subtitle(subtitle);
    row.set_model(Some(&StringList::new(items)));
    row.set_selected(0);
    row.set_list_factory(Some(&combo_popup_factory()));
    // The popup ListView is a template child, built with the row — walkable
    // and fixable right away.
    unset_hover_select(row.upcast_ref());
    row
}

/// The popup ListView must not select whatever row the pointer crosses:
/// libadwaita's combo template ships it with single-click-activate on, and
/// GTK reads that flag as "activate on click AND select on hover" — the
/// selected look (accent row, checkmark) swept along with the mouse instead
/// of waiting for a press. Off, a press alone picks again: the factory's
/// click gesture selects the clicked item before firing the activate action
/// upstream commits from. The inline value view is walked too; its flag was
/// already off, so this is a no-op there.
fn unset_hover_select(w: &gtk4::Widget) {
    if let Some(view) = w.downcast_ref::<gtk4::ListView>() {
        view.set_single_click_activate(false);
    }
    let mut child = w.first_child();
    while let Some(c) = child {
        unset_hover_select(&c);
        child = c.next_sibling();
    }
}

/// The popup list factory every combo row ships with: a rebuild of the
/// upstream default factory's rows — label plus the selected-item checkmark.
/// The checkmark's opacity follows `item.selected`: the popup model is a
/// single selection mirroring the row's selection, so it marks exactly the
/// item the popup will commit. The click gesture is the pick path now that
/// `unset_hover_select` turned hover-select off: select first (upstream's
/// row_activated_cb reads the popup selection, not the signal position),
/// then fire `list.activate-item` — the same action GTK's Enter uses — so
/// upstream commits and pops the popover down. The item rides a weak ref:
/// a strong one would cycle (item → controller → closure → item) and leak
/// every pooled popup item.
fn combo_popup_factory() -> gtk4::SignalListItemFactory {
    let factory = gtk4::SignalListItemFactory::new();
    let wired: Rc<std::cell::RefCell<std::collections::HashSet<usize>>> =
        Rc::new(std::cell::RefCell::new(std::collections::HashSet::new()));
    let wired_bind = Rc::clone(&wired);
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
    factory.connect_bind(move |_, obj| {
        let wired = Rc::clone(&wired_bind);
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
        // The pick gesture belongs on the row widget, not on the item's
        // content: the content box is shorter than the row, so a click near a
        // row edge moved the popup selection (GTK's own press handler works on
        // the whole row) and never reached the gesture — the checkmark moved,
        // the value stayed. One gesture per row: the rows are pooled.
        let Some(row) = item.child().and_then(|c| c.parent()) else {
            return;
        };
        let key = row.as_ptr() as usize;
        if !wired.borrow_mut().insert(key) {
            return;
        }
        // The rows are pooled and destroyed with the popup: a stale entry
        // would leave a future row at the same address without its gesture.
        let wired_destroy = Rc::clone(&wired);
        row.connect_destroy(move |_| {
            wired_destroy.borrow_mut().remove(&key);
        });
        add_pick_gesture(&row, item);
    });
    factory
}

/// The one path that commits a pick: select the pressed row in the popup model,
/// then fire the action upstream's handler commits from. Attached to the row
/// widget, so it cannot miss a click the popup accepted.
fn add_pick_gesture(row: &gtk4::Widget, item: &gtk4::ListItem) {
    let weak_item = gtk4::glib::WeakRef::new();
    weak_item.set(Some(item));
    let click = gtk4::GestureClick::new();
    click.set_button(gtk4::gdk::BUTTON_PRIMARY);
    click.connect_released(move |_, n_press, _, _| {
        if n_press != 1 {
            return;
        }
        let Some(item) = weak_item.upgrade() else {
            return;
        };
        let Some(row) = item.child().and_then(|c| c.parent()) else {
            return;
        };
        let Some(view) = row
            .ancestor(gtk4::ListView::static_type())
            .and_downcast::<gtk4::ListView>()
        else {
            return;
        };
        let Some(model) = view.model().and_downcast::<gtk4::SingleSelection>() else {
            return;
        };
        let position = item.position();
        if position == gtk4::INVALID_LIST_POSITION {
            return;
        }
        model.select_item(position, false);
        // The action lives on the ListView's widget class (gtklistview.c
        // installs "list.activate-item") and the row is its descendant, so the
        // Err is unreachable by construction and there is nothing to fall back
        // to on an input path.
        let _ = row.activate_action("list.activate-item", Some(&position.to_variant()));
    });
    row.add_controller(click);
}

/// One regular (bordered) action button; `icon_name: None` renders label
/// only. A page's SizeGroup gives them all the widest button's width and
/// their shared label rows keep the heights equal.
pub(super) fn action_button(icon_name: Option<&str>, label: &str) -> gtk4::Button {
    let btn = gtk4::Button::with_label(label);
    btn.set_valign(gtk4::Align::Center);
    if let Some(icon_name) = icon_name {
        let icon = gtk4::Image::from_icon_name(icon_name);
        let text = btn
            .child()
            .and_downcast::<gtk4::Label>()
            .expect("with_label");
        // Detach the label before re-parenting it into the content box.
        btn.set_child(None::<&gtk4::Widget>);
        let content = gtk4::Box::new(Orientation::Horizontal, 6);
        content.append(&icon);
        content.append(&text);
        btn.set_child(Some(&content));
    }
    btn
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
    let divider = pacing_divider();
    row.add_suffix(&switch);
    row.add_suffix(&divider);
    row.add_suffix(value);
    // Merged rows only: the control group hugs the text block instead of
    // floating at the row's right edge — the suffix box expands so its
    // start-packed children land right after the fixed-width title, and
    // the slack moves to the row's end.
    if let (Some(suffixes), _) = find_suffixes_and_switch(row.upcast_ref()) {
        suffixes.set_hexpand(true);
    }
    (row, switch)
}

/// Like `switch_value_row`, but the value sits *left* of the switch — the
/// order a mode picker reads in (`[mode] [on/off]`).
pub(super) fn select_switch_row<W: IsA<gtk4::Widget>>(
    title: &str,
    subtitle: &str,
    value: &W,
) -> (adw::ActionRow, gtk4::Switch) {
    let row = action_row(title, subtitle);
    let switch = gtk4::Switch::new();
    switch.set_valign(gtk4::Align::Center);
    value.set_valign(gtk4::Align::Center);
    row.add_suffix(value);
    row.add_suffix(&pacing_divider());
    row.add_suffix(&switch);
    // No hexpand here: an expanded suffix box packs its children from the left
    // and leaves the switch floating short of the row's edge, where every
    // plain switch row puts it. Natural width puts the switch last, flush with
    // the ones above and below.
    (row, switch)
}

/// One width for every merged-row value control: the rows must read as a
/// balanced column, not some boxes short and some long. In pixels, not
/// width_chars — the spin's built-in arrow stack makes a 12-char spin
/// wider than a 12-char entry, which would skew the divider column.
const VALUE_WIDTH_PX: i32 = 120;

/// The fixed width of every decorated row's text block: with one shared
/// title width the switch/divider/value controls all start at the same x
/// across the tab.
const TITLE_WIDTH_PX: i32 = 340;

/// Pin the row's internal title box (subtitle's parent) to the shared text
/// width — the divider column lines up across all rows of the tab.
pub(crate) fn uniform_title_width<W>(row: &W)
where
    W: IsA<gtk4::Widget> + IsA<adw::PreferencesRow>,
{
    if let Some(box_) = row_subtitle(row).and_then(|l| l.parent().and_downcast::<gtk4::Box>()) {
        box_.set_size_request(TITLE_WIDTH_PX, -1);
    }
}

/// The internal suffixes box and switch of an Adw row, through the same
/// private-child walk the badge used to take.
fn find_suffixes_and_switch(w: &gtk4::Widget) -> (Option<gtk4::Box>, Option<gtk4::Switch>) {
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
    let mut box_out = None;
    let mut switch_out = None;
    walk(w, &mut box_out, &mut switch_out);
    (box_out, switch_out)
}

/// The value half of a merged row: a bare SpinButton without SpinRow's
/// title — the merged row's title carries the meaning. Same adjustment
/// shape as `spin_row`.
pub(super) fn spin_button(min: f64, max: f64, step: f64) -> gtk4::SpinButton {
    let adjustment = gtk4::Adjustment::new(min, min, max, step, step * 10.0, 0.0);
    let spin = gtk4::SpinButton::new(Some(&adjustment), step, 0);
    spin.set_size_request(VALUE_WIDTH_PX, -1);
    spin
}

/// The vertical divider between two pacing columns of a merged row — the
/// same separator `switch_value_row` draws between its halves.
pub(super) fn pacing_divider() -> gtk4::Separator {
    let divider = gtk4::Separator::new(gtk4::Orientation::Vertical);
    divider.set_margin_start(8);
    divider.set_margin_end(8);
    divider
}

/// One pacing column of a merged row: the caption above the spin. The
/// caller places the column (dividers go between columns). The spin is
/// centered, not the default Fill: the vertical box hands a filling child
/// the whole column width — the caption's width — so a long caption
/// ("Hoàn dấu sau Backspace (ms)") stretches its input while its siblings
/// stay short. Centered, every spin renders at the shared VALUE_WIDTH_PX.
pub(super) fn pacing_column(caption: &str, spin: &gtk4::SpinButton) -> gtk4::Box {
    let label = gtk4::Label::new(Some(caption));
    label.add_css_class("caption");
    label.set_halign(gtk4::Align::Center);
    spin.set_halign(gtk4::Align::Center);
    let column = gtk4::Box::new(gtk4::Orientation::Vertical, 4);
    column.append(&label);
    column.append(spin);
    column
}

/// Equalize the caption columns of the merged rows. Captions differ in
/// length ("Nhịp xoá lùi" against "Chờ sau khi viết lại từ"), so packed at their
/// natural widths the divider lands off-centre and each row starts at its
/// own x. Every column takes the widest caption's width, which centres each
/// divider and lines the rows up with each other.
pub(super) fn equalize_columns(columns: &[gtk4::Box]) {
    let widest = columns
        .iter()
        .map(|c| c.measure(gtk4::Orientation::Horizontal, -1).1)
        .max()
        .unwrap_or(0);
    for column in columns {
        column.set_size_request(widest, -1);
    }
}

/// The text half of a merged row: a plain entry sized to the shared value
/// width (a short character list, `: " ').
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
