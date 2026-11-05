//! The `-emoji` picker window: a search field over the embedded emoji
//! table (plus the user's `emoji.toml` overrides), one click copies the
//! glyph. On X11 the picker then injects Ctrl+V through `/dev/uinput` once
//! the focus has returned to the previous window; Wayland only copies and
//! says so. Spawned detached from the engine (`server/hotkey.rs`), so it
//! must survive an `ibus restart` — systemd-run owns its lifetime.

use gtk4::glib;
use gtk4::prelude::*;
use libadwaita as adw;

use super::text::t;
use crate::emoji::{picker_entries, UserEmoji};

pub fn run() -> Result<(), Box<dyn std::error::Error>> {
    libadwaita::init().map_err(|e| format!("libadwaita init failed: {e}"))?;
    // A separate id from the settings window: GApplication is
    // single-instance, and the picker must never wake the settings app.
    let app = adw::Application::builder()
        .application_id("org.freedesktop.IBus.mangrs.emoji")
        .build();
    app.connect_activate(build);
    app.run_with_args::<&str>(&[]);
    Ok(())
}

fn build(app: &adw::Application) {
    let search = gtk4::SearchEntry::new();
    search.set_placeholder_text(Some(&t("emoji.search", "Tìm emoji (từ khóa Việt/Anh)…")));
    search.set_hexpand(true);

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&search));

    let flow = gtk4::FlowBox::new();
    flow.set_selection_mode(gtk4::SelectionMode::None);
    flow.set_activate_on_single_click(true);
    flow.set_homogeneous(true);
    flow.set_max_children_per_line(9);
    flow.set_min_children_per_line(6);
    flow.set_margin_start(8);
    flow.set_margin_end(8);
    flow.set_margin_bottom(8);

    // The search blob (name + keywords, lowercase) lives in the child's
    // widget name; the filter closure and the match counter read it.
    for entry in picker_entries(&UserEmoji::load().map) {
        let child = gtk4::FlowBoxChild::new();
        let label = gtk4::Label::new(Some(&entry.glyph));
        // x-large keeps the glyph readable without a fixed pixel size.
        label.set_markup(&format!("<span size='x-large'>{}</span>", entry.glyph));
        label.set_tooltip_text(Some(&entry.name));
        child.set_child(Some(&label));
        let mut blob = entry.name.to_lowercase();
        for keyword in &entry.keywords {
            blob.push(' ');
            blob.push_str(keyword);
        }
        child.set_widget_name(&blob);
        flow.append(&child);
    }

    let no_match = gtk4::Label::new(Some(&t("emoji.none", "Không có emoji nào khớp")));
    no_match.add_css_class("dim-label");
    no_match.set_margin_top(24);
    no_match.set_visible(false);

    let scroll = gtk4::ScrolledWindow::new();
    scroll.set_policy(gtk4::PolicyType::Never, gtk4::PolicyType::Automatic);
    scroll.set_vexpand(true);
    scroll.set_child(Some(&flow));

    // One-line status under the grid: copy outcome, or the paste notice on
    // Wayland. The picker closes by itself after a pick.
    let status = gtk4::Label::new(None);
    status.set_margin_bottom(10);

    let page = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    page.append(&header);
    page.append(&scroll);
    page.append(&no_match);
    page.append(&status);

    // `.content()`, not `set_child()` — an AdwApplicationWindow rejects the
    // latter (the rule-picker window aborted on startup over exactly this).
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title(t("emoji.title", "Emoji — MăngRS"))
        .default_width(520)
        .default_height(440)
        .content(&page)
        .build();

    // Escape closes the picker, whether the focus sits in the search or on
    // the grid.
    {
        let win = window.clone();
        let controller = gtk4::EventControllerKey::new();
        controller.connect_key_pressed(move |_, key, _, _| {
            if key == gtk4::gdk::Key::Escape {
                win.close();
                glib::Propagation::Stop
            } else {
                glib::Propagation::Proceed
            }
        });
        window.add_controller(controller);
    }

    // Search: refilter the grid and show the empty-state label when
    // nothing matches. The filter reads the entry's current text directly,
    // so no shared state is needed between the two closures.
    {
        let entry = search.clone();
        let no_match = no_match.clone();
        flow.set_filter_func(move |child| {
            let q = entry.text().trim().to_lowercase();
            q.is_empty() || child.widget_name().contains(q.as_str())
        });
        let flow_for_count = flow.clone();
        let no_match_for_count = no_match.clone();
        search.connect_search_changed(move |entry| {
            let q = entry.text().trim().to_lowercase();
            flow_for_count.invalidate_filter();
            let mut matches = q.is_empty();
            if !matches {
                let mut i = 0;
                while let Some(child) = flow_for_count.child_at_index(i) {
                    if child.widget_name().contains(q.as_str()) {
                        matches = true;
                        break;
                    }
                    i += 1;
                }
            }
            no_match_for_count.set_visible(!matches);
        });
    }

    // Pick: copy, then paste on X11 (the window closes first so the focus
    // returns to the app the user was typing in). Wayland has no reachable
    // paste channel — uinput events never reach mutter-relayed clients —
    // so the status line tells the user to press Ctrl+V.
    {
        let window = window.clone();
        let status = status.clone();
        flow.connect_child_activated(move |_, child| {
            let Some(label) = child.child().and_then(|c| c.downcast::<gtk4::Label>().ok()) else {
                return;
            };
            let glyph = label.text().to_string();
            let copied = crate::paste::copy_to_clipboard(&glyph);
            let x11_paste = copied
                && std::env::var_os("DISPLAY").is_some()
                && std::env::var_os("WAYLAND_DISPLAY").is_none()
                && crate::uinput::Uinput::available();
            if x11_paste {
                window.close();
                // Give the window manager a beat to hand the focus back.
                std::thread::spawn(|| {
                    std::thread::sleep(std::time::Duration::from_millis(250));
                    crate::uinput::Uinput::ctrl_v();
                });
            } else if copied {
                status.set_text(&t("emoji.copied", "Đã copy — dán bằng Ctrl+V"));
                let window = window.clone();
                gtk4::glib::timeout_add_local(std::time::Duration::from_millis(1200), move || {
                    window.close();
                    glib::ControlFlow::Break
                });
            } else {
                status.set_text(&t(
                    "emoji.copy_failed",
                    "Không copy được (cần wl-copy hoặc xclip)",
                ));
            }
        });
    }

    // Open with the search focused: type, then click.
    search.grab_focus();
    window.present();
}
