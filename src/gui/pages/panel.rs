//! Page "Menu panel": the user's own design of the panel popup — folders
//! (one level deep), toggles, radio groups, actions and separators, freely
//! ordered. The design vector is the source of truth; the boxed list is
//! its view. The page loads the editor's small CSS block and hosts the
//! toasts; the window shell stays untouched.

use super::*;

/// The page's only CSS: fixed leading columns keep handles and icons
/// aligned, the expanded children share their header's card with a guide
/// line, drags show an accent line and dim the dragged row. Named colors
/// only — the look holds in dark, light and any accent.
const PANEL_CSS: &str = "
/* the empty type-icon column keeps every title at the same x */
.menu-icon {
  min-width: 16px;
}
/* the expanded children sit on the same card as their header */
.menu-child {
  background: none;
}
/* a thin vertical guide line shows the hierarchy level */
.menu-child {
  border-left: 1px solid alpha(@borders, 0.7);
}
/* the drag insert position shows as a thin accent line */
.menu-drop-line {
  border-bottom: 2px solid @accent_color;
}
/* the row dims while it is being dragged */
.menu-dragging {
  opacity: 0.4;
}
";

pub(crate) fn build(ui: &Rc<Ui>) -> adw::ToastOverlay {
    let page = adw::PreferencesPage::new();

    // The editor's one CSS block; a display exists whenever the GUI runs.
    let provider = gtk4::CssProvider::new();
    provider.load_from_data(PANEL_CSS);
    if let Some(display) = gtk4::gdk::Display::default() {
        gtk4::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk4::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );
    }

    let menu = adw::PreferencesGroup::builder()
        .title(t("groups.panel.menu.title", "Menu trên panel"))
        .description(t(
            "groups.panel.menu.description",
            "Tùy chỉnh menu popup trên panel IBus.",
        ))
        .build();

    // Header: add (popover of the addable items) and reset (confirmed — it
    // throws the whole design away). Flat buttons against the title.
    let add_btn = gtk4::MenuButton::new();
    add_btn.set_icon_name("list-add-symbolic");
    add_btn.set_label(&t("panel.add_button", "Thêm mục"));
    add_btn.set_always_show_arrow(true);
    add_btn.add_css_class("flat");
    add_btn.set_tooltip_text(Some(&t("panel.add_tooltip", "Thêm mục vào cuối menu")));
    let add_popover = gtk4::Popover::new();
    add_popover.set_child(Some(&ui.menu_add_list(&MenuScope::Root, &add_popover)));
    add_btn.set_popover(Some(&add_popover));
    let reset_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
    reset_btn.add_css_class("flat");
    reset_btn.set_tooltip_text(Some(&t(
        "panel.reset_tooltip",
        "Đặt lại menu về thiết kế mặc định",
    )));
    reset_btn.update_property(&[gtk4::accessible::Property::Label(&t(
        "panel.reset_tooltip",
        "Đặt lại menu về thiết kế mặc định",
    ))]);
    let header = gtk4::Box::new(Orientation::Horizontal, 6);
    header.set_valign(gtk4::Align::Center);
    header.append(&add_btn);
    header.append(&reset_btn);
    menu.set_header_suffix(Some(&header));
    menu.add(&ui.menu_box);
    page.add(&menu);

    // Reset is the one destructive-enough action with a confirmation: a
    // stray click must not throw a hand-designed menu away.
    {
        let ui = ui.clone();
        reset_btn.connect_clicked(move |btn| {
            let dialog = adw::AlertDialog::builder()
                .heading(t("panel.reset_heading", "Đặt lại menu?"))
                .body(t(
                    "panel.reset_body",
                    "Mọi thay đổi thiết kế hiện tại sẽ mất.",
                ))
                .build();
            dialog.add_response("cancel", &t("panel.rename_cancel", "Hủy"));
            dialog.add_response("apply", &t("panel.reset_button", "Đặt lại"));
            dialog.set_response_appearance("apply", adw::ResponseAppearance::Destructive);
            dialog.set_default_response(Some("apply"));
            dialog.set_close_response("cancel");
            {
                let ui = ui.clone();
                dialog.connect_response(Some("apply"), move |_, _| {
                    ui.menu_reset();
                    ui.fill_menu_list();
                    ui.fire_save();
                });
            }
            dialog.present(btn.root().and_downcast::<gtk4::Window>().as_ref());
        });
    }

    let overlay = adw::ToastOverlay::new();
    overlay.set_child(Some(&page));
    overlay
}
