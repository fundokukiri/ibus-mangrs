//! Page "Dấu & tự động": tone style, spelling recovery, automatic
//! capitalization and the special-character shortcuts. The rows come from
//! `build`; this page only groups them and decorates the experimental
//! double-space row with a caption badge before its switch.

use super::*;

/// The experimental row carries a small "Thử nghiệm" badge left of the
/// switch. AdwSwitchRow appends its own switch as the first suffix, so the
/// badge is prepended to the (internal) suffixes box; the switch itself
/// also gets a spoken name mentioning the trial status. The internal boxes
/// are reached through the tree — the same private-child walk as
/// `row_title` — because Adwaita exposes no insert-before-suffix API.
fn decorate_experimental(row: &adw::SwitchRow) {
    fn find(w: &gtk4::Widget) -> (Option<gtk4::Box>, Option<gtk4::Switch>) {
        let mut box_out = None;
        let mut switch_out = None;
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
        walk(w, &mut box_out, &mut switch_out);
        (box_out, switch_out)
    }
    let badge = gtk4::Label::new(Some(&t("rows.double_space.badge", "Thử nghiệm")));
    badge.add_css_class("caption");
    badge.add_css_class("warning");
    badge.set_valign(gtk4::Align::Center);
    let (suffixes, switch) = find(row.upcast_ref());
    if let Some(suffixes) = suffixes {
        // First suffix = left of the row's own switch.
        suffixes.prepend(&badge);
    }
    if let Some(switch) = switch {
        switch.update_property(&[gtk4::accessible::Property::Label(&t(
            "rows.double_space.a11y",
            "Nhấn Space hai lần thành dấu chấm, thử nghiệm",
        ))]);
    }
}

pub(crate) fn build(ui: &Rc<Ui>) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();

    // Row titles match the quick-menu registry labels (ibus_prop
    // ::QUICK_TOGGLES): a toggle ticked in the panel menu must be findable
    // under the same name here.
    let tones = adw::PreferencesGroup::builder()
        .title(t("groups.tone.tones.title", "Dấu thanh"))
        .description(t(
            "groups.tone.tones.description",
            "Kiểu đặt dấu trên nguyên âm",
        ))
        .build();
    tones.add(&ui.modern);
    tones.add(&ui.free);
    page.add(&tones);

    let spelling = adw::PreferencesGroup::builder()
        .title(t("groups.tone.spelling.title", "Chính tả"))
        .description(t(
            "groups.tone.spelling.description",
            "Sửa lỗi và khôi phục từ",
        ))
        .build();
    spelling.add(&ui.auto_correct);
    spelling.add(&ui.auto_restore);
    spelling.add(&ui.bypass_url);
    spelling.add(&ui.abbrev);
    page.add(&spelling);

    let automatic = adw::PreferencesGroup::builder()
        .title(t("groups.tone.auto.title", "Tự động"))
        .description(t(
            "groups.tone.auto.description",
            "Viết hoa và dấu câu tự động",
        ))
        .build();
    automatic.add(&ui.auto_capitalize);
    automatic.add(&ui.double_space);
    decorate_experimental(&ui.double_space);
    page.add(&automatic);

    let special = adw::PreferencesGroup::builder()
        .title(t("groups.tone.special.title", "Ký tự đặc biệt"))
        .description(t(
            "groups.tone.special.description",
            "Chuyển đổi phím tắt ký tự",
        ))
        .build();
    special.add(&ui.w2u);
    special.add(&ui.bracket);
    special.add(&ui.em_dash);
    page.add(&special);

    page
}
