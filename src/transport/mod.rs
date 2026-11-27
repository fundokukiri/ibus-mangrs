//! Injection backends: pressing real keys outside the IM channel.
//!
//! Every backend implements [`BackendOps`], whose method set is a hard
//! whitelist: Backspace and Delete. Injected keys enter the input pipeline
//! like the user's own keystrokes, so a bug in an injection path could type
//! text desktop-wide; the whitelist keeps that damage structural — no op
//! presses a letter or a digit out of the user's own key stream. The one
//! exception is [`BackendOps::type_text`], offered by the wlroots backend
//! only: it types the word a rewrite is replacing, through a keymap whose
//! entire alphabet is those few codepoints, so the residual damage is the
//! word being written twice. Text travels over IBus (CommitText) everywhere
//! else.
//!
//! [`Injection`] holds the selected backend. [`Injection::select`] walks a
//! preference list through [`constructors`] — led by a config-fed preferred
//! kind when one is set (`set_preferred`); a kind whose construction fails
//! is denied so it is not re-attempted on every keystroke, and
//! [`Injection::re_request`] clears the denials when the environment may
//! have changed — as does [`Injection::re_request_paced`] from the key
//! path, paced by `RE_REQUEST_COOLDOWN`.

use std::time::{Duration, Instant};

use crate::uinput::Uinput;

