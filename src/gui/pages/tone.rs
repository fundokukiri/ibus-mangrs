//! Page "Dấu & tự động": tone style, spelling recovery, automatic
//! capitalization and the special-character shortcuts.

use super::*;

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // Row titles match the quick-menu registry labels (ibus_prop
    // ::QUICK_TOGGLES): a toggle ticked in the panel menu must be findable
    // under the same name here.
    let tones = adw::PreferencesGroup::builder()
        .title("Dấu thanh")
        .description("Kiểu đặt dấu trên nguyên âm")
        .build();
    tones.add(&ui.modern);
    tones.add(&ui.free);
    page.add(&tones);

    let spelling = adw::PreferencesGroup::builder()
        .title("Chính tả")
        .description("Sửa lỗi và khôi phục từ")
        .build();
    spelling.add(&ui.auto_correct);
    spelling.add(&ui.auto_restore);
    spelling.add(&ui.bypass_url);
    spelling.add(&ui.abbrev);
    page.add(&spelling);

    let automatic = adw::PreferencesGroup::builder()
        .title("Tự động")
        .description("Viết hoa và dấu câu tự động")
        .build();
    automatic.add(&ui.auto_capitalize);
    automatic.add(&ui.double_space);
    page.add(&automatic);

    let special = adw::PreferencesGroup::builder()
        .title("Ký tự đặc biệt")
        .description("Chuyển đổi phím tắt ký tự")
        .build();
    special.add(&ui.w2u);
    special.add(&ui.bracket);
    special.add(&ui.em_dash);
    page.add(&special);

    page
}
