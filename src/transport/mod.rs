//! Injection backends: pressing real keys outside the IM channel.
//!
//! Every backend implements [`BackendOps`], whose method set is a hard
//! whitelist: Backspace, Shift+Left (selection), Delete and Ctrl+V (paste).
//! Injected keys enter the input pipeline like the user's own keystrokes, so
//! a bug in an injection path could type text desktop-wide; the whitelist
//! keeps that damage structural — no op presses a letter or a digit (`V`
//! appears only inside Ctrl+V, the clipboard channel). Text travels over
//! IBus instead (CommitText or the clipboard, see `paste.rs`).
//!
//! [`Injection`] holds the selected backend. [`Injection::select`] walks a
//! preference list through [`constructors`]; a kind whose construction fails
//! is denied so it is not re-attempted on every keystroke, and
//! [`Injection::re_request`] clears the denials when the environment may
//! have changed.

use std::time::Instant;

use crate::uinput::Uinput;

mod ei_sys;
mod libei;
mod uinput;
mod wlr_vk;
mod xtest;

/// An injection transport. The order of use is decided by preference lists,
/// not by this enum.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum Kind {
    /// Kernel `/dev/uinput` virtual keyboard (see `uinput.rs`).
    Uinput,
    /// X11 XTest fake input.
    XTest,
    /// wlroots `virtual-keyboard` protocol.
    WlrVirtualKeyboard,
    /// libei over mutter's direct RemoteDesktop API (measured 15/11; the
    /// portal path dead-ends for input-only sessions).
    Libei,
}

impl Kind {
    /// The name the debug log prints for the resolved backend.
    pub fn label(self) -> &'static str {
        match self {
            Kind::Uinput => "uinput",
            Kind::XTest => "xtest",
            Kind::WlrVirtualKeyboard => "wlr-virtual-keyboard",
            Kind::Libei => "libei",
        }
    }
}

/// One injection backend. The method set is the whitelist: exactly these
/// structural ops, never anything that could type text. Every op returns
/// `false` when it could not be done, so the caller can fall back to the
/// protocol path instead of silently dropping the deletion.
///
/// `Send` because the backend is held across the engine's threads.
pub trait BackendOps: Send {
    fn kind(&self) -> Kind;
    /// Whether the transport is usable right now.
    fn available(&self) -> bool;
    /// Press Backspace `n` times, `gap_ms` apart.
    fn backspaces(&self, n: u32, gap_ms: u64) -> bool;
    /// Select `n` characters to the left with Shift+Left, `gap_ms` apart.
    fn select_left(&self, n: u32, gap_ms: u64) -> bool;
    /// Press Delete `n` times.
    fn delete(&self, n: u32) -> bool;
    /// Press Ctrl+V (paste the clipboard).
    fn paste(&self) -> bool;
}

/// Builds the backend for `kind`; `None` means the kind cannot be used, and
/// `Injection::select` then denies it instead of re-attempting it on every
/// keystroke.
pub fn constructors(kind: Kind) -> Option<Box<dyn BackendOps>> {
    // Backends register themselves here as their modules land; a kind with
    // no backend yet is denied and the engine keeps the protocol path.
    match kind {
        Kind::Uinput if Uinput::available() => Some(Box::new(uinput::UinputBackend)),
        Kind::Uinput => None,
        Kind::XTest => xtest::construct(),
        Kind::WlrVirtualKeyboard => wlr_vk::construct(),
        Kind::Libei => libei::construct(),
    }
}

/// The session's injection backends, most preferred first — the environment
/// matrix of the rebuild design, with the GNOME row measured by the 15/11
/// spike (DEVLOG). Compositor-routed backends win where the compositor
/// offers them; the kernel device covers everything else.
pub fn preference_order() -> Vec<Kind> {
    if std::env::var_os("WAYLAND_DISPLAY").is_some() {
        if on_mutter() {
            // libei rides mutter's direct RemoteDesktop API: no device node,
            // no udev rule, no consent dialog; the kernel device stays as
            // the fallback (e.g. EI denied).
            vec![Kind::Libei, Kind::Uinput]
        } else {
            // wlroots-family compositors (Hyprland, sway, ...) expose the
            // virtual-keyboard protocol without any device node. Mutter
            // exposes no such global, hence the arm above.
            vec![Kind::WlrVirtualKeyboard, Kind::Uinput]
        }
    } else if std::env::var_os("DISPLAY").is_some() {
        // X11 sessions: the kernel device first, XTest where the device
        // node is unreachable (no input group, containerized).
        vec![Kind::Uinput, Kind::XTest]
    } else {
        // No session evidence at all (tests, exotic launches): the legacy
        // default rather than a guess.
        vec![Kind::Uinput]
    }
}

