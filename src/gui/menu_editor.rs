//! Panel-menu label helpers, the design-vector path utilities and the
//! whole menu editor: row widgets, drag/reorder, add/remove/rename.

use super::*;

/// (kind id, key, display label, Vietnamese kind label) for one node.
fn menu_row(node: &MenuNode) -> (String, String, String, String) {
    match node {
        MenuNode::Toggle { key } => (
            "toggle".into(),
            key.clone(),
            menu_toggle_label(key),
            t("menu.kind_toggle", "Bật/tắt"),
        ),
        MenuNode::Radios { group, .. } => (
            "radios".into(),
            group.clone(),
            menu_radios_label(group),
            t("menu.kind_radios", "Nhóm chọn"),
        ),
        MenuNode::Action { key } => (
            "action".into(),
            key.clone(),
            menu_action_label(key),
            t("menu.kind_action", "Chức năng"),
        ),
        MenuNode::Separator => (
            "sep".into(),
            String::new(),
            "— phân cách —".into(),
            t("menu.kind_sep", "Gạch ngang"),
        ),
        MenuNode::Folder { label, .. } => (
            "folder".into(),
            String::new(),
            label.clone(),
            t("menu.kind_folder", "Thư mục"),
        ),
    }
}

fn menu_toggle_label(key: &str) -> String {
    if key == "enabled" {
        return t("menu.toggle_enabled", "Bật tiếng Việt");
    }
    crate::ibus_prop::quick_toggle(key)
        .map(|tg| tg.label.to_string())
        .unwrap_or_else(|| key.to_string())
}

fn menu_radios_label(group: &str) -> String {
    match group {
        "charset" => t("menu.label_charset", "Bảng mã"),
        "method" => t("menu.label_method", "Kiểu gõ"),
        "display" => t("menu.label_display", "Chế độ gõ"),
        other => other.into(),
    }
}

fn menu_action_label(key: &str) -> String {
    match key {
        "macro_editor" => t("menu.action_macro_editor", "Mở bảng gõ tắt…"),
        "settings" => t("menu.action_settings", "Cấu hình"),
        "about" => t("menu.action_about", "Giới thiệu (About)"),
        other => other.into(),
    }
}

/// The display label of a radio-group entry ("vni" in charset → "VNI").
fn menu_entry_label(group: &str, key: &str) -> String {
    let entries: &[(&str, &str)] = match group {
        "charset" => crate::ibus_prop::CHARSETS,
        "method" => crate::ibus_prop::METHODS,
        "display" => crate::ibus_prop::MODES,
        _ => &[],
    };
    entries
        .iter()
        .find(|(k, _)| *k == key)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| key.to_string())
}

/// Where a picked menu item lands: root end, or inside one root container
/// (a folder takes generic items, a radio group only its own entries).
#[derive(Clone)]
pub(crate) enum MenuScope {
    Root,
    Container(Vec<usize>),
}

/// "0.2.1"-style key so a drag payload carries the row's design position.
fn menu_path_key(path: &[usize]) -> String {
    path.iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(".")
}

/// Inverse of `menu_path_key`.
fn menu_parse_key(key: &str) -> Option<Vec<usize>> {
    key.split('.')
        .map(|p| p.parse().ok())
        .collect::<Option<Vec<_>>>()
}

/// The type icon for a menu item kind; separators carry none.
fn menu_kind_icon(kind: &str) -> Option<&'static str> {
    match kind {
        "toggle" => Some("emblem-ok-symbolic"),
        "radios" | "entry" => Some("radio-checked-symbolic"),
        "action" => Some("system-run-symbolic"),
        "folder" => Some("folder-symbolic"),
        _ => None,
    }
}

/// The swap target for one reorder step, or None at the list ends.
fn menu_step(idx: usize, delta: isize, len: usize) -> Option<usize> {
    match delta.signum() {
        -1 => idx.checked_sub(1),
        _ => Some(idx + 1).filter(|&p| p < len),
    }
}

