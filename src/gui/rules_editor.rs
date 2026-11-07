//! The App rules editor: rows rebuilt from the design vector, reorder and
//! delete plumbing (⋮ menu and drag-and-drop), the empty state, and the
//! live rule-match preview driven by the engine's own matching functions.

use super::*;

impl Ui {
    /// The rules as the rows currently show them; row order is match
    /// priority. An empty pattern is kept (an inert rule the user can
    /// delete), so `refresh`'s round-trip assert holds for hand-edited
    /// configs too. The empty-state row is a plain ListBoxRow and skips.
    pub(super) fn rules_from_rows(&self) -> Vec<crate::config::AppRule> {
        let mut out = Vec::new();
        let mut child = self.rules_box.first_child();
        while let Some(row) = child {
            child = row.next_sibling();
            let Some(row) = row.downcast_ref::<adw::ActionRow>() else {
                continue;
            };
            out.push(crate::config::AppRule {
                pattern: row.title().trim().to_string(),
                // The picker is always present; the default only keeps a
                // hand-broken row from vanishing on save.
                mode: row_mode(row).unwrap_or(ModeConfig::Preedit),
            });
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
        if rules.is_empty() {
            // One dim, centered row instead of a StatusPage: the empty list
            // keeps the same card shape as the filled one.
            let row = gtk4::ListBoxRow::new();
            row.set_activatable(false);
            let label = gtk4::Label::new(Some(&t("rules.empty", "Chưa có luật nào")));
            label.add_css_class("dim-label");
            label.set_halign(gtk4::Align::Center);
            label.set_margin_top(14);
            label.set_margin_bottom(14);
            row.set_child(Some(&label));
            self.rules_box.append(&row);
        }
        let total = rules.len();
        for (idx, rule) in rules.iter().enumerate() {
            self.rule_row(idx, total, &rule.pattern, rule.mode);
        }
        *self.rules_loading.borrow_mut() = false;
        self.update_rule_preview();
    }

    /// One rule as a one-line row: [drag handle] [priority number] [app id]
    /// [mode picker] [⋮ menu]. Row order is match priority — the engine
    /// takes the first rule that hits; renumbering happens by rebuilding.
    fn rule_row(self: &Rc<Self>, idx: usize, total: usize, pattern: &str, mode: ModeConfig) {
        // Markup is off before the title is set (the builder applies its
        // stored properties in declaration order, not call order): the
        // title is a user-typed app id, and a bare `&` in it would warn
        // (and blank) as Pango markup.
        let row = adw::ActionRow::new();
        row.set_use_markup(false);
        row.set_title(pattern);
        row.set_activatable(false);
        // The theme ellipsize is END; a long id would hide its tail, and
        // the title label is not a public child — hence the helper.
        if let Some(title) = row_title(&row) {
            title.set_ellipsize(gtk4::pango::EllipsizeMode::Middle);
        }
        row.set_tooltip_text(Some(pattern));
        row.update_property(&[gtk4::accessible::Property::Description(&format!(
            "{}, {}, {}",
            idx + 1,
            pattern,
            MODES[mode_index(mode) as usize]
        ))]);

        // The drag handle starts a row move; the ghost is the row itself.
        let handle = gtk4::Image::from_icon_name("list-drag-handle-symbolic");
        handle.add_css_class("dim-label");
        handle.set_valign(gtk4::Align::Center);
        handle.set_tooltip_text(Some(&t(
            "rows.rule_handle.tooltip",
            "Kéo để đổi thứ tự luật",
        )));
        handle.update_property(&[gtk4::accessible::Property::Label(&t(
            "rows.rule_handle.tooltip",
            "Kéo để đổi thứ tự luật",
        ))]);
        if let Some(cursor) = gtk4::gdk::Cursor::from_name("grab", None) {
            handle.set_cursor(Some(&cursor));
        }
        {
            let src = gtk4::DragSource::new();
            src.set_actions(gtk4::gdk::DragAction::MOVE);
            src.set_content(Some(&gtk4::gdk::ContentProvider::for_value(
                &idx.to_string().to_value(),
            )));
            let ghost_row = row.clone();
            src.connect_drag_begin(move |_, drag| {
                // The ghost renders the live row (WidgetPaintable in a
                // DragIcon child); the original then fades, so the two
                // never blur together.
                let icon = gtk4::DragIcon::for_drag(drag);
                let ghost =
                    gtk4::Picture::for_paintable(&gtk4::WidgetPaintable::new(Some(&ghost_row)));
                icon.set_child(Some(&ghost));
                ghost_row.add_css_class("rule-dragging");
            });
            let ghost_row = row.clone();
            src.connect_drag_end(move |_, _, _| {
                ghost_row.remove_css_class("rule-dragging");
                ghost_row.remove_css_class("rule-drop-above");
                ghost_row.remove_css_class("rule-drop-below");
            });
            handle.add_controller(src);
        }

        // The priority number: one fixed-width column so every row's title
        // lines up (the old per-row width made row 1 drift).
        let num = gtk4::Label::new(Some(&(idx + 1).to_string()));
        num.add_css_class("dim-label");
        num.add_css_class("caption");
        num.set_width_chars(3);
        num.set_xalign(0.5);
        num.set_valign(gtk4::Align::Center);
        row.add_prefix(&handle);
        row.add_prefix(&num);

        // The mode picker: a flat dropdown reading as dim row text; the
        // popup list carries the modes with the current one checked, and a
        // change saves immediately.
        let pick = gtk4::DropDown::new(Some(StringList::new(MODES)), gtk4::Expression::NONE);
        pick.set_selected(mode_index(mode));
        pick.add_css_class("flat");
        pick.add_css_class("dim-label");
        pick.add_css_class("rule-mode");
        pick.set_valign(gtk4::Align::Center);
        if let Some(label) = first_label(&pick) {
            label.set_ellipsize(gtk4::pango::EllipsizeMode::End);
        }
        {
            let ui = self.clone();
            pick.connect_selected_notify(move |_| {
                if *ui.rules_loading.borrow() {
                    return;
                }
                ui.fire_save();
                ui.update_rule_preview();
            });
        }

        // The ⋮ menu: move up/down (disabled at the ends) and delete with
        // its undo toast. A manual popover — the menu model API cannot
        // style one item destructive.
        let more = gtk4::MenuButton::new();
        more.set_icon_name("view-more-symbolic");
        more.add_css_class("flat");
        more.add_css_class("circular");
        more.set_valign(gtk4::Align::Center);
        more.set_tooltip_text(Some(&t("rows.rule_menu.tooltip", "Thao tác với luật này")));
        more.update_property(&[gtk4::accessible::Property::Label(&t(
            "rows.rule_menu.tooltip",
            "Thao tác với luật này",
        ))]);
        let pop = gtk4::Popover::new();
        let menu = gtk4::Box::new(Orientation::Vertical, 0);
        let up_btn = gtk4::Button::with_label(&t("rows.rule_menu.up", "Lên"));
        up_btn.add_css_class("flat");
        up_btn.set_sensitive(idx > 0);
        {
            let ui = self.clone();
            let pop = pop.clone();
            up_btn.connect_clicked(move |_| {
                pop.popdown();
                ui.edit_rules(|rules| {
                    if idx > 0 {
                        rules.swap(idx - 1, idx);
                    }
                });
            });
        }
        menu.append(&up_btn);
        let down_btn = gtk4::Button::with_label(&t("rows.rule_menu.down", "Xuống"));
        down_btn.add_css_class("flat");
        down_btn.set_sensitive(idx + 1 < total);
        {
            let ui = self.clone();
            let pop = pop.clone();
            down_btn.connect_clicked(move |_| {
                pop.popdown();
                ui.edit_rules(|rules| {
                    if idx + 1 < rules.len() {
                        rules.swap(idx, idx + 1);
                    }
                });
            });
        }
        menu.append(&down_btn);
        let del_btn = gtk4::Button::with_label(&t("rows.rule_menu.delete", "Xóa"));
        del_btn.add_css_class("flat");
        del_btn.add_css_class("destructive-action");
        {
            let ui = self.clone();
            let pop = pop.clone();
            let rule = crate::config::AppRule {
                pattern: pattern.to_string(),
                mode,
            };
            del_btn.connect_clicked(move |_| {
                pop.popdown();
                ui.edit_rules(|rules| {
                    if idx < rules.len() {
                        rules.remove(idx);
                    }
                });
                // Undo puts the rule back at its old slot while the toast
                // lives, like the custom shortcuts' trash; toasts silenced,
                // the delete just stands.
                if ui.toasts_on() {
                    let toast = adw::Toast::new(&t("rules.deleted_toast", "Đã xóa luật"));
                    toast.set_button_label(Some(&t("misc.undo", "Hoàn tác")));
                    {
                        let ui = ui.clone();
                        let rule = rule.clone();
                        toast.connect_button_clicked(move |_| {
                            ui.edit_rules(|rules| {
                                let at = idx.min(rules.len());
                                rules.insert(at, rule.clone());
                            });
                        });
                    }
                    ui.rules_overlay.add_toast(toast);
                }
            });
        }
        menu.append(&del_btn);
        pop.set_child(Some(&menu));
        more.set_popover(Some(&pop));

        row.add_suffix(&pick);
        row.add_suffix(&more);

        // Dropping on a row lands the dragged rule before it (top half) or
        // after it (bottom half); the accent bar marks the landing edge.
        let tgt = gtk4::DropTarget::new(gtk4::glib::Type::STRING, gtk4::gdk::DragAction::MOVE);
        {
            let mark_row = row.clone();
            let mark = move |tgt: &gtk4::DropTarget, y: f64| {
                // Detached targets can briefly have no widget; without a
                // height there is nothing to mark anyway.
                let Some(widget) = tgt.widget() else {
                    return;
                };
                let below = y > widget.height().max(1) as f64 / 2.0;
                mark_row.remove_css_class("rule-drop-above");
                mark_row.remove_css_class("rule-drop-below");
                mark_row.add_css_class(if below {
                    "rule-drop-below"
                } else {
                    "rule-drop-above"
                });
            };
            let mark_enter = mark.clone();
            tgt.connect_enter(move |tgt, _, y| {
                mark_enter(tgt, y);
                gtk4::gdk::DragAction::MOVE
            });
            let mark_motion = mark.clone();
            tgt.connect_motion(move |tgt, _, y| {
                mark_motion(tgt, y);
                gtk4::gdk::DragAction::MOVE
            });
            let row_leave = row.clone();
            tgt.connect_leave(move |_| {
                row_leave.remove_css_class("rule-drop-above");
                row_leave.remove_css_class("rule-drop-below");
            });
        }
        {
            let ui = self.clone();
            let row = row.clone();
            tgt.connect_drop(move |tgt, value, _, y| {
                row.remove_css_class("rule-drop-above");
                row.remove_css_class("rule-drop-below");
                let Some(from) = value
                    .get::<String>()
                    .ok()
                    .and_then(|s| s.parse::<usize>().ok())
                else {
                    // A foreign string drag, not one of our handles.
                    return false;
                };
                let to = match tgt.widget() {
                    Some(widget) if y > widget.height().max(1) as f64 / 2.0 => idx + 1,
                    _ => idx,
                };
                if from != to {
                    ui.edit_rules(|rules| {
                        if from < rules.len() {
                            let rule = rules.remove(from);
                            let mut at = to;
                            if from < at {
                                at -= 1;
                            }
                            rules.insert(at.min(rules.len()), rule);
                        }
                    });
                }
                true
            });
        }
        row.add_controller(tgt);

        self.rules_box.append(&row);
    }

    /// The "thử luật" verdict as the result row's status icon and subtitle:
    /// which rule hits the typed name and the mode it forces. Rules only —
    /// the engine's capability fallbacks are not modelled here; matching is
    /// `config::app_rule_matches`, the engine's own function.
    pub(super) fn update_rule_preview(&self) {
        let input = self.rule_preview_input.text().trim().to_string();
        let icon = &self.rule_preview_icon;
        let set_state = |icon_name: &str, success: bool| {
            icon.set_icon_name(Some(icon_name));
            icon.remove_css_class("dim-label");
            icon.remove_css_class("success");
            if success {
                icon.add_css_class("success");
            } else {
                icon.add_css_class("dim-label");
            }
        };
        if input.is_empty() {
            set_state("system-search-symbolic", false);
            self.rule_preview.set_subtitle(&t(
                "rules.preview_empty",
                "Nhập tên app ở trên để xem kết quả",
            ));
            return;
        }
        let rules = self.rules_from_rows();
        let target = Some(input.as_str());
        match crate::config::app_rule_mode(&rules, target) {
            Some(mode) => {
                let idx = rules
                    .iter()
                    .position(|r| crate::config::app_rule_matches(&r.pattern, target))
                    .map(|i| i + 1)
                    .unwrap_or(0);
                set_state("emblem-ok-symbolic", true);
                self.rule_preview.set_subtitle(
                    &t("rules.match_result", "Khớp luật #{n}: {mode}")
                        .replace("{n}", &idx.to_string())
                        .replace("{mode}", MODES[mode_index(mode) as usize]),
                );
            }
            None => {
                set_state("list-remove-symbolic", false);
                self.rule_preview
                    .set_subtitle(&t("rules.no_match", "Không khớp luật nào"));
            }
        }
    }
}

/// The row's mode picker: the only DropDown inside the row. GObject clones
/// share the object, so reading `selected` reads what the user sees.
fn row_mode(row: &adw::ActionRow) -> Option<ModeConfig> {
    fn walk(w: &gtk4::Widget) -> Option<gtk4::DropDown> {
        if let Some(d) = w.downcast_ref::<gtk4::DropDown>() {
            return Some(d.clone());
        }
        let mut child = w.first_child();
        while let Some(c) = child {
            if let Some(d) = walk(&c) {
                return Some(d);
            }
            child = c.next_sibling();
        }
        None
    }
    walk(row.upcast_ref()).map(|d| mode_at(d.selected()))
}