/// Whether the Wayland compositor is mutter/GNOME. The desktop-env check is
/// the cheap stable signal; probing `org.gnome.Mutter.RemoteDesktop` name
/// ownership would be stronger but costs a bus round trip per selection.
fn on_mutter() -> bool {
    std::env::var("XDG_CURRENT_DESKTOP")
        .map(|d| d.split(':').any(|c| c.eq_ignore_ascii_case("GNOME")))
        .unwrap_or(false)
}

/// The selected backend plus the selection bookkeeping.
#[derive(Default)]
pub struct Injection {
    inner: Option<Box<dyn BackendOps>>,
    // Kinds whose construction failed, skipped by later selects so a broken
    // transport is not retried on every keystroke; `re_request` clears them.
    denied: Vec<Kind>,
    // Retry pacing for the re-request wiring; read once that lands.
    #[allow(dead_code)]
    last_attempt: Option<Instant>,
}

impl Injection {
    /// No backend until `select` builds one.
    pub fn new() -> Self {
        Self::default()
    }

    /// The kind of the backend currently held, if any.
    pub fn current(&self) -> Option<Kind> {
        self.inner.as_ref().map(|backend| backend.kind())
    }

    /// Walk `prefer` through the real constructor table and keep the first
    /// backend that builds. Keeps the previously held backend when every
    /// kind fails.
    pub fn select(&mut self, prefer: &[Kind]) -> Option<Kind> {
        self.select_with(prefer, &constructors)
    }

    /// `select` with the constructor table injected — the seam that lets
    /// tests (and later wiring) stub construction.
    pub fn select_with(
        &mut self,
        prefer: &[Kind],
        ctor: &dyn Fn(Kind) -> Option<Box<dyn BackendOps>>,
    ) -> Option<Kind> {
        for &kind in prefer {
            if self.denied.contains(&kind) {
                continue;
            }
            self.last_attempt = Some(Instant::now());
            match ctor(kind) {
                Some(backend) => {
                    self.inner = Some(backend);
                    return Some(kind);
                }
                None => self.denied.push(kind),
            }
        }
        None
    }

    /// Forget the denials so the next `select` retries every kind. The
    /// config-apply re-select fires this: a reload is the user-visible
    /// "environment may have changed" moment, and a denied backend's
    /// constructor failure is fast (no service, no global), so re-attempting
    /// every kind there costs little on a rare event.
    pub fn re_request(&mut self) {
        self.denied.clear();
    }

    /// Clear one kind's denial so the next `select` retries it. The
    /// mid-session self-heal uses this for Uinput only: a logind ACL grant
    /// can land minutes after startup, and the uinput constructor is a
    /// cheap device probe — the other backends' failures are
    /// environment-fixed (no mutter, no compositor global, no DISPLAY), so
    /// retrying them per keystroke would pay their full acquisition cost
    /// for nothing.
    pub fn retry_denied(&mut self, kind: Kind) {
        self.denied.retain(|k| *k != kind);
    }

    pub fn backspaces(&self, n: u32, gap_ms: u64) -> bool {
        match &self.inner {
            Some(backend) => backend.backspaces(n, gap_ms),
            None => false,
        }
    }

    pub fn select_left(&self, n: u32, gap_ms: u64) -> bool {
        match &self.inner {
            Some(backend) => backend.select_left(n, gap_ms),
            None => false,
        }
    }

    pub fn delete(&self, n: u32) -> bool {
        match &self.inner {
            Some(backend) => backend.delete(n),
            None => false,
        }
    }

