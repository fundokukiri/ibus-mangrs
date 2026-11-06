//! The App rules editor: rows rebuilt from the design vector, reorder
//! and delete plumbing, and the live rule-match preview.

use super::*;

impl Ui {
    /// The rules as the rows currently show them; row order is match
    /// priority. An empty pattern is kept (an inert rule the user can
    /// delete), so `refresh`'s round-trip assert holds for hand-edited
    /// configs too.
    pub(super) fn rules_from_rows(&self) -> Vec<crate::config::AppRule> {
        let mut out = Vec::new();
        let mut child = self.rules_box.first_child();
        while let Some(row) = child {
            child = row.next_sibling();
            if let Some(combo) = row.downcast_ref::<adw::ComboRow>() {
                out.push(crate::config::AppRule {
                    pattern: combo.title().trim().to_string(),
                    mode: mode_at(combo.selected()),
                });
            }
        }
        out
    }

    /// Apply one user edit to the rule list: rebuild the rows, then save
    /// through the funnel like every other control.
    pub(super) fn edit_rules(self: &Rc<Self>, edit: impl FnOnce(&mut Vec<crate::config::AppRule>)) {
        let mut rules = self.rules_from_rows();
        edit(&mut rules);
        self.fill_rules(&rules);
        self.fire_save();
    }

    /// Refresh the focused-app detection for the merged add-rule group: the
    /// hint lands in the entry's placeholder, copy/apply turn on, and the
    /// pill tooltip carries the window title (or the detection error). The
    /// D-Bus call is sync with a 300 ms timeout on the main thread — GTK
    /// types are not Send — but the caller is the window's focus-leave, a
    /// moment when the window is idle anyway; with the extension installed
    /// the call is near-instant.
    pub(crate) fn refresh_rule_focus(self: &Rc<Self>) {
        let tooltip = match focused_app_name() {
            Ok((class, title)) => {
                *self.rule_focus_value.borrow_mut() = class;
                Some(title)
            }
            Err(e) => {
                self.rule_focus_value.borrow_mut().clear();
                Some(e)
            }
        };
        let hint = self.rule_focus_value.borrow().clone();
        let placeholder = if hint.is_empty() {
            t(
                "rows.rule_name.placeholder",
                "vd: firefox, org.gnome.Console",
            )
        } else {
            hint
        };
        self.rule_name_entry
            .set_placeholder_text(Some(&placeholder));
        self.rule_pill.set_tooltip_text(tooltip.as_deref());
        self.update_rule_add_state();
    }

    /// The add-rule form state: a typed name or the focused-app hint alone
    /// enables Thêm (the hint is a valid pattern), and copy/apply only make
    /// sense with an app actually detected.
    pub(super) fn update_rule_add_state(self: &Rc<Self>) {
        let typed = !self.rule_name_entry.text().trim().is_empty();
        let hint = !self.rule_focus_value.borrow().is_empty();
        self.rule_add_btn.set_sensitive(typed || hint);
        self.rule_copy_btn.set_visible(hint);
        self.rule_apply_btn.set_visible(hint);
    }

    /// Rebuild the rule rows from `rules`. Also runs at refresh — that path
    /// must not save; only user edits do (`edit_rules`).
    pub(super) fn fill_rules(self: &Rc<Self>, rules: &[crate::config::AppRule]) {
        *self.rules_loading.borrow_mut() = true;
        while let Some(child) = self.rules_box.first_child() {
            self.rules_box.remove(&child);
        }
        let total = rules.len();
        for (idx, rule) in rules.iter().enumerate() {
            self.rule_row(idx, total, &rule.pattern, rule.mode);
        }
        self.rules_box.set_visible(total > 0);
        self.rules_status.set_visible(total == 0);
        *self.rules_loading.borrow_mut() = false;
        self.update_rule_preview();
    }

