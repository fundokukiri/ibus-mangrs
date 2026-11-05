//! Shared harness for the local end-to-end functional tests.
//!
//! Drives the **real release binary** over a **private D-Bus session bus**
//! (created by `tests/e2e/run.sh` with `dbus-run-session`), playing the role of
//! ibus-daemon and the focused client: calls `org.freedesktop.IBus.Engine`
//! methods, listens to the engine's signals, and asserts on the exact stream
//! real applications see. This is the protocol complement to `cargo test`,
//! which never touches D-Bus.
//!
//! Each `tests/e2e_<feature>.rs` file owns one feature's cases and calls
//! [`run_suite`]. Skipped unless run through `tests/e2e/run.sh` (which sets
//! `MANGRS_E2E` and `MANGRS_E2E_BIN`): the cases need a private bus, and
//! firing engines at the user's real session bus would be wrong. Plain
//! `cargo test` never executes them — every case lives behind `#[ignore]`.
//!
//! Hard limits, by design:
//!   * no uinput key injection is ever triggered (injected keys would land in
//!     whatever window has focus on the real desktop); those paths are covered
//!     by the unit tests with fake transports,
//!   * nothing outside temp dirs is touched (config, cache, system IBus).

// Shared toolbox: each feature file uses a different subset of it.
#![allow(dead_code)]

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread::sleep;
use std::time::{Duration, Instant};

use dbus::arg::{RefArg, Variant};
use dbus::ffidisp::{BusType, Connection};
use dbus::strings::{BusName, Interface, Member, Path as DbusPath};
use dbus::Message;

use ibus_mangrs::ibus_text::{plain_text, text_from_variant};
use ibus_mangrs::server::{BUS_NAME, ENGINE_IFACE, ENGINE_PATH};

pub const PUMP_MS: u64 = 60;
pub const SETTLE_MS: u64 = 300;
const CALL_TIMEOUT_MS: i32 = 3000;

pub const CAP_PREEDIT: u32 = 1;
pub const CAP_SURROUNDING: u32 = 1 << 5;
pub const CAPS_BOTH: u32 = CAP_PREEDIT | CAP_SURROUNDING;
pub const CONTROL: u32 = 1 << 2;
pub const ALT: u32 = 1 << 3;
pub const KEY_BACKSPACE: u32 = 0xFF08;

// --- engine signals, parsed at the wire -------------------------------------

#[derive(Debug, Clone)]
pub enum Sig {
    Commit(String),
    /// (text, visible, preedit mode) — mode 1 is IBUS_ENGINE_PREEDIT_COMMIT.
    Preedit {
        text: String,
        visible: bool,
        mode: u32,
        /// Whether the variant carried an IBusAttribute (the underline).
        underlined: bool,
    },
    Delete {
        offset: i32,
        n: u32,
    },
    /// A key event the engine forwarded through IBus (the ForwardKeyEvent
    /// mode's Backspace transport).
    ForwardKeyEvent {
        keyval: u32,
        keycode: u32,
        state: u32,
    },
    RegisterProperties(String),
    UpdateProperty,
    RequireSurroundingText,
    Other(String),
}

pub fn commits(sigs: &[Sig]) -> Vec<&str> {
    sigs.iter()
        .filter_map(|s| match s {
            Sig::Commit(t) => Some(t.as_str()),
            _ => None,
        })
        .collect()
}

pub fn preedits(sigs: &[Sig]) -> Vec<(&str, bool, u32)> {
    sigs.iter()
        .filter_map(|s| match s {
            Sig::Preedit {
                text,
                visible,
                mode,
                ..
            } => Some((text.as_str(), *visible, *mode)),
            _ => None,
        })
        .collect()
}

pub fn deletes(sigs: &[Sig]) -> Vec<(i32, u32)> {
    sigs.iter()
        .filter_map(|s| match s {
            Sig::Delete { offset, n } => Some((*offset, *n)),
            _ => None,
        })
        .collect()
}

pub fn underlines(sigs: &[Sig]) -> Vec<bool> {
    sigs.iter()
        .filter_map(|s| match s {
            Sig::Preedit { underlined, .. } => Some(*underlined),
            _ => None,
        })
        .collect()
}

pub fn has_member(sigs: &[Sig], name: &str) -> bool {
    sigs.iter().any(|s| match s {
        Sig::Other(o) => o == name,
        Sig::UpdateProperty => name == "UpdateProperty",
        Sig::RequireSurroundingText => name == "RequireSurroundingText",
        Sig::RegisterProperties(_) => name == "RegisterProperties",
        _ => false,
    })
}

// --- simulated client -------------------------------------------------------

/// Stand-in for the application's text field: applies commit/delete/forward
/// exactly like the unit-test replay harness in engine.rs.
#[derive(Default)]
pub struct Buffer {
    chars: Vec<char>,
    caret: usize,
}

