//! Page "Menu panel": the user's own design of the panel popup — folders
//! (one level deep), toggles, radio groups, actions and separators, freely
//! ordered. A TreeView editor, kept deliberately: a ListView+TreeListModel
//! rewrite would triple the code for no user gain.

use super::*;

#[allow(deprecated)] // TreeView/ComboBoxText, see the mod.rs import note
pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    let menu = adw::PreferencesGroup::builder()
        .title("Menu trên panel")
        .description(
            "Tự thiết kế menu popup — Bảng mã / Kiểu gõ / Chế độ gõ cũng chỉnh được mục bên trong",
        )
        .build();
    let menu_add_btn = gtk4::Button::with_label("Thêm");
    menu_add_btn.add_css_class("flat");
    let menu_folder_btn = gtk4::Button::with_label("Thêm thư mục");
    menu_folder_btn.add_css_class("flat");
    let menu_rename_btn = gtk4::Button::with_label("Đổi tên");
    menu_rename_btn.add_css_class("flat");
    let row1 = gtk4::Box::new(Orientation::Horizontal, 8);
    row1.set_margin_start(10);
    row1.set_margin_end(10);
    row1.set_margin_top(8);
    row1.set_margin_bottom(4);
    row1.append(&ui.menu_kind);
    row1.append(&menu_add_btn);
    row1.append(&menu_folder_btn);
    row1.append(&ui.menu_name);
    row1.append(&menu_rename_btn);
    let menu_up_btn = gtk4::Button::from_icon_name("go-up-symbolic");
    menu_up_btn.add_css_class("flat");
    let menu_down_btn = gtk4::Button::from_icon_name("go-down-symbolic");
    menu_down_btn.add_css_class("flat");
    let menu_out_btn = gtk4::Button::from_icon_name("go-previous-symbolic");
    menu_out_btn.add_css_class("flat");
    let menu_del_btn = gtk4::Button::from_icon_name("edit-delete-symbolic");
    menu_del_btn.add_css_class("flat");
    let menu_reset_btn = gtk4::Button::from_icon_name("view-refresh-symbolic");
    menu_reset_btn.add_css_class("flat");
    menu_reset_btn.set_tooltip_text(Some("Đặt lại menu về thiết kế mặc định"));
    for (btn, tip) in [
        (&menu_up_btn, "Lên — đổi thứ tự mục đang chọn"),
        (&menu_down_btn, "Xuống"),
        (&menu_out_btn, "Ra ngoài thư mục (chỉ con của thư mục)"),
        (&menu_del_btn, "Xoá mục đang chọn (thư mục phải rỗng)"),
    ] {
        btn.set_tooltip_text(Some(tip));
    }
    let row2 = gtk4::Box::new(Orientation::Horizontal, 8);
    row2.set_margin_start(10);
    row2.set_margin_end(10);
    row2.set_margin_top(4);
    row2.set_margin_bottom(8);
    row2.append(&menu_up_btn);
    row2.append(&menu_down_btn);
    row2.append(&menu_out_btn);
    row2.append(&menu_del_btn);
    row2.append(&menu_reset_btn);
    let menu_scroll = gtk4::ScrolledWindow::new();
    menu_scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    menu_scroll.set_min_content_height(260);
    menu_scroll.set_child(Some(&ui.menu_view));
    menu.add(&row1);
    menu.add(&row2);
    menu.add(&menu_scroll);
    page.add(&menu);

    // The editor buttons mutate the tree, then save through the shared
    // funnel — the design is on disk (and the panel, after a reload) before
    // the window loses focus.
    {
        let ui = ui.clone();
        menu_add_btn.connect_clicked(move |_| {
            let Some(id) = ui.menu_kind.active_id() else {
                return;
            };
            let Some((kind, key)) = id.split_once(':') else {
                return;
            };
            ui.menu_insert(kind, key);
            ui.fire_save();
        });
    }
    {
        let ui = ui.clone();
        menu_folder_btn.connect_clicked(move |_| {
            let name = ui.menu_name.text().to_string();
            ui.menu_add_folder(&name);
            ui.menu_name.set_text("");
            ui.fire_save();
        });
    }
    {
        let ui = ui.clone();
        menu_rename_btn.connect_clicked(move |_| {
            let name = ui.menu_name.text().to_string();
            ui.menu_rename(&name);
            ui.fire_save();
        });
    }
    for (btn, delta) in [(&menu_up_btn, -1), (&menu_down_btn, 1)] {
        let ui = ui.clone();
        btn.connect_clicked(move |_| {
            ui.menu_move(delta);
            ui.fire_save();
        });
    }
    {
        let ui = ui.clone();
        menu_out_btn.connect_clicked(move |_| {
            ui.menu_out();
            ui.fire_save();
        });
    }
    {
        let ui = ui.clone();
        menu_del_btn.connect_clicked(move |_| {
            ui.menu_delete();
            ui.fire_save();
        });
    }
    {
        let ui = ui.clone();
        menu_reset_btn.connect_clicked(move |_| {
            ui.menu_reset();
            ui.fire_save();
        });
    }
    {
        let ui = ui.clone();
        ui.menu_view
            .selection()
            .connect_changed(move |_| ui.menu_fill_picker());
    }

    page
}