/// Rename a root folder through a small dialog. Only root-level folders
/// are renamable: entries inside keep their built-in labels, and children
/// of folders already sit where their parent design put them.
fn menu_rename_dialog(ui: &Rc<Ui>, path: &[usize], current: &str, parent: &gtk4::Widget) {
    let entry = Entry::builder()
        .text(current)
        .activates_default(true)
        .build();
    let dialog = adw::AlertDialog::builder()
        .heading(t("panel.rename_heading", "Đổi tên thư mục"))
        .body(t("panel.rename_body", "Nhập tên mới cho thư mục đang chọn"))
        .extra_child(&entry)
        .build();
    dialog.add_response("cancel", &t("panel.rename_cancel", "Hủy"));
    dialog.add_response("apply", &t("panel.rename_apply", "Đổi tên"));
    dialog.set_response_appearance("apply", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("apply"));
    dialog.set_close_response("cancel");
    {
        let ui = ui.clone();
        let path = path.to_vec();
        let entry = entry.clone();
        dialog.connect_response(Some("apply"), move |_, _| {
            if ui.menu_rename(&path, entry.text().trim()) {
                ui.fill_menu_list();
                ui.fire_save();
            }
        });
    }
    dialog.present(parent.root().and_downcast::<gtk4::Window>().as_ref());
}

/// Mirror of the old tree-store read: folders keep one level — a nested
/// folder or radio group survives as an empty node, which the engine
/// skips anyway — and root radio groups keep their entry keys.
pub(super) fn normalize_menu(nodes: &[MenuNode]) -> Vec<MenuNode> {
    nodes
        .iter()
        .map(|node| match node {
            MenuNode::Folder { label, children } => MenuNode::Folder {
                label: label.clone(),
                children: children
                    .iter()
                    .map(|child| match child {
                        MenuNode::Folder { label, .. } => MenuNode::Folder {
                            label: label.clone(),
                            children: Vec::new(),
                        },
                        MenuNode::Radios { group, .. } => MenuNode::Radios {
                            group: group.clone(),
                            children: Vec::new(),
                        },
                        other => other.clone(),
                    })
                    .collect(),
            },
            other => other.clone(),
        })
        .collect()
}

impl Ui {
    /// Expand/collapse one folder or radio group and re-render — the arrow
    /// state is derived from `menu_open`, so the rebuild keeps the other
    /// blocks as they are. Fired by the arrow button and the row itself.
    fn menu_toggle_expander(self: &Rc<Self>, path: Vec<usize>) {
        let now = !self.menu_open.borrow().contains(&path);
        if now {
            self.menu_open.borrow_mut().insert(path.clone());
        } else {
            self.menu_open.borrow_mut().remove(&path);
        }
        self.fill_menu_list();
    }

    /// Rebuild the row list from the design vector. Also runs after every
    /// edit; the caller saves through the funnel.
    pub(super) fn fill_menu_list(self: &Rc<Self>) {
        while let Some(child) = self.menu_box.first_child() {
            self.menu_box.remove(&child);
        }
        let nodes = self.menu.borrow().clone();
        for (idx, node) in nodes.iter().enumerate() {
            for widget in self.menu_row_widget(vec![idx], node, 0) {
                self.menu_box.append(&widget);
            }
        }
    }