    pub fn paste(&self) -> bool {
        match &self.inner {
            Some(backend) => backend.paste(),
            None => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The injected actions, recorded by [`FakeBackend`]. `BackendOps` has
    /// no default methods, so a method added to the trait beyond these four
    /// fails to compile here — this enum is the whitelist in data form.
    #[derive(Debug, PartialEq)]
    enum RecordedOp {
        Backspace,
        ShiftLeft,
        Delete,
        Paste,
    }

    /// Test double: always available, records every injected op.
    struct FakeBackend {
        kind: Kind,
        ops: Mutex<Vec<RecordedOp>>,
    }

    impl FakeBackend {
        fn new(kind: Kind) -> Self {
            Self {
                kind,
                ops: Mutex::new(Vec::new()),
            }
        }
    }

    impl BackendOps for FakeBackend {
        fn kind(&self) -> Kind {
            self.kind
        }

        fn available(&self) -> bool {
            true
        }

        fn backspaces(&self, _n: u32, _gap_ms: u64) -> bool {
            self.ops.lock().unwrap().push(RecordedOp::Backspace);
            true
        }

        fn select_left(&self, _n: u32, _gap_ms: u64) -> bool {
            self.ops.lock().unwrap().push(RecordedOp::ShiftLeft);
            true
        }

        fn delete(&self, _n: u32) -> bool {
            self.ops.lock().unwrap().push(RecordedOp::Delete);
            true
        }

        fn paste(&self) -> bool {
            self.ops.lock().unwrap().push(RecordedOp::Paste);
            true
        }
    }

    #[test]
    fn trait_surface_is_the_whitelist() {
        // One call per trait method: the record must hold exactly the four
        // whitelisted ops, in trait order, and nothing else.
        let backend = FakeBackend::new(Kind::XTest);
        assert!(backend.available());
        assert!(backend.backspaces(2, 10));
        assert!(backend.select_left(3, 10));
        assert!(backend.delete(1));
        assert!(backend.paste());
        assert_eq!(
            *backend.ops.lock().unwrap(),
            vec![
                RecordedOp::Backspace,
                RecordedOp::ShiftLeft,
                RecordedOp::Delete,
                RecordedOp::Paste,
            ]
        );
    }

    #[test]
    fn pass_through_when_no_backend() {
        let injection = Injection::new();
        assert_eq!(injection.current(), None);
        assert!(!injection.backspaces(3, 10));
        assert!(!injection.select_left(3, 10));
        assert!(!injection.delete(3));
        assert!(!injection.paste());
    }

    #[test]
    fn select_walks_preferences_and_denies_failures() {
        let mut injection = Injection::new();
        let attempts = Mutex::new(Vec::new());
        // Stub table: WlrVirtualKeyboard cannot be constructed, Uinput can.
        let stub = |kind: Kind| -> Option<Box<dyn BackendOps>> {
            attempts.lock().unwrap().push(kind);
            match kind {
                Kind::Uinput => Some(Box::new(FakeBackend::new(kind))),
                _ => None,
            }
        };
        assert_eq!(
            injection.select_with(&[Kind::WlrVirtualKeyboard, Kind::Uinput], &stub),
            Some(Kind::Uinput)
        );
        assert_eq!(
            *attempts.lock().unwrap(),
            vec![Kind::WlrVirtualKeyboard, Kind::Uinput]
        );
        assert_eq!(injection.denied, vec![Kind::WlrVirtualKeyboard]);
        assert_eq!(injection.current(), Some(Kind::Uinput));
        // Pass-through now reaches the held backend.
        assert!(injection.paste());
        // A second walk skips the denied kind, so WlrVirtualKeyboard is not
        // attempted again.
        assert_eq!(
            injection.select_with(&[Kind::WlrVirtualKeyboard, Kind::Uinput], &stub),
            Some(Kind::Uinput)
        );
        assert_eq!(
            *attempts.lock().unwrap(),
            vec![Kind::WlrVirtualKeyboard, Kind::Uinput, Kind::Uinput]
        );
    }

    #[test]
    fn uinput_constructor_registers_when_device_available() {
        // The one test that reaches the real device: the constructor must
        // track `Uinput::available()` exactly, so a machine without
        // `/dev/uinput` gets `None` and keeps the protocol path.
        let device = crate::uinput::Uinput::available();
        let backend = constructors(Kind::Uinput);
        assert_eq!(backend.is_some(), device);
        if let Some(backend) = backend {
            assert_eq!(backend.kind(), Kind::Uinput);
            assert_eq!(backend.available(), device);
            // 0 presses emits no key events (the device itself cannot be
            // proven untouched here — `available` opens it).
            assert!(backend.backspaces(0, 0));
        }
    }

    #[test]
    fn re_request_clears_denials() {
        let mut injection = Injection::new();
        let allow = Mutex::new(false);
        let stub = |kind: Kind| -> Option<Box<dyn BackendOps>> {
            if *allow.lock().unwrap() {
                Some(Box::new(FakeBackend::new(kind)))
            } else {
                None
            }
        };
        assert_eq!(injection.select_with(&[Kind::Libei], &stub), None);
        assert_eq!(injection.denied, vec![Kind::Libei]);
        // While denied, the kind is skipped without another attempt and no
        // duplicate denial is recorded.
        assert_eq!(injection.select_with(&[Kind::Libei], &stub), None);
        assert_eq!(injection.denied, vec![Kind::Libei]);
        injection.re_request();
        assert!(injection.denied.is_empty());
        // The retried kind can now be constructed and held.
        *allow.lock().unwrap() = true;
        assert_eq!(
            injection.select_with(&[Kind::Libei], &stub),
            Some(Kind::Libei)
        );
        assert_eq!(injection.current(), Some(Kind::Libei));
    }

    /// Serializes the env-mutating preference tests: `std::env` is
    /// process-global and the runner threads tests together. Poison recovery
    /// keeps a panicking assert from blocking the rest of the suite.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Restores every touched variable on drop, so a failing assert cannot
    /// leak session state into sibling tests.
    struct EnvGuard(Vec<(String, Option<String>)>);

    impl EnvGuard {
        /// Saves the current value of each named variable.
        fn save(names: &[&str]) -> Self {
            Self(
                names
                    .iter()
                    .map(|n| ((*n).to_string(), std::env::var(n).ok()))
                    .collect(),
            )
        }

        fn set(&self, name: &str, value: &str) {
            std::env::set_var(name, value);
        }

        fn unset(&self, name: &str) {
            std::env::remove_var(name);
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (name, value) in &self.0 {
                match value {
                    Some(v) => std::env::set_var(name, v),
                    None => std::env::remove_var(name),
                }
            }
        }
    }

    #[test]
    fn x11_session_prefers_uinput_then_xtest() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvGuard::save(&["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"]);
        _env.set("DISPLAY", ":0");
        _env.unset("WAYLAND_DISPLAY");
        _env.unset("XDG_CURRENT_DESKTOP");
        assert_eq!(preference_order(), vec![Kind::Uinput, Kind::XTest]);
    }

    #[test]
    fn wayland_non_mutter_prefers_wlr_then_uinput() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvGuard::save(&["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"]);
        _env.unset("DISPLAY");
        _env.set("WAYLAND_DISPLAY", "wayland-0");
        _env.set("XDG_CURRENT_DESKTOP", "Hyprland");
        assert_eq!(
            preference_order(),
            vec![Kind::WlrVirtualKeyboard, Kind::Uinput]
        );
    }