impl Buffer {
    pub fn text(&self) -> String {
        self.chars.iter().collect()
    }

    pub fn insert(&mut self, s: &str) {
        for c in s.chars() {
            self.chars.insert(self.caret, c);
            self.caret += 1;
        }
    }

    pub fn delete_surrounding(&mut self, offset: i32, n: u32) {
        let len = self.chars.len();
        let start = (self.caret as i64 + offset as i64).clamp(0, len as i64) as usize;
        let end = (start + n as usize).clamp(start, len);
        self.chars.drain(start..end);
        self.caret = self.caret.min(start);
    }

    pub fn apply(&mut self, sig: &Sig) {
        match sig {
            Sig::Commit(t) => self.insert(t),
            Sig::Delete { offset, n } => self.delete_surrounding(*offset, *n),
            // ForwardKeyEvent mode: the client's IM context applies the
            // forwarded Backspace like a real key press.
            Sig::ForwardKeyEvent { state: 0, .. } if self.caret > 0 => {
                self.caret -= 1;
                self.chars.remove(self.caret);
            }
            // Preedit text never sits in the application buffer.
            _ => {}
        }
    }
}

// --- one engine session -----------------------------------------------------

pub struct Sess {
    child: Child,
    dir: PathBuf,
    conn: Connection,
    signals: Vec<Sig>,
    mark: usize,
}

pub(crate) fn engine_msg(member: &str) -> Message {
    // Test-only unwraps: every name here is this crate's own constant.
    Message::method_call(
        &BusName::new(BUS_NAME).unwrap(),
        &DbusPath::new(ENGINE_PATH).unwrap(),
        &Interface::new(ENGINE_IFACE).unwrap(),
        &Member::new(member).unwrap(),
    )
}