    /// One rule as a native ComboRow: the app id is the title, the forced
    /// mode the subtitle; the suffix holds the reorder pair and the trash.
    /// Row order is match priority — the engine takes the first rule that
    /// hits.
    fn rule_row(self: &Rc<Self>, idx: usize, total: usize, pattern: &str, mode: ModeConfig) {
        // Markup is off before the title is set (the builder applies its
        // stored properties in declaration order, not call order): the
        // title is a user-typed app id, and a bare `&` in it would warn
        // (and blank) as Pango markup.
        let row = adw::ComboRow::new();
        row.set_use_markup(false);
        row.set_title(pattern);
        row.set_use_subtitle(true);
        row.set_model(Some(&StringList::new(MODES)));
        row.set_selected(mode_index(mode));
        // The theme ellipsize is END; a long id would hide its tail, and
        // the title label is not a public child — hence the helper.
        if let Some(title) = row_title(&row) {
            title.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        }
        row.set_tooltip_text(Some(pattern));
        let num = gtk4::Label::new(Some(&(idx + 1).to_string()));
        num.add_css_class("dim-label");
        num.add_css_class("caption");
        num.set_valign(gtk4::Align::Center);
        row.add_prefix(&num);
        {
            let ui = self.clone();
            row.connect_selected_notify(move |_| {
                if *ui.rules_loading.borrow() {
                    return;
                }
                ui.fire_save();
                ui.update_rule_preview();
            });
        }
        // Reorder as a linked pair; delete as a flat trash that only turns
        // red under the pointer ("error" is a permanent tint otherwise).
        let up = gtk4::Button::from_icon_name("go-up-symbolic");
        up.add_css_class("flat");
        up.set_sensitive(idx > 0);
        up.set_tooltip_text(Some(&t("rows.rule_up.tooltip", "Lên — luật trước thắng")));
        {
            let ui = self.clone();
            up.connect_clicked(move |_| {
                ui.edit_rules(|rules| {
                    if idx > 0 {
                        rules.swap(idx - 1, idx);
                    }
                });
            });
        }
        let down = gtk4::Button::from_icon_name("go-down-symbolic");
        down.add_css_class("flat");
        down.set_sensitive(idx + 1 < total);
        down.set_tooltip_text(Some(&t("rows.rule_down.tooltip", "Xuống")));
        {
            let ui = self.clone();
            down.connect_clicked(move |_| {
                ui.edit_rules(|rules| {
                    if idx + 1 < rules.len() {
                        rules.swap(idx, idx + 1);
                    }
                });
            });
        }
        let linked = gtk4::Box::new(Orientation::Horizontal, 0);
        linked.add_css_class("linked");
        linked.append(&up);
        linked.append(&down);
        let del = gtk4::Button::from_icon_name("user-trash-symbolic");
        del.add_css_class("flat");
        del.set_tooltip_text(Some(&t("rows.rule_delete.tooltip", "Xoá luật này")));
        {
            let ui = self.clone();
            del.connect_clicked(move |_| {
                ui.edit_rules(|rules| {
                    if idx < rules.len() {
                        rules.remove(idx);
                    }
                });
            });
        }
        {
            let del_enter = del.clone();
            let del_leave = del.clone();
            let hover = gtk4::EventControllerMotion::new();
            hover.connect_enter(move |_, _, _| del_enter.add_css_class("error"));
            hover.connect_leave(move |_| del_leave.remove_css_class("error"));
            del.add_controller(hover);
        }
        row.add_suffix(&linked);
        row.add_suffix(&del);
        self.rules_box.append(&row);
    }

    /// The "thử luật" verdict as the result row's subtitle: which rule hits
    /// the typed name and the mode it forces. Rules only — the engine's
    /// capability fallbacks are not modelled here.
    pub(super) fn update_rule_preview(&self) {
        let input = self.rule_preview_input.text().trim().to_string();
        let verdict = if input.is_empty() {
            String::new()
        } else {
            let rules = self.rules_from_rows();
            let target = Some(input.as_str());
            match crate::config::app_rule_mode(&rules, target) {
                Some(mode) => {
                    let idx = rules
                        .iter()
                        .position(|r| crate::config::app_rule_matches(&r.pattern, target))
                        .map(|i| i + 1)
                        .unwrap_or(0);
                    format!("Khớp luật #{idx} · {}", MODES[mode_index(mode) as usize])
                }
                None => t("rules.no_match", "Không khớp luật nào").to_string(),
            }
        };
        self.rule_preview.set_subtitle(&verdict);
    }
}