mod ei_sys;
mod libei;
mod libei_portal;
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
    /// Press Delete `n` times.
    fn delete(&self, n: u32) -> bool;
    /// Whether `type_text` can work at all here; cheap, unlike the call.
    fn supports_text(&self) -> bool {
        false
    }
    /// Type `text` as key events from this transport. Only the compositor
    /// backends can, and only for a whole rewrite: with the deletes on the
    /// same channel there is no ordering race left to wait out.
    fn type_text(&self, _text: &str) -> bool {
        false
    }
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
            // GNOME: an engine-held EI session shows the screen-sharing
            // indicator for its whole lifetime, and mutter 51 segfaults
            // (libeis NULL fn pointer) when an EI client vanishes — both
            // measured live on 15/11, twice logging the session out. Libei
            // stays registered but only the MANGRS_REQUIRE_LIBEI-gated unit
            // test and the experimental preferred-backend override reach
            // it here; it returns to this list by default only once an
            // indicator-free acquisition exists
            // and the crash is fixed; the kernel device covers GNOME fully.
            vec![Kind::Uinput]
        } else {
            // wlroots-family compositors (Hyprland, sway, ...) expose the
            // virtual-keyboard protocol without any device node. Mutter
            // exposes no such global, hence the arm above. Libei comes last:
            // on a portal-only compositor (KDE) it is the only rung that
            // works without the input group, but it costs a permission prompt
            // and a 5 s acquisition, so the two prompt-free transports are
            // tried first.
            vec![Kind::WlrVirtualKeyboard, Kind::Uinput, Kind::Libei]
        }
    } else if std::env::var_os("DISPLAY").is_some() {
        // X11 sessions: XTest first. Both transports deliver the same events,
        // and XTest needs no `/dev/uinput` access at all, so it works on a
        // stock account without the input group (the availability goal of R3;
        // the earlier measurement that the order does not change the echo race
        // still holds). The kernel device stays as the fallback for the X11
        // sessions XTest cannot serve.
        vec![Kind::XTest, Kind::Uinput]
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

/// Cooldown between the key path's paced re-requests: a cleared denial walks
/// the real constructor table, and those costs are asymmetric — the uinput
/// probe is a cheap device open, while a mutter/Libei acquisition runs whole
/// seconds — so a Flow word may pay for one at most this often.
const RE_REQUEST_COOLDOWN: Duration = Duration::from_secs(2);

/// The promoted preference list for `select`: the config's preferred kind
/// leads and the caller's list follows without the duplicate. `None` when no
/// preference is set or it already leads — the caller's slice goes to the
/// walk untouched.
fn promoted(prefer: &[Kind], preferred: Option<Kind>) -> Option<Vec<Kind>> {
    match preferred {
        Some(kind) if prefer.first() != Some(&kind) => {
            let mut list = Vec::with_capacity(prefer.len() + 1);
            list.push(kind);
            list.extend(prefer.iter().copied().filter(|k| *k != kind));
            Some(list)
        }
        _ => None,
    }
}

/// The selected backend plus the selection bookkeeping.
#[derive(Default)]
pub struct Injection {
    inner: Option<Box<dyn BackendOps>>,
    // Kinds whose construction failed, skipped by later selects so a broken
    // transport is not retried on every keystroke; `re_request` clears them.
    denied: Vec<Kind>,
    // Instant of the last constructor attempt (a select walk or a paced
    // clear): `re_request_paced` gates the key-path re-request on it.
    last_attempt: Option<Instant>,
    // The experimental config's preferred backend: `select` leads its walk
    // with it, the environment matrix follows as fallback. `None` — the
    // default — is the matrix verbatim.
    preferred: Option<Kind>,
}

impl Injection {
    /// No backend until `select` builds one.
    pub fn new() -> Self {
        Self::default()
    }

    /// Feed the config's preferred backend (startup and every config apply).
    /// `None` restores the environment matrix as-is.
    pub fn set_preferred(&mut self, preferred: Option<Kind>) {
        self.preferred = preferred;
    }

    /// The preference as fed, for the debug log's role word — a pure read.
    pub fn preferred(&self) -> Option<Kind> {
        self.preferred
    }

    /// The kind of the backend currently held, if any.
    pub fn current(&self) -> Option<Kind> {
        self.inner.as_ref().map(|backend| backend.kind())
    }

    /// Walk `prefer` through the real constructor table and keep the first
    /// backend that builds, led by the config's preferred kind when one is
    /// set. Keeps the previously held backend when every kind fails.
    pub fn select(&mut self, prefer: &[Kind]) -> Option<Kind> {
        match promoted(prefer, self.preferred) {
            None => self.select_with(prefer, &constructors),
            Some(list) => self.select_with(&list, &constructors),
        }
    }

    /// `select` with the constructor table injected — the seam that lets
    /// tests (and later wiring) stub construction.
    pub fn select_with(
        &mut self,
        prefer: &[Kind],
        ctor: &dyn Fn(Kind) -> Option<Box<dyn BackendOps>>,
    ) -> Option<Kind> {
        // Re-selection runs per focus-in and config apply; rebuilding a
        // healthy held backend pays the full acquisition and abandons the
        // live one behind the caller's back (15/11: one mutter session
        // dropped per focus change segfaulted mutter; the same churn on the
        // X11 fallback class rebuilt the X connection per focus). A dead
        // backend still walks, and a higher-preference kind still displaces
        // the held one when its constructor builds.
        if let Some(backend) = &self.inner {
            if backend.available() && prefer.first() == Some(&backend.kind()) {
                return Some(backend.kind());
            }
        }
        for &kind in prefer {
            if self.denied.contains(&kind) {
                continue;
            }
            if let Some(backend) = &self.inner {
                if backend.available() && backend.kind() == kind {
                    return Some(kind);
                }
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

    /// Clear the denials for the key path's re-request, PACED: unlike the
    /// config-apply `re_request` (rare, user-visible), this fires from
    /// `process_key` — and a cleared denial walks the real constructor
    /// table, whose costs are asymmetric (the uinput probe is a cheap
    /// device open, a mutter/Libei acquisition costs seconds) — so the
    /// denials may clear at most once per [`RE_REQUEST_COOLDOWN`]. Returns
    /// whether they cleared (the caller then re-selects).
    pub fn re_request_paced(&mut self) -> bool {
        let cooldown_over = match self.last_attempt {
            None => true,
            Some(at) => at.elapsed() > RE_REQUEST_COOLDOWN,
        };
        if cooldown_over {
            self.denied.clear();
            self.last_attempt = Some(Instant::now());
        }
        cooldown_over
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

    /// Whether the held backend can type a word by itself.
    pub fn supports_text(&self) -> bool {
        self.inner
            .as_ref()
            .is_some_and(|backend| backend.supports_text())
    }

    /// Type `text` through the held backend; `false` means the caller keeps
    /// the word on CommitText.
    pub fn type_text(&self, text: &str) -> bool {
        match &self.inner {
            Some(backend) => backend.type_text(text),
            None => false,
        }
    }

    pub fn backspaces(&self, n: u32, gap_ms: u64) -> bool {
        match &self.inner {
            Some(backend) => backend.backspaces(n, gap_ms),
            None => false,
        }
    }

    pub fn delete(&self, n: u32) -> bool {
        match &self.inner {
            Some(backend) => backend.delete(n),
            None => false,
        }
    }
}

/// Shared test fixture for the sibling backend test modules: `std::env` is
/// process-global and the runner threads tests together, so every test that
/// touches the session variables must hold this lock or a parallel restore
/// re-sets a variable mid-test (15/11: DISPLAY came back between an unset
/// and a probe and the XTest constructor reached the live display).
#[cfg(test)]
pub(crate) mod test_env {
    use std::sync::Mutex;

    /// Poison recovery keeps a panicking assert from blocking the suite.
    pub(crate) static ENV_LOCK: Mutex<()> = Mutex::new(());

    /// Restores every touched variable on drop, so a failing assert cannot
    /// leak session state into sibling tests.
    pub(crate) struct EnvGuard(Vec<(String, Option<String>)>);

    impl EnvGuard {
        /// Saves the current value of each named variable.
        pub(crate) fn save(names: &[&str]) -> Self {
            Self(
                names
                    .iter()
                    .map(|n| ((*n).to_string(), std::env::var(n).ok()))
                    .collect(),
            )
        }

        pub(crate) fn set(&self, name: &str, value: &str) {
            std::env::set_var(name, value);
        }

        pub(crate) fn unset(&self, name: &str) {
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
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    /// The injected actions, recorded by [`FakeBackend`]. `BackendOps` has
    /// no default methods, so a method added to the trait beyond these two
    /// fails to compile here — this enum is the whitelist in data form.
    #[derive(Debug, PartialEq)]
    enum RecordedOp {
        Backspace,
        Delete,
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

        fn delete(&self, _n: u32) -> bool {
            self.ops.lock().unwrap().push(RecordedOp::Delete);
            true
        }
    }

    #[test]
    fn trait_surface_is_the_whitelist() {
        // One call per trait method: the record must hold exactly the two
        // whitelisted ops, in trait order, and nothing else.
        let backend = FakeBackend::new(Kind::XTest);
        assert!(backend.available());
        assert!(backend.backspaces(2, 10));
        assert!(backend.delete(1));
        assert_eq!(
            *backend.ops.lock().unwrap(),
            vec![RecordedOp::Backspace, RecordedOp::Delete]
        );
    }

    #[test]
    fn pass_through_when_no_backend() {
        let injection = Injection::new();
        assert_eq!(injection.current(), None);
        assert!(!injection.backspaces(3, 10));
        assert!(!injection.delete(3));
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
        assert!(injection.backspaces(1, 0));
        // A second walk skips the denied kind, so WlrVirtualKeyboard is not
        // attempted again — and the healthy held Uinput is kept without a
        // rebuild (the churn the 15/11 fix stops).
        assert_eq!(
            injection.select_with(&[Kind::WlrVirtualKeyboard, Kind::Uinput], &stub),
            Some(Kind::Uinput)
        );
        assert_eq!(
            *attempts.lock().unwrap(),
            vec![Kind::WlrVirtualKeyboard, Kind::Uinput]
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

    #[test]
    fn re_request_paced_within_cooldown_is_a_no_op() {
        // The key path's re-request is paced by last_attempt: every
        // constructor attempt stamps it, so a Flow word right after a
        // failure must not clear the denials; once the stamp is older than
        // the cooldown the clear goes through and re-stamps, so a second
        // paced call within the cooldown builds at most once.
        let mut injection = Injection::new();
        let attempts = Mutex::new(Vec::new());
        let counting_ctor = |kind: Kind| -> Option<Box<dyn BackendOps>> {
            attempts.lock().unwrap().push(kind);
            Some(Box::new(FakeBackend::new(kind)))
        };
        // A failed walk denies the kind AND stamps last_attempt: the
        // paced clear right after is a no-op.
        let nothing = |_: Kind| -> Option<Box<dyn BackendOps>> { None };
        assert_eq!(injection.select_with(&[Kind::Libei], &nothing), None);
        assert_eq!(injection.denied, vec![Kind::Libei]);
        assert!(!injection.re_request_paced());
        assert_eq!(injection.denied, vec![Kind::Libei]);
        // A stamp older than the cooldown clears, and the next walk
        // retries the kind exactly once.
        injection.last_attempt =
            Some(Instant::now() - RE_REQUEST_COOLDOWN - Duration::from_millis(1));
        assert!(injection.re_request_paced());
        assert!(injection.denied.is_empty());
        assert_eq!(
            injection.select_with(&[Kind::Libei], &counting_ctor),
            Some(Kind::Libei)
        );
        assert_eq!(*attempts.lock().unwrap(), vec![Kind::Libei]);
        // The clear re-stamped: a second paced call within the cooldown is
        // a no-op — the denied constructor is not retried per keystroke.
        injection.inner = None;
        assert_eq!(injection.select_with(&[Kind::Libei], &nothing), None);
        assert_eq!(injection.denied, vec![Kind::Libei]);
        assert!(!injection.re_request_paced());
        assert_eq!(injection.select_with(&[Kind::Libei], &counting_ctor), None);
        assert_eq!(*attempts.lock().unwrap(), vec![Kind::Libei]);
    }

    #[test]
    fn unset_preferred_passes_the_callers_list_through() {
        // The pass-through is also the zero-allocation guarantee: with the
        // switch off, `select` hands the caller's slice to `select_with`
        // untouched.
        assert!(promoted(&[Kind::Uinput, Kind::XTest], None).is_none());
    }

    #[test]
    fn preferred_leads_and_the_matrix_follows_without_duplicates() {
        assert_eq!(
            promoted(&[Kind::Uinput, Kind::XTest], Some(Kind::XTest)),
            Some(vec![Kind::XTest, Kind::Uinput])
        );
        // A choice absent from the matrix is still honored — libei on GNOME.
        assert_eq!(
            promoted(&[Kind::Uinput], Some(Kind::Libei)),
            Some(vec![Kind::Libei, Kind::Uinput])
        );
    }

    #[test]
    fn a_choice_already_leading_the_matrix_is_a_pass_through() {
        // Leading with the choice must not rebuild a list per select call.
        assert!(promoted(
            &[Kind::WlrVirtualKeyboard, Kind::Uinput],
            Some(Kind::WlrVirtualKeyboard)
        )
        .is_none());
        assert_eq!(
            promoted(
                &[Kind::WlrVirtualKeyboard, Kind::Uinput],
                Some(Kind::Uinput)
            ),
            Some(vec![Kind::Uinput, Kind::WlrVirtualKeyboard])
        );
    }

    use super::test_env::{EnvGuard, ENV_LOCK};

    #[test]
    fn x11_session_prefers_xtest_then_uinput() {
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvGuard::save(&["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"]);
        _env.set("DISPLAY", ":0");
        _env.unset("WAYLAND_DISPLAY");
        _env.unset("XDG_CURRENT_DESKTOP");
        assert_eq!(preference_order(), vec![Kind::XTest, Kind::Uinput]);
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
            vec![Kind::WlrVirtualKeyboard, Kind::Uinput, Kind::Libei]
        );
    }

    #[test]
    fn gnome_wayland_keeps_libei_out_of_the_preference() {
        // DEVLOG 15/11: the mutter direct API delivered an EI keyboard in
        // 5.7 ms, but the live gate failed — the engine-held session shows
        // the screen-sharing indicator for its whole lifetime, and mutter
        // segfaults when an EI client vanishes (twice logged the session
        // out). Libei stays registered; the kernel device leads until the
        // indicator-free acquisition and the upstream crash are resolved.
        let _lock = ENV_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _env = EnvGuard::save(&["DISPLAY", "WAYLAND_DISPLAY", "XDG_CURRENT_DESKTOP"]);
        _env.unset("DISPLAY");
        _env.set("WAYLAND_DISPLAY", "wayland-0");
        _env.set("XDG_CURRENT_DESKTOP", "GNOME:Wayland");
        assert_eq!(preference_order(), vec![Kind::Uinput]);
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
    fn reselect_keeps_a_healthy_top_preferred_backend() {
        // Focus-in and config apply re-run select constantly; rebuilding the
        // held backend each time churns its acquisition (15/11: one mutter
        // session dropped per focus change). Only a preference change or a
        // dead backend may walk again.
        let mut injection = Injection::new();
        let attempts = Mutex::new(Vec::new());
        let counting_ctor = |kind: Kind| -> Option<Box<dyn BackendOps>> {
            attempts.lock().unwrap().push(kind);
            Some(Box::new(FakeBackend::new(kind)))
        };
        assert_eq!(
            injection.select_with(&[Kind::Uinput, Kind::XTest], &counting_ctor),
            Some(Kind::Uinput)
        );
        assert_eq!(
            injection.select_with(&[Kind::Uinput, Kind::XTest], &counting_ctor),
            Some(Kind::Uinput)
        );
        assert_eq!(*attempts.lock().unwrap(), vec![Kind::Uinput]);
        // The held kind dropped off the top of the preference: walk again.
        assert_eq!(
            injection.select_with(&[Kind::XTest], &counting_ctor),
            Some(Kind::XTest)
        );
        assert_eq!(*attempts.lock().unwrap(), vec![Kind::Uinput, Kind::XTest]);
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

    /// Test double whose availability flips at runtime — the dead-held
    /// backend shape the keep-guard must not protect. The flag is shared so
    /// the test can kill the held instance through its own handle.
    struct FlakyBackend {
        kind: Kind,
        alive: std::sync::Arc<std::sync::atomic::AtomicBool>,
    }

    impl FlakyBackend {
        fn fresh(kind: Kind) -> Self {
            Self {
                kind,
                alive: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            }
        }
    }

    impl BackendOps for FlakyBackend {
        fn kind(&self) -> Kind {
            self.kind
        }

        fn available(&self) -> bool {
            self.alive.load(std::sync::atomic::Ordering::Relaxed)
        }

        fn backspaces(&self, _n: u32, _gap_ms: u64) -> bool {
            true
        }

        fn delete(&self, _n: u32) -> bool {
            true
        }
    }

    #[test]
    fn dead_held_backend_is_replaced_by_the_walk() {
        // The plan's Task 8 Step 1 shape, pinned post-short-circuit: a held
        // backend whose available() flipped false must not keep its seat —
        // the walk rebuilds the same kind when it is the only preference and
        // a building higher preference displaces it. Constructor call
        // recording pins that a healthy held backend is kept without a
        // rebuild (the churn the 15/11 fix stops).
        use std::sync::atomic::{AtomicBool, Ordering};
        use std::sync::{Arc, Mutex};
        let mut injection = Injection::new();
        let calls = Mutex::new(Vec::new());
        let ctor = |kind: Kind| -> Option<Box<dyn BackendOps>> {
            calls.lock().unwrap().push(kind);
            Some(Box::new(FlakyBackend::fresh(kind)))
        };
        let held_alive = Arc::new(AtomicBool::new(true));
        injection.inner = Some(Box::new(FlakyBackend {
            kind: Kind::XTest,
            alive: Arc::clone(&held_alive),
        }));
        // Healthy held XTest reached after a denied higher preference:
        // kept, zero constructor calls (no churn).
        injection.denied.push(Kind::Uinput);
        assert_eq!(
            injection.select_with(&[Kind::Uinput, Kind::XTest], &ctor),
            Some(Kind::XTest)
        );
        assert!(calls.lock().unwrap().is_empty());
        injection.denied.clear();
        // Dead held XTest as the only preference: rebuilt, not kept.
        held_alive.store(false, Ordering::Relaxed);
        assert_eq!(
            injection.select_with(&[Kind::XTest], &ctor),
            Some(Kind::XTest)
        );
        assert_eq!(*calls.lock().unwrap(), vec![Kind::XTest]);
        // A building higher preference displaces the held one regardless of
        // its health (the rebuilt instance above is healthy again).
        assert_eq!(
            injection.select_with(&[Kind::Uinput, Kind::XTest], &ctor),
            Some(Kind::Uinput)
        );
        assert_eq!(*calls.lock().unwrap(), vec![Kind::XTest, Kind::Uinput]);
    }
}
