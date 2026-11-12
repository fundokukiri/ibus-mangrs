//! Entry point for the IBus engine daemon.
//!
//! Started by ibus-daemon via the component XML, or by hand for debugging.
//! `-gui` launches the GTK4 settings window instead (built with `--features gui`).

use std::sync::{Arc, Mutex};

use ibus_mangrs::config::Config;
use ibus_mangrs::engine::Engine;
use ibus_mangrs::server::{Server, SharedEngine};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();
    if args.iter().any(|a| a == "-gui" || a == "--gui") {
        return run_gui();
    }

    let config = Config::load();
    ibus_mangrs::debug_log::set_enabled(config.debug_log);
    ibus_mangrs::debug_log::set_text_enabled(config.debug_log_text);
    warn_if_in_input_group();
    ibus_mangrs::debug_log::log(&format!(
        "engine start uinput={}",
        ibus_mangrs::uinput::Uinput::available()
    ));
    let (method, mode) = config.engine_settings();
    let mut engine = Engine::new(method, mode);
    engine.apply_config(&config);
    engine.set_enabled(config.enabled);
    // Deletions for clients without surrounding-text data go through real
    // Backspace key injection instead of the (forbidden) protocol delete.
    engine.set_uinput_available(ibus_mangrs::uinput::Uinput::available());

    let engine: SharedEngine = Arc::new(Mutex::new(engine));
    let server = Server::connect(engine)?;

    // Exit cleanly on SIGTERM/SIGINT instead of dying with the default
    // handler: closing the socket makes ibus-daemon release the bus name,
    // so a restarted engine does not queue behind a dead owner (`InQueue`).
    install_signal_handlers();

    server.run();
    Ok(())
}

/// Warn when the user still belongs to `input`. That group can read every
/// keyboard of the machine (`/dev/input/event*`); `/dev/uinput` no longer
/// needs it, because the session ACL from `70-uinput.rules` is used instead.
fn warn_if_in_input_group() {
    let Ok(name) = std::ffi::CString::new("input") else {
        return;
    };
    let gid = unsafe {
        let group = libc::getgrnam(name.as_ptr());
        if group.is_null() {
            return;
        }
        (*group).gr_gid
    };
    let groups = unsafe {
        let count = libc::getgroups(0, std::ptr::null_mut());
        if count <= 0 {
            return;
        }
        let mut ids = vec![0 as libc::gid_t; count as usize];
        let read = libc::getgroups(count, ids.as_mut_ptr());
        if read <= 0 {
            return;
        }
        ids.truncate(read as usize);
        ids
    };
    let primary = unsafe { libc::getgid() };
    if groups.contains(&gid) || primary == gid {
        let msg = "warning: this user is still in the `input` group, which can \
                   read every keystroke of the machine; /dev/uinput does not \
                   need it any more (remove with: gpasswd -d <user> input, \
                   then log in again)";
        eprintln!("{msg}");
        ibus_mangrs::debug_log::log(msg);
    }
}

fn install_signal_handlers() {
    use signal_hook::consts::{SIGINT, SIGTERM};
    let Ok(mut signals) = signal_hook::iterator::Signals::new([SIGTERM, SIGINT]) else {
        return;
    };
    std::thread::spawn(move || {
        // Any of SIGTERM/SIGINT: exit so the socket closes and ibus-daemon
        // releases the bus name.
        let sig = signals.forever().next();
        eprintln!("ibus-mangrs: received signal {sig:?}, exiting");
        std::process::exit(0);
    });
}

#[cfg(feature = "gui")]
fn run_gui() -> Result<(), Box<dyn std::error::Error>> {
    ibus_mangrs::gui::run()
}

#[cfg(not(feature = "gui"))]
fn run_gui() -> Result<(), Box<dyn std::error::Error>> {
    Err("this build has no GUI; rebuild with --features gui".into())
}
