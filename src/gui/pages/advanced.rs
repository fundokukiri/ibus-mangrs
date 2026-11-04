//! Page "Tinh chỉnh": the set-once technical settings — the panel-menu
//! editor (a TreeView editor, kept deliberately: a ListView+TreeListModel
//! rewrite would triple the code for no user gain), the uinput pacing, the
//! X11/XIM workarounds and the debug-log switch.

use super::*;

#[allow(deprecated)] // TreeView/ComboBoxText, see the mod.rs import note
pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // The panel popup is the user's own design: folders (one level deep),
    // toggles, radio groups, actions and separators, freely ordered.
    let menu = adw::PreferencesGroup::builder()
        .title("Menu trên panel")
        .description("Tự thiết kế menu popup: thêm thư mục, thêm chức năng, bỏ vào thư mục, sắp xếp thứ tự. Bảng mã / Kiểu gõ / Chế độ gõ cũng chỉnh được mục bên trong.")
        .build();
    let menu_add_btn = gtk4::Button::with_label("Thêm");
    let menu_folder_btn = gtk4::Button::with_label("Thêm thư mục");
    let menu_rename_btn = gtk4::Button::with_label("Đổi tên");
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
    let menu_up_btn = gtk4::Button::with_label("Lên");
    let menu_down_btn = gtk4::Button::with_label("Xuống");
    let menu_out_btn = gtk4::Button::with_label("Ra ngoài");
    let menu_del_btn = gtk4::Button::with_label("Xoá");
    let menu_reset_btn = gtk4::Button::with_label("Đặt lại mặc định");
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
    menu_scroll.set_min_content_height(220);
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

    // The uinput pacing applies to Smooth / Super Smooth / Select only; the
    // spin rows save on every value change like every other control.
    let pacing = adw::PreferencesGroup::builder()
        .title("Tốc độ gõ (uinput)")
        .description("Chỉ áp dụng cho Smooth / Super Smooth / Select")
        .build();
    pacing.add(&ui.commit_interval);
    pacing.add(&ui.backspace_interval);
    pacing.add(&ui.post_delay);
    pacing.add(&ui.unicode_channel);
    page.add(&pacing);
    for row in [&ui.commit_interval, &ui.backspace_interval, &ui.post_delay] {
        let s = ui.save_cb.clone();
        row.connect_value_notify(move |_| {
            if let Some(save) = s.borrow().as_ref() {
                save();
            }
        });
    }

    let xim = adw::PreferencesGroup::builder()
        .title("Tương thích X11/XIM")
        .description("Cho app X11 trực tiếp và app XIM (Java/JetBrains)")
        .build();
    xim.add(&ui.x11_delete);
    xim.add(&ui.preedit_xim);
    page.add(&xim);

    let diagnostics = adw::PreferencesGroup::builder()
        .title("Chẩn đoán")
        .description("Ghi lại quyết định của engine để gỡ lỗi")
        .build();
    diagnostics.add(&ui.debug_log);
    page.add(&diagnostics);

    page
}