    /// One editor item's widgets: a leaf row, or an expander header plus
    /// its child block (folders and radio groups).
    fn menu_row_widget(
        self: &Rc<Self>,
        path: Vec<usize>,
        node: &MenuNode,
        depth: usize,
    ) -> Vec<gtk4::Widget> {
        let (kind, _key, label, kind_label) = menu_row(node);
        match node {
            MenuNode::Folder { children, .. } if depth == 0 && !children.is_empty() => {
                let mut rows = Vec::new();
                for (i, child) in children.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(i);
                    rows.extend(self.menu_row_widget(child_path, child, depth + 1));
                }
                self.menu_expander_row(path, &kind, &label, &kind_label, rows)
            }
            MenuNode::Radios { group, children } if depth == 0 && !children.is_empty() => {
                let mut rows = Vec::new();
                for (i, key) in children.iter().enumerate() {
                    let mut child_path = path.clone();
                    child_path.push(i);
                    rows.push(self.menu_leaf_row(
                        child_path,
                        "entry",
                        &menu_entry_label(group, key),
                        &t("menu.kind_entry", "Mục chọn"),
                        false,
                    ));
                }
                self.menu_expander_row(path, &kind, &label, &kind_label, rows)
            }
            _ => vec![self.menu_leaf_row(path, &kind, &label, &kind_label, kind == "sep")],
        }
    }

    /// One leaf editor row (real leaves and radio-group entries share it):
    /// leading handle + type icon, the per-row menu, the same-parent drop.
    fn menu_leaf_row(
        self: &Rc<Self>,
        path: Vec<usize>,
        kind: &str,
        title: &str,
        subtitle: &str,
        dim_title: bool,
    ) -> gtk4::Widget {
        let row = adw::ActionRow::builder()
            .title(title)
            .subtitle(subtitle)
            .use_markup(false) // titles are user-typed labels; a bare & must show
            .activatable(false)
            .build();
        if dim_title {
            // A slim dim row: separators carry no title emphasis.
            if let Some(title) = row_title(&row) {
                title.add_css_class("dim-label");
            }
        }
        row.add_prefix(&self.menu_lead(path.clone(), kind));
        row.add_suffix(&self.menu_more_button(path.clone(), kind));
        self.menu_drop_target(&row, path);
        row.upcast::<gtk4::Widget>()
    }

    /// The leading box (handle, then type icon) — one widget, so leaf rows
    /// and the expander header place the handle at the same spot despite
    /// their differing add_prefix insert orders.
    fn menu_lead(&self, path: Vec<usize>, kind: &str) -> gtk4::Box {
        let lead = gtk4::Box::new(Orientation::Horizontal, 6);
        lead.append(&self.menu_drag_handle(&path));
        if let Some(name) = menu_kind_icon(kind) {
            let icon = gtk4::Image::from_icon_name(name);
            icon.add_css_class("dim-label");
            lead.append(&icon);
        }
        lead
    }

    /// A folder / radio group as two list rows: the header (our own arrow
    /// INSIDE — before the row menu, which stays last; AdwExpanderRow
    /// would always render its chevron after the menu) and the child
    /// block, collapsed by default, indented one step.
    fn menu_expander_row(
        self: &Rc<Self>,
        path: Vec<usize>,
        kind: &str,
        title: &str,
        subtitle: &str,
        children: Vec<gtk4::Widget>,
    ) -> Vec<gtk4::Widget> {
        // Activatable: clicking the row itself expands, like the arrow.
        let header = adw::ActionRow::builder()
            .title(title)
            .subtitle(subtitle)
            .use_markup(false) // same as menu_leaf_row: user-typed labels
            .build();
        header.add_prefix(&self.menu_lead(path.clone(), kind));
        let more = self.menu_more_button(path.clone(), kind);
        let expanded = self.menu_open.borrow().contains(&path);
        let arrow = gtk4::Button::from_icon_name(if expanded {
            "pan-down-symbolic"
        } else {
            "pan-end-symbolic"
        });
        arrow.add_css_class("flat");
        arrow.set_valign(gtk4::Align::Center);
        arrow.set_tooltip_text(Some(&t("panel.expand_tooltip", "Mở rộng / thu gọn")));
        // A plain child list: boxed-list styling would nest a card inside
        // the card, the plain list keeps the row look and its separators.
        let child_list = gtk4::ListBox::new();
        child_list.set_selection_mode(gtk4::SelectionMode::None);
        child_list.set_margin_start(12);
        for child in &children {
            child_list.append(child);
        }
        let children_row = gtk4::ListBoxRow::new();
        children_row.set_activatable(false);
        children_row.set_selectable(false);
        children_row.set_child(Some(&child_list));
        children_row.set_visible(expanded);
        // The arrow expands directly; the row body expands through its
        // own click gesture — the list's row-activated never fired for
        // these rows. The gesture ignores clicks that land on the row's
        // own buttons (arrow, row menu). The expanded set lives in the
        // Ui, so a rebuild after a move keeps blocks open.
        let toggle = {
            let ui = self.clone();
            let path = path.clone();
            move || ui.menu_toggle_expander(path.clone())
        };
        let toggle: Rc<dyn Fn()> = Rc::new(toggle);
        arrow.connect_clicked({
            let toggle = toggle.clone();
            move |_| toggle()
        });
        {
            let header_pick = header.clone();
            let toggle = toggle.clone();
            let gesture = gtk4::GestureClick::new();
            gesture.connect_released(move |_, _, x, y| {
                let Some(target) = header_pick.pick(x, y, gtk4::PickFlags::DEFAULT) else {
                    return;
                };
                let mut node = target;
                loop {
                    if node.downcast_ref::<gtk4::Button>().is_some()
                        || node.downcast_ref::<gtk4::MenuButton>().is_some()
                    {
                        return;
                    }
                    let Some(parent) = node.parent() else {
                        break;
                    };
                    node = parent;
                }
                toggle();
            });
            header.add_controller(gesture);
        }
        header.add_suffix(&arrow);
        header.add_suffix(&more);
        self.menu_drop_target(&header, path);
        vec![
            header.upcast::<gtk4::Widget>(),
            children_row.upcast::<gtk4::Widget>(),
        ]
    }

    /// Drop side of the drag handle: reorder same-parent siblings by the
    /// payload path (see `menu_drag`).
    fn menu_drop_target(self: &Rc<Self>, widget: &impl IsA<gtk4::Widget>, dst_path: Vec<usize>) {
        let drop =
            gtk4::DropTarget::new(gtk4::glib::types::Type::STRING, gtk4::gdk::DragAction::MOVE);
        {
            let ui = self.clone();
            drop.connect_drop(move |_, value, _, _| {
                let Ok(src_key) = value.get::<String>() else {
                    return false;
                };
                let Some(src_path) = menu_parse_key(&src_key) else {
                    return false;
                };
                let moved = ui.menu_drag(&src_path, &dst_path);
                if moved {
                    ui.fill_menu_list();
                    ui.fire_save();
                }
                moved
            });
        }
        widget.add_controller(drop);
    }

    /// The drag handle: a DragSource whose payload is the row's design
    /// path (see `menu_drag` for the receiving side).
    fn menu_drag_handle(&self, path: &[usize]) -> gtk4::Image {
        let handle = gtk4::Image::from_icon_name("list-drag-handle-symbolic");
        handle.add_css_class("dim-label");
        let payload = menu_path_key(path).to_value();
        let src = gtk4::DragSource::new();
        src.set_actions(gtk4::gdk::DragAction::MOVE);
        src.connect_prepare(move |_, _, _| Some(gtk4::gdk::ContentProvider::for_value(&payload)));
        handle.add_controller(src);
        handle
    }

    /// The per-row menu: reorder, out, rename (root folders), add-into
    /// (root containers) and delete. Every action re-renders and saves
    /// through the funnel — row state exists only in the design vector.
    fn menu_more_button(self: &Rc<Self>, path: Vec<usize>, kind: &str) -> gtk4::MenuButton {
        let more = gtk4::MenuButton::new();
        more.set_icon_name("view-more-symbolic");
        more.add_css_class("flat");
        more.set_tooltip_text(Some(&t("panel.row_menu_tooltip", "Thao tác với mục này")));
        let popover = gtk4::Popover::new();
        let list = gtk4::Box::new(Orientation::Vertical, 0);
        // Sensitivity: the sibling position decides up/down, the kind
        // decides rename and add-into, and only folder children can leave.
        let (pos, len, parent_kind) = {
            let menu = self.menu.borrow();
            let siblings_len = match path.len() {
                1 => menu.len(),
                2 => match menu.get(path[0]) {
                    Some(MenuNode::Folder { children, .. }) => children.len(),
                    Some(MenuNode::Radios { children, .. }) => children.len(),
                    _ => 0,
                },
                _ => 0,
            };
            let parent_kind = match path.len() {
                2 => menu.get(path[0]).map(|n| menu_row(n).0).unwrap_or_default(),
                _ => "root".to_string(),
            };
            (path.last().copied().unwrap_or(0), siblings_len, parent_kind)
        };
        let item = |list: &gtk4::Box, label: &str, css: &str| -> gtk4::Button {
            let btn = gtk4::Button::with_label(label);
            btn.add_css_class("flat");
            if !css.is_empty() {
                btn.add_css_class(css);
            }
            list.append(&btn);
            btn
        };
        let up = item(&list, &t("panel.move_up", "Lên"), "");
        up.set_sensitive(pos > 0);
        {
            let ui = self.clone();
            let p = path.clone();
            let popover = popover.clone();
            up.connect_clicked(move |_| {
                if ui.menu_move(&p, -1) {
                    ui.fill_menu_list();
                    ui.fire_save();
                }
                popover.popdown();
            });
        }
        let down = item(&list, &t("panel.move_down", "Xuống"), "");
        down.set_sensitive(pos + 1 < len);
        {
            let ui = self.clone();
            let p = path.clone();
            let popover = popover.clone();
            down.connect_clicked(move |_| {
                if ui.menu_move(&p, 1) {
                    ui.fill_menu_list();
                    ui.fire_save();
                }
                popover.popdown();
            });
        }
        if parent_kind == "folder" {
            let out = item(&list, &t("panel.move_out", "Đưa ra khỏi thư mục"), "");
            {
                let ui = self.clone();
                let p = path.clone();
                let popover = popover.clone();
                out.connect_clicked(move |_| {
                    if ui.menu_out(&p) {
                        ui.fill_menu_list();
                        ui.fire_save();
                    }
                    popover.popdown();
                });
            }
        }
        if kind == "folder" && path.len() == 1 {
            let rename = item(&list, &t("panel.rename_apply", "Đổi tên"), "");
            let current = self
                .menu
                .borrow()
                .get(path[0])
                .and_then(|n| match n {
                    MenuNode::Folder { label, .. } => Some(label.clone()),
                    _ => None,
                })
                .unwrap_or_default();
            {
                let ui = self.clone();
                let p = path.clone();
                let popover = popover.clone();
                rename.connect_clicked(move |btn| {
                    menu_rename_dialog(&ui, &p, &current, btn.upcast_ref());
                    popover.popdown();
                });
            }
        }
        if (kind == "folder" || kind == "radios") && path.len() == 1 {
            let add_here = gtk4::MenuButton::new();
            add_here.set_label(&t("panel.add_here", "Thêm vào đây…"));
            add_here.add_css_class("flat");
            let add_popover = gtk4::Popover::new();
            add_popover.set_child(Some(
                &self.menu_add_list(&MenuScope::Container(vec![path[0]]), &add_popover),
            ));
            add_here.set_popover(Some(&add_popover));
            list.append(&add_here);
        }
        let del = item(&list, &t("panel.item_delete", "Xóa"), "destructive-action");
        {
            let ui = self.clone();
            let p = path.clone();
            let popover = popover.clone();
            del.connect_clicked(move |_| {
                if ui.menu_delete(&p) {
                    ui.fill_menu_list();
                    ui.fire_save();
                }
                popover.popdown();
            });
        }
        popover.set_child(Some(&list));
        more.set_popover(Some(&popover));
        more
    }

    /// The addable-item list for `scope` (root, or one root container):
    /// everything generic for root/folder, a radio group's own entries for
    /// that group — the old type picker's rule. Buttons append, re-render,
    /// save and close `popover`.
    pub(super) fn menu_add_list(
        self: &Rc<Self>,
        scope: &MenuScope,
        popover: &gtk4::Popover,
    ) -> gtk4::Widget {
        let list = gtk4::Box::new(Orientation::Vertical, 0);
        for (id, label) in self.menu_addable_items(scope) {
            let btn = gtk4::Button::with_label(&label);
            btn.add_css_class("flat");
            {
                let ui = self.clone();
                let scope = scope.clone();
                let popover = popover.clone();
                btn.connect_clicked(move |_| {
                    let Some((kind, key)) = id.split_once(':') else {
                        return;
                    };
                    if ui.menu_add(kind, key, &scope) {
                        ui.fill_menu_list();
                        ui.fire_save();
                    }
                    popover.popdown();
                });
            }
            list.append(&btn);
        }
        let scroll = gtk4::ScrolledWindow::new();
        scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
        // Tall enough that the generic item list barely scrolls.
        scroll.set_max_content_height(520);
        scroll.set_propagate_natural_height(true);
        scroll.set_child(Some(&list));
        scroll.upcast::<gtk4::Widget>()
    }

    /// The items addable in `scope` (id "kind:key", display label).
    fn menu_addable_items(&self, scope: &MenuScope) -> Vec<(String, String)> {
        let mut out = Vec::new();
        if let MenuScope::Container(path) = scope {
            let group = self.menu.borrow().get(path[0]).and_then(|n| match n {
                MenuNode::Radios { group, .. } => Some(group.clone()),
                _ => None,
            });
            if let Some(group) = group {
                let entries: &[(&str, &str)] = match group.as_str() {
                    "charset" => crate::ibus_prop::CHARSETS,
                    "method" => crate::ibus_prop::METHODS,
                    "display" => crate::ibus_prop::MODES,
                    _ => &[],
                };
                for (key, label) in entries {
                    out.push((format!("entry:{key}"), label.to_string()));
                }
                return out;
            }
        }
        out.push((
            "toggle:enabled".into(),
            t("menu.toggle_enabled", "Bật tiếng Việt"),
        ));
        for quick in crate::ibus_prop::QUICK_TOGGLES {
            out.push((format!("toggle:{}", quick.key), quick.label.to_string()));
        }
        out.push((
            "radios:charset".into(),
            t("menu.radios_charset", "Bảng mã (nhóm chọn)"),
        ));
        out.push((
            "radios:method".into(),
            t("menu.radios_method", "Kiểu gõ (nhóm chọn)"),
        ));
        out.push((
            "radios:display".into(),
            t("menu.radios_display", "Chế độ gõ (nhóm chọn)"),
        ));
        out.push((
            "action:macro_editor".into(),
            t("menu.action_macro_editor", "Mở bảng gõ tắt…"),
        ));
        out.push((
            "action:settings".into(),
            t("menu.action_settings", "Cấu hình"),
        ));
        out.push((
            "action:about".into(),
            t("menu.action_about", "Giới thiệu (About)"),
        ));
        out.push(("sep:".into(), t("menu.sep", "Gạch ngang phân cách")));
        // Last, so the frequently used kinds keep their default (first)
        // spot like the old picker.
        out.push(("folder:".into(), t("menu.folder", "Thư mục mới")));
        out
    }

    /// Append the picked item to its scope (root end, or the container's
    /// end — the tree editor inserted at the container's end too). A radio
    /// group takes only its own entries.
    fn menu_add(&self, kind: &str, key: &str, scope: &MenuScope) -> bool {
        let node = match kind {
            "toggle" => MenuNode::Toggle {
                key: key.to_string(),
            },
            "radios" => MenuNode::Radios {
                group: key.to_string(),
                children: Vec::new(),
            },
            "action" => MenuNode::Action {
                key: key.to_string(),
            },
            "sep" => MenuNode::Separator,
            _ => MenuNode::Folder {
                label: t("menu.folder", "Thư mục mới"),
                children: Vec::new(),
            },
        };
        let mut menu = self.menu.borrow_mut();
        match scope {
            MenuScope::Root => {
                menu.push(node);
                true
            }
            MenuScope::Container(path) => match menu.get_mut(path[0]) {
                Some(MenuNode::Radios { children, .. }) if kind == "entry" => {
                    children.push(key.to_string());
                    true
                }
                Some(MenuNode::Folder { children, .. }) => {
                    children.push(node);
                    true
                }
                _ => false,
            },
        }
    }

    /// Reorder a sibling pair. `path` points at the moved row; delta -1 is
    /// up, 1 is down (a radios entry reorders the group's key list).
    fn menu_move(&self, path: &[usize], delta: isize) -> bool {
        let mut menu = self.menu.borrow_mut();
        let Some(&idx) = path.last() else {
            return false;
        };
        match path.len() {
            1 => match menu_step(idx, delta, menu.len()) {
                Some(target) => {
                    menu.swap(idx, target);
                    true
                }
                None => false,
            },
            2 => match menu.get_mut(path[0]) {
                Some(MenuNode::Folder { children, .. }) => {
                    match menu_step(idx, delta, children.len()) {
                        Some(target) => {
                            children.swap(idx, target);
                            true
                        }
                        None => false,
                    }
                }
                Some(MenuNode::Radios { children, .. }) => {
                    match menu_step(idx, delta, children.len()) {
                        Some(target) => {
                            children.swap(idx, target);
                            true
                        }
                        None => false,
                    }
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Drop side of the drag handle: move the source item before the
    /// target row within the same sibling list. Cross-parent drops are
    /// ignored — the popup nests one level and reparenting by drop is not
    /// worth the surprise.
    fn menu_drag(&self, src: &[usize], dst: &[usize]) -> bool {
        if src.len() != dst.len() || src == dst || src[..src.len() - 1] != dst[..dst.len() - 1] {
            return false;
        }
        let (Some(&s), Some(&d)) = (src.last(), dst.last()) else {
            return false;
        };
        let mut menu = self.menu.borrow_mut();
        match src.len() {
            1 => {
                let item = menu.remove(s);
                let d = if s < d { d - 1 } else { d };
                menu.insert(d, item);
                true
            }
            2 => match menu.get_mut(src[0]) {
                Some(MenuNode::Folder { children, .. }) => {
                    if s >= children.len() || d >= children.len() {
                        return false;
                    }
                    let item = children.remove(s);
                    let d = if s < d { d - 1 } else { d };
                    children.insert(d, item);
                    true
                }
                Some(MenuNode::Radios { children, .. }) => {
                    if s >= children.len() || d >= children.len() {
                        return false;
                    }
                    let key = children.remove(s);
                    let d = if s < d { d - 1 } else { d };
                    children.insert(d, key);
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Move a folder child up to the root (appended at the end, like the
    /// tree editor appended it). Radio-group entries cannot exist at the
    /// root, so they stay put.
    fn menu_out(&self, path: &[usize]) -> bool {
        let mut menu = self.menu.borrow_mut();
        if path.len() != 2 {
            return false;
        }
        let item = match menu.get_mut(path[0]) {
            Some(MenuNode::Folder { children, .. }) if path[1] < children.len() => {
                Some(children.remove(path[1]))
            }
            _ => None,
        };
        let Some(item) = item else {
            return false;
        };
        // An emptied folder goes away with its last child.
        let emptied = matches!(
            menu.get(path[0]),
            Some(MenuNode::Folder { children, .. }) if children.is_empty()
        );
        if emptied {
            menu.remove(path[0]);
        }
        menu.push(item);
        true
    }

    /// Rename a root folder. Entries inside keep their built-in labels,
    /// and only root folders are renamable.
    fn menu_rename(&self, path: &[usize], label: &str) -> bool {
        let label = label.trim();
        if label.is_empty() || path.len() != 1 {
            return false;
        }
        let mut menu = self.menu.borrow_mut();
        match menu.get_mut(path[0]) {
            Some(MenuNode::Folder { label: l, .. }) => {
                *l = label.to_string();
                true
            }
            _ => false,
        }
    }

    /// Remove one item. The popup must never end up empty (the last
    /// top-level entry refuses), and an emptied container goes away with
    /// its last child — both as the tree editor did.
    fn menu_delete(&self, path: &[usize]) -> bool {
        let mut menu = self.menu.borrow_mut();
        let Some(&idx) = path.last() else {
            return false;
        };
        match path.len() {
            1 if menu.len() <= 1 => false,
            1 => {
                menu.remove(idx);
                true
            }
            2 => match menu.get_mut(path[0]) {
                Some(MenuNode::Folder { children, .. }) if idx < children.len() => {
                    children.remove(idx);
                    if children.is_empty() {
                        menu.remove(path[0]);
                    }
                    true
                }
                Some(MenuNode::Radios { children, .. }) if idx < children.len() => {
                    children.remove(idx);
                    if children.is_empty() {
                        menu.remove(path[0]);
                    }
                    true
                }
                _ => false,
            },
            _ => false,
        }
    }

    /// Throw the design away and go back to the shipped shape.
    pub(super) fn menu_reset(&self) {
        let default = Config::load().default_menu();
        *self.menu.borrow_mut() = normalize_menu(&default);
    }
}