impl Sess {
    fn spawn(bin: &PathBuf, case: &Case, index: usize) -> Sess {
        let Case {
            name: _,
            cfg,
            macros,
            raw,
            dict,
            f: _,
        } = case;
        let dir = env::temp_dir().join(format!("mangrs-e2e-{}-{index}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let cfg_dir = dir.join(".config").join("ibus-mangrs");
        fs::create_dir_all(&cfg_dir).expect("config dir");
        // Config values are preformatted TOML values (`"vni"`, `true`).
        let mut toml_text = String::new();
        for (k, v) in cfg.iter() {
            toml_text.push_str(&format!("{k} = {v}\n"));
        }
        fs::write(cfg_dir.join("config.toml"), toml_text).expect("write config");
        if !raw.is_empty() {
            let mut f = fs::OpenOptions::new()
                .append(true)
                .open(cfg_dir.join("config.toml"))
                .expect("config for append");
            use std::io::Write;
            write!(f, "{raw}").expect("append raw toml");
        }
        if !dict.is_empty() {
            fs::write(cfg_dir.join("dictionary.txt"), dict).expect("write dictionary");
        }
        if !macros.is_empty() {
            let mut text = String::new();
            for (k, v) in macros.iter() {
                text.push_str(&format!("{k} = \"{v}\"\n"));
            }
            fs::write(cfg_dir.join("macros.toml"), text).expect("write macros");
        }
        fs::create_dir_all(dir.join(".cache")).expect("cache dir");

        let conn = Connection::get_private(BusType::Session)
            .expect("session bus — run through tests/e2e/run.sh");
        conn.register().expect("register on the private bus");
        conn.add_match(&format!(
            "type='signal',interface='{ENGINE_IFACE}',path='{ENGINE_PATH}'"
        ))
        .expect("add signal match");

        let child = Command::new(bin)
            .env(
                "IBUS_ADDRESS",
                env::var("DBUS_SESSION_BUS_ADDRESS").expect("private bus address"),
            )
            .env("XDG_CONFIG_HOME", dir.join(".config"))
            .env("XDG_CACHE_HOME", dir.join(".cache"))
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn engine");

        let mut s = Sess {
            child,
            dir,
            conn,
            signals: Vec::new(),
            mark: 0,
        };
        s.wait_ready();
        s
    }

    fn wait_ready(&mut self) {
        for _ in 0..100 {
            let ping = Message::method_call(
                &BusName::new(BUS_NAME).unwrap(),
                &DbusPath::new(ENGINE_PATH).unwrap(),
                &Interface::new("org.freedesktop.DBus.Peer").unwrap(),
                &Member::new("Ping").unwrap(),
            );
            if self.conn.send_with_reply_and_block(ping, 300).is_ok() {
                return;
            }
            sleep(Duration::from_millis(50));
        }
        // A half-started engine would keep the bus name and poison every later
        // case — take it down before surfacing the failure.
        unsafe {
            libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM);
        }
        let _ = self.child.wait();
        panic!("engine did not come up on the private bus");
    }

    pub fn config_path(&self) -> PathBuf {
        self.dir
            .join(".config")
            .join("ibus-mangrs")
            .join("config.toml")
    }

    pub fn cache_dir(&self) -> PathBuf {
        self.dir.join(".cache")
    }

    fn call(&mut self, msg: Message) -> Message {
        self.conn
            .send_with_reply_and_block(msg, CALL_TIMEOUT_MS)
            .expect("engine call")
    }

    /// Send an engine method call and keep the reply (used for
    /// CandidateClicked, which has no bool reply to interpret).
    pub fn call_engine(&mut self, msg: Message) {
        self.call(msg);
    }

    pub fn key(&mut self, keyval: u32, state: u32) -> bool {
        let reply = self.call(engine_msg("ProcessKeyEvent").append3(keyval, 0u32, state));
        reply.read1::<bool>().expect("bool reply")
    }

    pub fn focus_in(&mut self, client: &str) {
        self.call(engine_msg("FocusInId").append2("/org/freedesktop/IBus/InputContext_1", client));
    }

    pub fn caps(&mut self, value: u32) {
        self.call(engine_msg("SetCapabilities").append1(value));
    }

    pub fn focus_out(&mut self) {
        self.call(engine_msg("FocusOut"));
    }

    pub fn disable(&mut self) {
        self.call(engine_msg("Disable"));
    }

    pub fn reset_call(&mut self) {
        self.call(engine_msg("Reset"));
    }

    pub fn property(&mut self, name: &str, state: u32) {
        self.call(engine_msg("PropertyActivate").append2(name, state));
    }

    pub fn report(&mut self, text: &str) {
        let len = text.chars().count() as u32;
        self.call(engine_msg("SetSurroundingText").append3(plain_text(text), len, len));
    }

    /// Focus a fresh client and settle; returns the focus-time signals
    /// (property menu, RequireSurroundingText).
    pub fn focus(&mut self, caps: u32, client: &str) -> Vec<Sig> {
        self.focus_in(client);
        self.caps(caps);
        self.pump(SETTLE_MS);
        self.drain()
    }

    /// Plain `FocusIn` without `FocusInId`: the engine learns nothing about
    /// the client — the unnamed X11/XIM path the `none` rule targets.
    pub fn focus_anonymous(&mut self, caps: u32) -> Vec<Sig> {
        self.call(engine_msg("FocusIn"));
        self.caps(caps);
        self.pump(SETTLE_MS);
        self.drain()
    }

    /// Collect everything the engine emitted since the last drain.
    pub fn drain(&mut self) -> Vec<Sig> {
        let out: Vec<Sig> = self.signals[self.mark..].to_vec();
        self.signals.truncate(self.mark);
        out
    }

    /// Receive signals for up to `ms` (libdbus buffers them during the
    /// blocking calls above, so ordering is preserved).
    pub fn pump(&mut self, ms: u64) {
        let deadline = Instant::now() + Duration::from_millis(ms);
        while Instant::now() < deadline {
            let mut incoming = Vec::new();
            for m in self.conn.incoming(10) {
                incoming.push(m);
                if Instant::now() >= deadline {
                    break;
                }
            }
            for m in incoming {
                self.collect(m);
            }
            if Instant::now() >= deadline {
                break;
            }
            sleep(Duration::from_millis(5));
        }
    }

    fn collect(&mut self, m: Message) {
        let name = m
            .member()
            .map(|x| x.to_string())
            .unwrap_or_else(|| "unknown".into());
        let sig = match name.as_str() {
            "CommitText" => m
                .read1::<Variant<Box<dyn RefArg>>>()
                .ok()
                .and_then(|v| text_from_variant(&v))
                .map(Sig::Commit),
            "UpdatePreeditText" => m
                .read4::<Variant<Box<dyn RefArg>>, u32, bool, u32>()
                .ok()
                .map(|(v, _cursor, visible, mode)| {
                    // The attribute list rides inside the variant; its
                    // presence in the Debug render is the underline.
                    let underlined = format!("{v:?}").contains("IBusAttribute");
                    Sig::Preedit {
                        text: text_from_variant(&v).unwrap_or_default(),
                        visible,
                        mode,
                        underlined,
                    }
                }),
            "DeleteSurroundingText" => m
                .read2::<i32, u32>()
                .ok()
                .map(|(offset, n)| Sig::Delete { offset, n }),
            "ForwardKeyEvent" => m
                .read3::<u32, u32, u32>()
                .ok()
                .map(|(keyval, keycode, state)| Sig::ForwardKeyEvent {
                    keyval,
                    keycode,
                    state,
                }),
            "RegisterProperties" => Some(Sig::RegisterProperties(format!("{m:?}"))),
            "UpdateProperty" => Some(Sig::UpdateProperty),
            "RequireSurroundingText" => Some(Sig::RequireSurroundingText),
            _ => Some(Sig::Other(name.clone())),
        };
        self.signals.push(sig.unwrap_or(Sig::Other(name)));
    }

    /// Type without surrounding-text reports (preedit-style cases).
    pub fn type_plain(&mut self, s: &str) -> Vec<Sig> {
        let mut all = Vec::new();
        for ch in s.chars() {
            let _ = self.key(ch as u32, 0);
            self.pump(PUMP_MS);
            all.extend(self.drain());
        }
        all
    }

    /// Type like a well-behaved client: after every key, apply the engine's
    /// signals to the buffer, insert the forwarded raw key, and send the fresh
    /// SetSurroundingText report. This keeps every delete on the safe protocol
    /// path (never key injection). Returns the final buffer and every signal.
    pub fn type_reported(&mut self, s: &str) -> (String, Vec<Sig>) {
        let mut buf = Buffer::default();
        let mut all = Vec::new();
        for ch in s.chars() {
            let consumed = self.key(ch as u32, 0);
            self.pump(PUMP_MS);
            for sig in self.drain() {
                buf.apply(&sig);
                all.push(sig);
            }
            if !consumed {
                buf.insert(&ch.to_string());
            }
            self.report(&buf.text());
            self.pump(PUMP_MS);
            for sig in self.drain() {
                buf.apply(&sig);
                all.push(sig);
            }
        }
        (buf.text(), all)
    }

    /// Type without reports, tracking the buffer from commits/forwards.
    pub fn type_buffered(&mut self, s: &str) -> (String, Vec<Sig>) {
        let mut buf = Buffer::default();
        let mut all = Vec::new();
        for ch in s.chars() {
            let consumed = self.key(ch as u32, 0);
            self.pump(PUMP_MS);
            for sig in self.drain() {
                buf.apply(&sig);
                all.push(sig);
            }
            if !consumed {
                buf.insert(&ch.to_string());
            }
        }
        (buf.text(), all)
    }

    pub fn stop(mut self) {
        // SIGTERM for a clean exit (the bus name is released by any exit, but
        // the engine's signal handlers log nothing on SIGKILL paths).
        unsafe {
            libc::kill(self.child.id() as libc::pid_t, libc::SIGTERM);
        }
        if self.child.wait().is_err() {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
        let _ = fs::remove_dir_all(&self.dir);
    }
}

// --- case table -------------------------------------------------------------

pub struct Case {
    pub name: &'static str,
    /// Flat `key = value` config lines; values are preformatted TOML.
    pub cfg: &'static [(&'static str, &'static str)],
    pub macros: &'static [(&'static str, &'static str)],
    /// Extra verbatim TOML appended to config.toml (e.g. `[[app_rules]]`).
    pub raw: &'static str,
    /// dictionary.txt content (one word per line).
    pub dict: &'static str,
    pub f: fn(&mut Sess),
}

/// The whole feature as one `#[ignore]`d test: plain `cargo test` never spawns
/// an engine; `tests/e2e/run.sh` turns it on inside a private bus.
pub fn run_suite(cases: &[Case]) {
    let Some(bin) = env::var_os("MANGRS_E2E_BIN").map(PathBuf::from) else {
        println!("skip: MANGRS_E2E_BIN not set — run through tests/e2e/run.sh");
        return;
    };
    if env::var_os("MANGRS_E2E").is_none() {
        println!("skip: MANGRS_E2E not set — refusing to fire engines at the real session bus");
        return;
    }
    let filter = env::var("MANGRS_E2E_FILTER").ok();

    let mut failures: Vec<String> = Vec::new();
    let mut run = 0;
    for case in cases {
        if let Some(f) = &filter {
            if !case.name.to_lowercase().contains(&f.to_lowercase()) {
                continue;
            }
        }
        run += 1;
        let t0 = Instant::now();
        let mut s = Sess::spawn(&bin, case, run);
        let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            (case.f)(&mut s);
        }));
        s.stop();
        let secs = t0.elapsed().as_secs_f32();
        match outcome {
            Ok(()) => println!("[PASS] {} ({secs:.1}s)", case.name),
            Err(e) => {
                println!(
                    "[FAIL] {} ({secs:.1}s)\n       {}",
                    case.name,
                    panic_message(&e)
                );
                failures.push(case.name.to_string());
            }
        }
    }
    println!("\n{}/{} passed", run - failures.len(), run);
    assert!(failures.is_empty(), "failed cases: {}", failures.join("; "));
}

fn panic_message(e: &Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "panic (no message)".into()
    }
}
