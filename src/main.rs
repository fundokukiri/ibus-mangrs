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
