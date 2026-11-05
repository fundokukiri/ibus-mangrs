//! The per-app rule picker (`-picker`): a small window the engine spawns on
//! the app-rule hotkey, carrying the rule target over argv — the same
//! string the rules will match against (the engine knows it at spawn time;
//! once the window opens, GetFocusedWindow would answer with the picker).
//! A choice writes an `[[app_rules]]` entry to config.toml; the engine's
//! mtime watch applies it within a second.

use gtk4::prelude::*;
use libadwaita as adw;

use crate::config::{app_rule_mode, Config, ModeConfig};
use crate::ibus_prop::MODES;

pub fn run_picker(app_target: Option<String>) -> Result<(), Box<dyn std::error::Error>> {
    // Adwaita styles must be initialized before any Adw widget exists.
    libadwaita::init().map_err(|e| format!("libadwaita init failed: {e}"))?;
    let app = adw::Application::builder()
        // A different id from the settings window: both may run at once,
        // and single-instance activation must not raise the control panel.
        .application_id("org.freedesktop.IBus.mangrs.picker")
        .build();
    app.connect_activate(move |app| build_window(app, app_target.clone()));
    app.run_with_args::<&str>(&[]);
    Ok(())
}

fn build_window(app: &adw::Application, app_target: Option<String>) {
    let target = app_target.unwrap_or_default();
    let known = !target.is_empty();

    // The rule matching this target right now preselects its radio; the
    // remove button only makes sense when a rule exists.
    let config = Config::load();
    let current = app_rule_mode(&config.app_rules, Some(&target));
    let has_rule = current.is_some();

    let root = gtk4::Box::new(gtk4::Orientation::Vertical, 0);
    let header = gtk4::HeaderBar::new();
    root.append(&header);
    let content = gtk4::Box::new(gtk4::Orientation::Vertical, 12);
    content.set_margin_top(18);
    content.set_margin_bottom(18);
    content.set_margin_start(18);
    content.set_margin_end(18);
    root.append(&content);

    // `content` is the Adwaita window's own property: its gtk-level
    // set_child is overridden to abort ("not supported for
    // AdwApplicationWindow"), which is exactly the flash-and-die the first
    // picker version shipped.
    let window = adw::ApplicationWindow::builder()
        .application(app)
        .title("MăngRS — luật theo app")
        .default_width(440)
        .content(&root)
        .build();

    let heading = gtk4::Label::builder().wrap(true).xalign(0.0).build();
    if known {
        heading.set_text(&format!("Chế độ gõ cho: {target}"));
        heading.add_css_class("monospace");
    } else {
        heading.set_text(
            "Không xác định được app đang focus (cần extension MăngRS Focus hoặc một app có tên).",
        );
    }
    content.append(&heading);

    // One radio per display mode, in the panel menu's order. Clicking one
    // only selects; the buttons below write the rule.
    let radios = gtk4::Box::new(gtk4::Orientation::Vertical, 6);
    let mut buttons: Vec<(gtk4::CheckButton, ModeConfig)> = Vec::new();
    for (key, label) in MODES {
        let Some(mode) = ModeConfig::from_key(key) else {
            continue;
        };
        let btn = gtk4::CheckButton::with_label(label);
        if let Some((first, _)) = buttons.first() {
            btn.set_group(Some(first));
        }
        if current == Some(mode) {
            btn.set_active(true);
        }
        radios.append(&btn);
        buttons.push((btn, mode));
    }
    content.append(&radios);

    let apply = {
        let window = window.clone();
        let target = target.clone();
        move |mode: ModeConfig| {
            let mut config = Config::load();
            config.upsert_app_rule(&target, mode);
            if config.save().is_ok() {
                window.close();
            }
        }
    };

    // The off switch as a button: the most common choice ("no Vietnamese in
    // this app") should not need a radio hunt.
    let off_btn = gtk4::Button::with_label("Tắt tiếng Việt cho app này");
    off_btn.add_css_class("suggested-action");
    {
        let apply = apply.clone();
        off_btn.connect_clicked(move |_| apply(ModeConfig::Off));
    }
    let remove_btn = gtk4::Button::with_label("Xóa luật của app");
    remove_btn.add_css_class("destructive-action");
    remove_btn.set_sensitive(has_rule);
    {
        let window = window.clone();
        let target = target.clone();
        remove_btn.connect_clicked(move |_| {
            let mut config = Config::load();
            config.remove_app_rule(&target);
            if config.save().is_ok() {
                window.close();
            }
        });
    }
    let apply_btn = gtk4::Button::with_label("Áp dụng");
    apply_btn.add_css_class("flat");
    {
        let window = window.clone();
        let buttons = buttons.clone();
        apply_btn.connect_clicked(move |_| {
            let Some((_, mode)) = buttons.iter().find(|(b, _)| b.is_active()) else {
                return;
            };
            let mut config = Config::load();
            config.upsert_app_rule(&target, *mode);
            if config.save().is_ok() {
                window.close();
            }
        });
    }
    // An unnamed target cannot write a rule that would ever match.
    off_btn.set_sensitive(known);
    apply_btn.set_sensitive(known);

    let row = gtk4::Box::new(gtk4::Orientation::Horizontal, 6);
    row.append(&off_btn);
    row.append(&remove_btn);
    row.append(&apply_btn);
    content.append(&row);

    window.present();
}