    #[test]
    fn gnome_wayland_order_follows_the_spike_outcome() {
        // DEVLOG 15/11: the mutter direct API delivered an EI keyboard in
        // 5.7 ms with no dialog; libei leads, the kernel device is fallback.
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvGuard::save(&["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"]);
        _env.unset("DISPLAY");
        _env.set("WAYLAND_DISPLAY", "wayland-0");
        _env.set("XDG_CURRENT_DESKTOP", "GNOME:Wayland");
        assert_eq!(preference_order(), vec![Kind::Libei, Kind::Uinput]);
    }

    #[test]
    fn retry_denied_retries_only_one_kind() {
        // The self-heal clears the Uinput denial alone: the other backends'
        // failures are environment-fixed and their constructors are too
        // expensive to re-attempt per keystroke.
        let mut injection = Injection::new();
        let stub = |kind: Kind| -> Option<Box<dyn BackendOps>> {
            match kind {
                Kind::Uinput => Some(Box::new(FakeBackend::new(kind))),
                _ => None,
            }
        };
        assert_eq!(
            injection.select_with(&[Kind::Libei, Kind::Uinput], &stub),
            Some(Kind::Uinput)
        );
        // Drain: nothing available anywhere, so both end up denied.
        let nothing = |_: Kind| -> Option<Box<dyn BackendOps>> { None };
        injection.inner = None;
        assert_eq!(
            injection.select_with(&[Kind::Libei, Kind::Uinput], &nothing),
            None
        );
        assert_eq!(injection.denied, vec![Kind::Libei, Kind::Uinput]);
        injection.retry_denied(Kind::Uinput);
        assert_eq!(injection.denied, vec![Kind::Libei]);
        assert_eq!(
            injection.select_with(&[Kind::Uinput], &stub),
            Some(Kind::Uinput)
        );
    }

    #[test]
    fn backend_death_reselects_on_apply_config() {
        // The apply_config re-select walks the preferences again: a held
        // backend whose transport died is replaced by the next kind that
        // builds (the dead kind's ctor fails now). Server wiring: the
        // FocusIn/Enable apply_config arm calls `injection.select`.
        let mut injection = Injection::new();
        let xtest_ctor = |kind: Kind| -> Option<Box<dyn BackendOps>> {
            match kind {
                Kind::XTest => Some(Box::new(FakeBackend::new(kind))),
                _ => None,
            }
        };
        // Startup: the uinput device is not there, XTest builds.
        assert_eq!(
            injection.select_with(&[Kind::Uinput, Kind::XTest], &xtest_ctor),
            Some(Kind::XTest)
        );
        assert_eq!(injection.current(), Some(Kind::XTest));
        // Config reload after a uinput grant landed: the apply_config arm
        // clears the denials first (re_request), so the walk retries the
        // higher-preference kind over the held one.
        injection.re_request();
        let uinput_ctor = |kind: Kind| -> Option<Box<dyn BackendOps>> {
            match kind {
                Kind::Uinput => Some(Box::new(FakeBackend::new(kind))),
                _ => None,
            }
        };
        assert_eq!(
            injection.select_with(&[Kind::Uinput, Kind::XTest], &uinput_ctor),
            Some(Kind::Uinput)
        );
        assert_eq!(injection.current(), Some(Kind::Uinput));
    }
}
