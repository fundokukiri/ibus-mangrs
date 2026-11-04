//! Page "Menu panel": the user's own design of the panel popup — folders
//! (one level deep), toggles, radio groups, actions and separators, freely
//! ordered. The design vector is the source of truth; the boxed list is
//! its view: one native row per item, drag handles plus a per-row menu.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let menu = adw::PreferencesGroup::builder()
        .title(t("groups.panel.menu.title", "Menu trên panel"))
        .description(t(
            "groups.panel.menu.description",
            "Tùy chỉnh menu popup trên panel IBus.",
        ))
        .build();

    // Header: add (popover of the addable items) and reset (confirmed — it
    // throws the whole design away).
    let add_btn = gtk4::MenuButton::new();
    add_btn.set_label(&t("panel.add_button", "Thêm mục"));
    add_btn.add_css_class("flat");
    add_btn.set_tooltip_text(Some(&t("panel.add_tooltip", "Thêm mục vào cuối menu")));
    let add_popover = gtk4::Popover::new();
    add_popover.set_child(Some(&ui.menu_add_list(&MenuScope::Root, &add_popover)));
    add_btn.set_popover(Some(&add_popover));
    let reset_btn = gtk4::Button::with_label(&t("panel.reset_button", "Đặt lại"));
    reset_btn.add_css_class("flat");
    reset_btn.set_tooltip_text(Some(&t(
        "panel.reset_tooltip",
        "Đặt lại menu về thiết kế mặc định",
    )));
    let header = gtk4::Box::new(Orientation::Horizontal, 6);
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

    page
}
