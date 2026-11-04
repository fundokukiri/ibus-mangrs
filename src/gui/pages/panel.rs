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
    menu_add_btn.add_css_class("suggested-action");
    let row1 = gtk4::Box::new(Orientation::Horizontal, 8);
    row1.set_margin_start(10);
    row1.set_margin_end(10);
    row1.set_margin_top(8);
    row1.set_margin_bottom(4);
    row1.append(&ui.menu_kind);
    row1.append(&menu_add_btn);
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
    // Rename lives on the tree itself, not a separate form: double-click or
    // press-and-hold the selected root folder. The tooltip makes the
    // gesture discoverable — there is no visible button for it.
    ui.menu_view.set_tooltip_text(Some(
        "Nhấp đôi hoặc giữ chuột vào thư mục (cấp đầu) để đổi tên",
    ));
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
            if kind == "folder" {
                ui.menu_add_folder("");
            } else {
                ui.menu_insert(kind, key);
            }
            ui.fire_save();
        });
    }
    {
        let ui = ui.clone();
        let view = ui.menu_view.clone();
        let view_c = view.clone();
        view.connect_row_activated(move |_, path, _| {
            if path.depth() == 1 {
                open_folder_rename(&ui, &view_c);
            }
        });
    }
    {
        let ui = ui.clone();
        let view = ui.menu_view.clone();
        let view_c = view.clone();
        let long = gtk4::GestureLongPress::new();
        long.connect_pressed(move |_, _, _| open_folder_rename(&ui, &view_c));
        view.add_controller(long);
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

/// Rename the selected root folder through a small dialog. Only root-level
/// folders are renamable: entries inside keep their built-in labels, and
/// children of folders already sit where their parent design put them.
#[allow(deprecated)] // TreeView, see the mod.rs import note
fn open_folder_rename(ui: &Rc<Ui>, view: &TreeView) {
    let Some((iter, depth)) = ui.menu_selected() else {
        return;
    };
    if depth != 0 || ui.menu_value(&iter, MENU_COL_KIND) != "folder" {
        return;
    }
    let entry = gtk4::Entry::builder()
        .text(ui.menu_value(&iter, MENU_COL_LABEL).as_str())
        .activates_default(true)
        .build();
    let dialog = adw::AlertDialog::builder()
        .heading("Đổi tên thư mục")
        .body("Nhập tên mới cho thư mục đang chọn")
        .extra_child(&entry)
        .build();
    dialog.add_response("cancel", "Hủy");
    dialog.add_response("apply", "Đổi tên");
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("apply"));
    dialog.set_close_response("cancel");
    {
        let ui = ui.clone();
        let entry = entry.clone();
        dialog.connect_response(Some("apply"), move |_, _| {
            ui.menu_rename(entry.text().trim());
            ui.fire_save();
        });
    }
    dialog.present(view.root().and_downcast::<gtk4::Window>().as_ref());
}
