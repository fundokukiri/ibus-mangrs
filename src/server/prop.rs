//! The panel property menu: registration, PropertyActivate handling and
//! the settings/repository openers the menu actions spawn.

use super::*;

/// Send the full property list to ibus-daemon. `RegisterProperties` is a
/// signal `(v)`, not a method: the daemon's engine proxy relays it to the
/// panel (GNOME Shell's input-source popup).
pub(super) fn register_properties(config: &Config, out: &mut Vec<Message>) {
    let props = ibus_prop::properties(config);
    out.push(signal("RegisterProperties").append1(ibus_prop::prop_list_variant(&props)));
}

/// Handle a panel click. The daemon sends `PropertyActivate(s name, u state)`.
///
/// GNOME Shell sends one call per item of a clicked radio group: the picked
/// item with state CHECKED and every sibling with state UNCHECKED (in menu
/// order, `keyboard.js::_buildPropSubMenu`). The UNCHECKED calls must be
/// ignored — honoring them makes the last sibling of the group always win
/// (`VIQR` for the method menu, `preedit` for the display menu).
pub(super) fn activate_property(
    engine: &mut Engine,
    name: &str,
    state: u32,
    out: &mut Vec<Message>,
) {
    if name == ibus_prop::KEY_ABOUT {
        open_repo_page();
        return;
    }
    if name == ibus_prop::KEY_SHORTCUTS {
        open_settings("general");
        return;
    }

    // Radio items arrive as `group:value`; only the CHECKED call counts.
    if let Some((group, value)) = name.split_once(':') {
        if state != ibus_prop::STATE_CHECKED {
            return;
        }
        let mut config = Config::load();
        match group {
            "charset" => match CharsetConfig::from_key(value) {
                Some(c) => config.charset = c,
                None => return,
            },
            "method" => match MethodConfig::from_key(value) {
                Some(m) => config.method = m,
                None => return,
            },
            "display" => match ModeConfig::from_key(value) {
                Some(m) => config.mode = m,
                None => return,
            },
            _ => return,
        }
        // Do not move the check mark if the setting could not be persisted.
        if config.save().is_err() {
            return;
        }
        let actions = engine.apply_config(&config);
        engine.set_enabled(config.enabled);
        emit(engine, &actions, out);
        // Radios need their whole group resent so the old one loses the check.
        update_group(&ibus_prop::properties(&config), group, out);
        return;
    }

    // Toggles carry their new state directly. The popup shows the user's
    // `quick_menu` selection, so every registered toggle must work here —
    // the registry is the single source of both (see `ibus_prop::QUICK_TOGGLES`).
    let checked = state == ibus_prop::STATE_CHECKED;
    let mut config = Config::load();
    match name {
        ibus_prop::KEY_ENABLED => config.enabled = checked,
        ibus_prop::KEY_MACRO_EDITOR => {
            open_settings("macros");
            return;
        }
        _ => match ibus_prop::quick_toggle(name) {
            Some(t) => (t.set)(&mut config, checked),
            None => return,
        },
    }
    if config.save().is_err() {
        return;
    }
    let actions = engine.apply_config(&config);
    engine.set_enabled(config.enabled);
    emit(engine, &actions, out);
    update_group(&ibus_prop::properties(&config), name, out);
}

fn update_group(props: &[Prop], group: &str, out: &mut Vec<Message>) {
    for prop in props {
        if prop.key == group {
            if prop.sub.is_empty() {
                push_update(prop, out);
            } else {
                for child in &prop.sub {
                    push_update(child, out);
                }
            }
        }
        update_group(&prop.sub, group, out);
    }
}

fn push_update(prop: &Prop, out: &mut Vec<Message>) {
    out.push(signal("UpdateProperty").append1(prop.variant()));
}

/// The "Phím tắt" item opens the GTK4 settings window, same as `-gui`.
fn open_settings(page: &str) {
    use std::os::unix::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else {
        return;
    };
    let page = format!("--page={page}");
    // ibus-daemon runs as the systemd user unit
    // org.freedesktop.IBus.session.GNOME.service, and an `ibus restart`
    // tears that whole cgroup down (KillMode=control-group) — every process
    // the engine ever spawned dies with it, process groups notwithstanding.
    // The window is therefore started as a transient user service: systemd
    // owns it in its own cgroup, so it survives the restart (its restart
    // button depends on that). Plain spawn for sessions without a systemd
    // user manager; its own process group is all that can be done there.
    let spawned = std::process::Command::new("systemd-run")
        .args(["--user", "--quiet"])
        .arg(&exe)
        .arg("-gui")
        .arg(&page)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
    if spawned.is_err() {
        let _ = std::process::Command::new(exe)
            .arg("-gui")
            .arg(page)
            .process_group(0)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn();
    }
}

/// Open the project's GitHub repository in the default browser (the
/// "MăngRS <version>" entry at the top of the menu).
fn open_repo_page() {
    let _ = std::process::Command::new("xdg-open")
        .arg(ibus_prop::HOMEPAGE)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}
