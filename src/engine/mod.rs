//! Protocol-independent engine state machine.
//!
//! Translates IBus key events into a list of [`Action`]s. Nothing here knows
//! about D-Bus, so the whole layer is unit-testable.
//!
//! # Display modes
//!
//! * **Preedit** — the factory default. The application shows the
//!   composition inline with an underline. Each keystroke sends
//!   `UpdatePreeditText`; nothing is committed until the word ends.
//! * **Surrounding** — commit as you type, no underline: text is committed
//!   to the application immediately and rewritten with the best working
//!   delete transport. When the application reports surrounding text, the
//!   engine verifies the report against what it believes it committed and
//!   repairs drift (swallowed deletes, edits the application made).
//!
//! The removed NonPreedit mode was merged into Surrounding: it was the same
//! commit-as-you-type contract without the verification, and the mode
//! resolution walk already served both identically. Old `nonpreedit` config
//! keys and app rules alias to Surrounding.
//!
//! Per-application rules (`AppRule`) can force a mode for a matched client;
//! clients without a rule fall back to the built-in heuristic.

use std::time::{Duration, Instant, SystemTime};

use crate::bamboo_wrapper::{encode, BambooWrapper, Charset, Method, Outcome};
use crate::config::{AppRule, BackspaceToneConfig, Config, SuggestDisplayConfig};
use crate::debug_log::{self, brief};
use crate::emoji::UserEmoji;
use crate::macros::{Dictionary, Macros};
use crate::suggest::{merge_user_entries, SuggestTable};

mod helpers;
mod process;
use process::CandidateEnd;
mod surrounding;

#[cfg(test)]
mod backspace_probe;
#[cfg(test)]
mod digit_debug;
#[cfg(test)]
mod digit_probe2;
#[cfg(test)]
mod keys_workload_probe;
mod scoring;
#[cfg(test)]
pub(crate) mod tests;
#[cfg(test)]
mod x_probe;

use self::helpers::*;

/// Display mode, selected in the GUI/menu and persisted in the config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Mode {
    /// Show the composition inline until the word ends.
    // The enum's Default mirrors the config's factory default mode.
    #[default]
    Preedit,
    /// Commit as you type, no underline: every key commits immediately and
    /// the rewrite deletes ride the best working transport; with a
    /// surrounding-text report the engine also verifies the report and
    /// repairs drift. The removed NonPreedit mode shared this arm.
    Surrounding,
    /// Raw keys are forwarded while typing; the latin tail is erased with
    /// real Backspaces at word end and the Vietnamese word is committed.
    /// The removed Turbo mode (this without the app-autofill hold-back)
    /// shared this arm, the removed Select mode (whose Shift+Left
    /// select-and-erase became this Backspace erase) too, and the removed
    /// SmoothForward mode (the same contract on the ordered forward
    /// transport) as well: with a backend it types exactly this, and
    /// without one the walk's ForwardBackspace fallback keeps its
    /// no-`/dev/uinput` property.
    Flow,
    /// Non-preedit whose tail rewrite deletes by forwarding Backspace key
    /// events through IBus instead of `DeleteSurroundingText` — works in
    /// clients that drop the protocol delete and where `/dev/uinput` cannot
    /// reach (mutter-relayed Wayland). ibus-bamboo's ForwardKeyEvent I.
    /// The removed ForwardSelect mode (ForwardKeyEvent II, the select-then-
    /// erase contract) shared the forward transport; its apps land here.
    ForwardBackspace,
    /// Vietnamese processing off; macros still run, auto-capitalize does
    /// not (the gap analysis documented this drift — keep the comment true).
    Off,
}

/// What the first navigation key does while a preedit commit is pending on
/// a mutter-relayed context (the `nav_key_while_preedit` config key).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NavKeyWhilePreedit {
    /// Consume the key: the commit lands while the caret is still in place,
    /// the next press moves the caret.
    Swallow,
    /// Send the commit and the key in one reply (ibus-bamboo's sequence):
    /// the caret moves on the first press. The shipped default.
    #[default]
    Forward,
    /// Consume the key and re-emit it as ForwardKeyEvent signals after the
    /// commit; the ordered connection delivers the commit first.
    Replay,
}

impl NavKeyWhilePreedit {
    /// Tolerant parse like the GUI language value: an unknown value keeps
    /// the default instead of failing the whole config file. A saved
    /// `swallow` keeps swallowing.
    pub fn from_config_value(value: &str) -> Self {
        match value {
            "swallow" => Self::Swallow,
            "replay" => Self::Replay,
            _ => Self::Forward,
        }
    }
}

/// One thing the server must tell IBus to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Insert text at the cursor.
    Commit(String),
    /// Remove `chars` characters immediately before the cursor.
    DeleteBefore(u32),
    /// Remove `n` characters starting `offset` characters before the cursor.
    DeleteSurrounding { offset: i32, n: u32 },
    /// For clients proven to drop the protocol delete: press Backspace
    /// `backspaces` times (real key events through /dev/uinput) and commit
    /// `commit` afterwards. Backspaces delete from the cursor, so the
    /// common suffix inside the word is removed too and re-typed.
    RewriteByBackspace { backspaces: u32, commit: String },
    /// Replace the visible preedit string.
    UpdatePreedit { text: String, cursor: u32 },
    /// Clear the preedit string.
    HidePreedit,
    /// Let the application handle the key (return `false` from ProcessKeyEvent).
    Forward,
    /// The key was consumed; do nothing further.
    Consumed,
    /// Press Backspace `n` times through /dev/uinput, paced — the uinput
    /// modes' word-end rewrite. Emitted only when injection is possible
    /// (mode resolution guarantees it); the server falls back to the
    /// protocol delete if the device died mid-session.
    InjectBackspaces(u32),
    /// Press Backspace `n` times by forwarding the key events through IBus
    /// (`ForwardKeyEvent`) — the ForwardBackspace mode's delete transport.
    /// Unlike `/dev/uinput` this reaches every client, including
    /// mutter-relayed Wayland contexts; unlike the protocol delete it works
    /// in clients that drop `DeleteSurroundingText`.
    ForwardBackspaces(u32),
    /// Idle pause (ms) after a uinput rewrite, before the app sees more keys.
    PostDelay(u64),
    /// Emit CommitText immediately, bypassing the injected-echo deferral —
    /// the per-key batch's parked commit. Its deletes were injected and
    /// their echoes accounted when the batch armed; a normal Commit here
    /// would defer behind that stale count and land ~150 ms late.
    CommitNow(String),
    /// Show the suggestion strip with these candidate words — the full
    /// ≤25-item list for the in-flight word. The server renders it as an
    /// IBus lookup table (the app-rule picker's precedent) and pages it with
    /// its pager controls; the selection digits are intercepted while it is
    /// up. The word rides along for the box's input line: the inline modes
    /// have no preedit signal to convert, so the action is the only source
    /// of the word-in-flight there.
    UpdateCandidates { word: String, items: Vec<String> },
    /// Close the suggestion strip: the word ended, the table has nothing
    /// for the word, or the composition was dropped.
    HideCandidates,
}

pub struct Engine {
    wrapper: BambooWrapper,
    mode: Mode,
    /// Modes the user may select, in GUI/cycle order. Mode resolution never
    /// takes a hidden mode from the config or an app rule; capability
    /// fallbacks inside `effective_mode` are exempt so typing never breaks.
    visible_modes: Vec<Mode>,
    /// The focused application's name from sources beyond the IBus client
    /// string (the focus extension for Wayland apps, the X active window for
    /// X11) — used ONLY for rule matching. Transport decisions keep using
    /// `client`, so a relayed client keeps its mutter-safe delete handling
    /// even after it is named.
    app_name: Option<String>,
    /// IBus capabilities reported by the client, from `SetCapabilities`.
    caps: u32,
    /// True while a preedit string is on screen, so we only send `HidePreedit`
    /// when there is something to hide.
    preedit_visible: bool,
    /// `client` from `FocusInId`, e.g. `gtk4-im:chrome`. `None` before the
    /// first focus.
    client: Option<String>,
    /// Charset for text sent to the application.
    charset: Charset,
    /// Encoded text the engine believes sits right before the cursor in the
    /// application (the current word, or the macro/restore replacement).
    tail: String,
    /// Last `SetSurroundingText(text, cursor_pos)` from the client.
    surrounding: Option<(String, u32)>,
    /// The last repair delete sent, to recognize clients that swallow it.
    repair_delete: Option<(i32, u32)>,
    /// Last emitted character, for `--`, double space and auto-capitalization.
    last_char: Option<char>,
    /// True while the next word starts a sentence (after `.`, `!`, `?`,
    /// Enter — spaces in between keep the flag).
    capitalize_next: bool,
    /// The word that just started begins a sentence. The uinput modes only
    /// see the finished word, so the decision is taken at the first key.
    cap_word: bool,
    /// Set once the client has been proven to drop DeleteSurroundingText:
    /// the server then injects real Backspace keys instead of the signal.
    delete_dropped: bool,
    /// Delivery proof for the last rewrite delete: `(expected prefix length
    /// when the delete lands, expected when it is swallowed)`. The next ST
    /// report settles it — a report matching the swallowed shape proves the
    /// client drops rewrite deletes, and (without uinput) the following
    /// words leave delete-based rewriting. Skipped for pure deletions,
    /// where a stale report is indistinguishable from a swallowed one.
    awaiting_delete_proof: Option<(usize, usize)>,
    /// Whether /dev/uinput is usable — deletions for clients without
    /// surrounding-text data go through key injection instead.
    uinput_available: bool,
    /// True from the moment the resolution walk downgrades a Flow request
    /// to ForwardBackspace for lack of an injection backend. Every
    /// `effective_mode` run clears it first and re-sets it on a Flow
    /// downgrade, so it always describes the last resolution; the paced
    /// backend re-request reads it.
    flow_needs_backend: bool,
    /// One-time gate for the Flow fallback log line: once per session,
    /// not per resolution.
    flow_fallback_logged: bool,
    /// The selected injection backend is routed by the compositor itself
    /// (Libei / wlr virtual keyboard), so the relay ban on injection does
    /// not apply to it — see `deletes_allowed`.
    injection_relay_safe: bool,
    /// Backspace keys injected through uinput that will echo back as
    /// ProcessKeyEvent; those must be forwarded untouched.
    pending_injected: u32,
    /// Commits that must wait until their injected Backspaces have been
    /// forwarded to the client: `(flush_after_echoes, text)`. The injected
    /// keys travel through the compositor while a CommitText signal goes
    /// straight over D-Bus — without deferral the commit outruns the
    /// backspace and the echo then deletes the freshly committed word
    /// (seen as "the accent flashes and reverts").
    deferred: Vec<(u32, String)>,
    /// How many injected-backspace echoes have been forwarded so far; drives
    /// `deferred` flush thresholds.
    echoes_forwarded: u32,
    /// When the last batch of injections happened — echoes that have not
    /// come back within the stale window are treated as lost.
    last_inject: Option<Instant>,
    /// Commits parked until a wall-clock deadline. On a mutter relay the
    /// commit channel reaches the client a beat ahead of the key events,
    /// so a rewrite commit must be emitted past the last delete of its
    /// batch even though every echo has already been forwarded; the
    /// server parks them here and the main-loop tick emits them.
    timed_commits: Vec<(Instant, String)>,
    /// The per-key display's single in-flight batch: the parked commit
    /// whose text each new key REPLACES (coalescing). Kept apart from
    /// `timed_commits` so a macro/emoji park from another producer can
    /// never be clobbered by a per-key replace. `None` = no batch in
    /// flight; the next key starts one.
    per_key_park: Option<(Instant, String)>,
    /// What the application certainly holds while that batch is in flight:
    /// the tail minus the pending insert, as `per_key_coalesce` computed it
    /// when the batch armed (the screen state before the batch's deletes
    /// land). Read by the server's focus-bounce stash; meaningful only while
    /// `per_key_park` is `Some` (the getter gates on it).
    in_flight_landed: String,
    // --- settings, mirrors of the config file ---
    app_rules: Vec<AppRule>,
    auto_restore: bool,
    em_dash: bool,
    auto_capitalize: bool,
    double_space_period: bool,
    macro_enabled: bool,
    capitalize_macro: bool,
    macro_in_off_mode: bool,
    macro_expand_valid_words: bool,
    time_format: String,
    date_format: String,
    macros: Macros,
    dictionary: Option<Dictionary>,
    /// mtimes of the auxiliary files behind the fields above: apply_config
    /// runs on every focus change, so each file is reloaded only when it
    /// changed on disk.
    macros_mtime: Option<SystemTime>,
    dictionary_mtime: Option<SystemTime>,
    /// Preedit shown without the underline attribute.
    hide_underline: bool,
    /// Opt-in: protocol deletes to direct clients that advertise
    /// surrounding-text support without ever reporting their text.
    x11_delete_without_report: bool,
    /// Opt-in: keep Preedit for XIM clients, whose server draws the preedit.
    preedit_for_xim: bool,
    /// Experimental: navigation-key behavior while a preedit commit is
    /// pending (the server reads this per keystroke).
    nav_key_while_preedit: NavKeyWhilePreedit,
    /// Uinput pacing (ms): text/commit step, Backspace gap, post-rewrite idle.
    backspace_interval_ms: u64,
    post_delay_ms: u64,
    /// Custom lost-echo stale window (ms); the built-in default applies
    /// while the switch is off. Too small risks flushing the deferred
    /// commit over late echoes (lost characters); too large makes every
    /// word appear late in apps that never route injected keys back.
    echo_stale_enabled: bool,
    echo_stale_ms: u64,
    /// Commit a paused preedit word after `idle_commit_ms` of no keys.
    idle_commit_enabled: bool,
    idle_commit_ms: u64,
    /// How long the per-key batch stays parked before it lands. Plain
    /// config: the delay row stands alone in the Tuning tab, outside the
    /// experimental gate.
    flow_per_key_delay_ms: u64,
    /// How long the Backspace correction waits before rewriting the screen
    /// (ms); 0 means the first main-loop wake. Plain config: the delay row
    /// stands alone in the Tuning tab, outside the experimental gate.
    flow_backspace_restore_delay_ms: u64,
    /// The landed screen region before the current per-key word (`chào `
    /// while a stray `f` sits after it). The Backspace restore uses it to
    /// give a finished word back to the composition when the stray key is
    /// erased. Filled at every per-key word start, and only consulted by
    /// the Backspace path.
    landed_region: String,
    /// Whether the current per-key region has produced no composition change
    /// yet — a lone key over a finished word (`f`) is litter, not a word, so
    /// erasing it may restore the region behind it. A real word clears this
    /// on its first composing key.
    region_plain: bool,
    /// Screen correction waiting for the user's Backspace to land: the text
    /// the tracked region must read, once the main loop wakes. See
    /// `DEFERRED_FIX_MS` for why it cannot be emitted with the key.
    deferred_fix: Option<(Instant, String)>,
    /// The user's Backspace is physically down. Injected Backspaces sent while
    /// it is (the deferred fix included) never reach the application, so the
    /// fix waits for the release — the server reports it.
    backspace_down: bool,
    /// Raw-key count at the moment a key undid a mark. The restore only fires
    /// while nothing has been typed or deleted since: `undo_in_word` stays set
    /// for the whole word (the wrapper's poison guard), so without this every
    /// later Backspace looked like an undone-mark erase — measured live, it
    /// re-marked the word while the user was deleting it.
    undo_key_at: Option<usize>,
    /// The last character in `tail` is the key that finished the word (a
    /// digit, for the presets that end a word on one). The Backspace that
    /// erases it deletes a character; it is not an edit of the word under it.
    tail_ends_word: bool,
    /// How Backspace treats a tone-marked word; pushed into the wrapper per
    /// keystroke because the uinput modes force the unikey undo.
    backspace_tone: BackspaceToneConfig,
    /// When the last key reached the engine — the idle-commit clock. Only
    /// stamped while the feature is on, so the off path costs nothing.
    last_key_at: Option<Instant>,
    /// The word an idle commit just committed: `(raw keys, encoded word)`.
    /// The next word character re-enters the composition (un-commit with a
    /// protocol delete, replay the raw keys) so the word continues as if
    /// the pause never happened. Cleared by any terminator, backspace,
    /// focus loss or state reset.
    idle_committed: Option<(String, String)>,
    /// Display mode locked for the word being typed. The daemon flips
    /// capabilities 1ms apart around focus changes and not at all on quick
    /// refocus; re-resolving the mode mid-word flips preedit↔non-preedit
    /// under the user's hands (duplicated or invisible text). Locked at the
    /// first key of a word, released when the word ends.
    word_mode: Option<Mode>,
    /// The word being typed follows a URL/email/path character: keep it raw
    /// until the next terminator (URL/email/path bypass).
    url_word: bool,
    /// The bypass was opened by real URL evidence (`://` or `www.`) — the
    /// whole address stays raw until a sentence boundary (see
    /// `URL_SPAN_KEEP`). False is the per-word bypass after `/ @ . _`.
    url_span: bool,
    /// The word that just ended was scheme-like and its terminator was `:`:
    /// the next `/` completes `://` and opens the URL span.
    scheme_pending: bool,
    /// The word that just ended was `www` and its terminator was `.`: the
    /// next word character opens the URL span.
    www_pending: bool,
    /// Keep words raw after URL/email/path characters (`bypass_url`).
    bypass_url: bool,
    /// The filtered `extra_url_chars` — empty unless the switch is on
    /// (see `apply_config` for the filter).
    extra_url_chars: Vec<char>,
    /// The filtered `auto_capitalize_after` — empty unless the switch is
    /// on (same filter shape as `extra_url_chars`, without the `-` ban: a
    /// hyphen trigger is legitimate here).
    extra_capitalize_after: Vec<char>,
    /// Emoji shortcodes on: a word after `:` composes like any word and a
    /// closing `:` rewrites `:name:` to the emoji (see `emoji_close_rewrite`).
    emoji_enabled: bool,
    /// The word being typed is an emoji shortcode candidate (`:name:`). The
    /// word composes like any word; the flag only matters at its end — the
    /// closing ':' looks the raw keys up and rewrites the shown text to the
    /// emoji, any other end (or erasing the word) just drops the flag.
    emoji_word: bool,
    /// The user's emoji table, reloaded by mtime like the macros.
    emojis: UserEmoji,
    emoji_mtime: Option<SystemTime>,
    /// Tail char count when the current raw-forward word armed — the
    /// `url_word`/`emoji_word` span is everything pushed after it. A
    /// Backspace that erases the span releases the bypass (terminators were
    /// the only release before, so deleting a mis-typed word after `:` left
    /// typing raw until space/enter). The per-key tail spans whole words
    /// (`x:abc`), so an empty tail is not the signal — the arm point is.
    raw_word_start: usize,
    /// The app appended suggestion text after the caret mid-word (Chrome's
    /// inline autocomplete). Its rewrite delete is dropped and repairs crash
    /// mutter, so show the following words as preedit — no delete needed.
    suggestions: bool,
    /// Cached `client_rule_mode` result keyed on the name pair that produced
    /// it. Resolution lowercases the pattern and the target and the key path
    /// asks for the mode more than once per keystroke; the cache turns that
    /// into two string compares. The key covers `app_name`/`client` changes
    /// by itself; `apply_config` clears it because the rules changed.
    rule_cache: Option<(Option<String>, Option<String>, Option<Mode>)>,
    /// Suggestion strip (experimental `suggest_words`): a candidate list is
    /// currently on the client's screen. Gates the hide-on-word-end paths
    /// and makes the accept path a no-op once the strip is gone.
    suggest_open: bool,
    /// Whether the pick in flight lands its trailing space — the normal pick
    /// does, the `` ` `` key does not (see `accept_candidate_tight`).
    pick_space: bool,
    /// The strip's display choice: the word-in-flight shows only in the
    /// floating box (default), in both places (mirror), or only inline
    /// beside the table.
    suggest_display: SuggestDisplayConfig,
    /// The word the last inline word end or accepted candidate committed —
    /// the context the strip's next round keys on (phase 2). Cleared with
    /// the rest of the word state on focus loss / reset.
    last_committed_word: Option<String>,
    /// The word before `last_committed_word` — the pair (this, last) is the
    /// trigram context the state-B scorer keys on. Shifts with every new
    /// context word, clears alongside it.
    prev_committed_word: Option<String>,
    /// The `suggest_words` switch, mirrored like the other settings — the
    /// one strip switch: the model's compounds, single words and next-word
    /// lists all serve under it, the dictionary being out of the strip.
    suggest_words: bool,
    /// Pick learning, baked with `suggest_words` under the experimental
    /// gate — picks land in the config dir's TSV files and buy an in-band
    /// bonus only while that is on.
    suggest_learn: bool,
    /// The pick files' home: the config dir, set at every config apply;
    /// tests point it at a tempdir so behavioral data never touches the
    /// owner's files.
    learn_dir: std::path::PathBuf,
    /// Pick-driven personalization state: `Some` while `suggest_learn` is
    /// baked on (recording + in-band bonus), `None` writes nothing.
    picks: Option<crate::learn::UserPicks>,
    /// The focused field's content purpose is PASSWORD/PIN — the strip
    /// never opens and nothing is learned there. Set by the server's
    /// ContentType handler, cleared on focus loss.
    password_quiet: bool,
    /// The prediction model opened at the last config apply: its compounds
    /// serve the state-A list and its n-grams the state-B surface; `None`
    /// (no model file) keeps the strip legacy-only. The Arc shares the
    /// process-wide cached parse (predict.rs) — tests construct hundreds of
    /// engines, and config reloads re-apply, so a fresh open per apply would
    /// rescan and resort the whole vocabulary every time.
    pub(crate) model: Option<std::sync::Arc<crate::predict::PredictModel>>,
    /// The table the strip serves from: the builtin rows (model-harvested
    /// or embedded) merged with the user dictionary at a weight that
    /// outranks every builtin frequency. Rebuilt on every config apply.
    suggest_table: SuggestTable,
}

/// `IBUS_CAP_PREEDIT_TEXT` and `IBUS_CAP_SURROUNDING_TEXT` from ibustypes.h.
/// A strip candidate as `(surface, pair, unigram)`: how often the model saw
/// it right after the word just committed, then how common it is on its own.
type Rated = (String, u32, u32);

const CAP_PREEDIT_TEXT: u32 = 1 << 0;

/// `GtkInputPurpose`/`IBusInputPurpose` values delivered through the
/// engine's ContentType property — PASSWORD and PIN close the strip and
/// the learning files; TERMINAL (10) keeps ordinary typing.
pub const PURPOSE_PASSWORD: u32 = 8;
pub const PURPOSE_PIN: u32 = 9;
const CAP_SURROUNDING_TEXT: u32 = 1 << 5;

/// Safety net for clients that are neither covered by an `AppRule` nor by the
/// config file but drop `DeleteSurroundingText`. Chromium's
/// `ui/gtk/input_method_context_impl_gtk.cc` connects only `commit` and
/// `preedit-*`, so a `DeleteSurroundingText` sent there is dropped and the
/// next commit appends: `tieengs` comes out as `tieêngs`. Electron inherits
/// the same code.
const FALLBACK_NO_DELETE_SURROUNDING: &[&str] = &[
    "chrome", "chromium", "electron", "code", "brave", "msedge", "vivaldi", "slack", "discord",
];

/// While a URL span is open (armed by a `://` scheme or `www.`), these word
/// terminators belong to the address and keep it raw; any other terminator
/// (space, Enter, `, ; ! " '` …) ends the span and typing composes again.
/// Only real URL evidence opens the span — no built-in character starts a
/// bypass by itself, so punctuation in prose never blocks marks. The
/// user's own `extra_url_chars` list is the only per-word trigger.
const URL_SPAN_KEEP: &[char] = &[
    '/', ':', '@', '.', '_', '~', '=', '#', '&', '%', '?', '-', '+',
];

/// RFC 3986 scheme shape: an ASCII letter, then letters, digits, `+`, `-`
/// or `.`. Every scheme passes so custom app links (`spotify://`,
/// `zoommtg://`) open the span too; in prose a colon is followed by a
/// space, not by `//`.
fn is_scheme_like(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic())
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
}

impl Engine {
    /// How long injected-key echoes may be missing before they count as
    /// lost and deferred commits flush anyway (ibus-bamboo's pacing
    /// ballpark). One shared value: the key handlers and the main-loop
    /// tick must agree or a lost echo holds the commit for different
    /// lengths of time — both read it through `echo_stale()`.
    pub const DEFAULT_ECHO_STALE_MS: u64 = 150;

    /// How long the server parks a rewrite commit after the last echo of
    /// its batch has been forwarded. Measured on a mutter relay (08/11):
    /// the app applies the forwarded Backspaces a beat behind the commit
    /// channel, and a commit emitted with the last echo overtook the
    /// final deletes (`titiến`). 60 ms clears the observed ~7 ms apply
    /// window with margin while staying imperceptible at word end.
    pub const ECHO_APPLY_GRACE_MS: u64 = 60;

    /// How long the Flow Backspace correction waits before rewriting the
    /// screen after the application applied the user's own Backspace.
    /// Backspaces the engine injects while that key is still being handled
    /// never reach the application (measured: 3 injected presses, no echo,
    /// only the commit landed), so the rewrite must wait until the physical
    /// key is long gone. Also the engine-side default of the config knob.
    pub const DEFERRED_FIX_MS: u64 = 150;

    pub fn new(method: Method, mode: Mode) -> Self {
        let mut engine = Self {
            wrapper: BambooWrapper::new(method, bamboo_core::Config::default()),
            mode,
            visible_modes: Vec::new(),
            caps: 0,
            preedit_visible: false,
            client: None,
            charset: Charset::Unicode,
            tail: String::new(),
            surrounding: None,
            repair_delete: None,
            last_char: None,
            app_name: None,
            app_rules: Vec::new(),
            auto_restore: true,
            em_dash: false,
            auto_capitalize: false,
            double_space_period: false,
            macro_enabled: true,
            capitalize_macro: true,
            macro_in_off_mode: false,
            macro_expand_valid_words: false,
            time_format: "%H:%M".into(),
            date_format: "%d/%m/%Y".into(),
            macros: Macros::load(),
            dictionary: None,
            macros_mtime: None,
            dictionary_mtime: None,
            hide_underline: false,
            x11_delete_without_report: false,
            preedit_for_xim: false,
            nav_key_while_preedit: NavKeyWhilePreedit::default(),
            delete_dropped: false,
            awaiting_delete_proof: None,
            uinput_available: false,
            flow_needs_backend: false,
            flow_fallback_logged: false,
            injection_relay_safe: false,
            pending_injected: 0,
            deferred: Vec::new(),
            echoes_forwarded: 0,
            last_inject: None,
            timed_commits: Vec::new(),
            per_key_park: None,
            in_flight_landed: String::new(),
            word_mode: None,
            url_word: false,
            url_span: false,
            scheme_pending: false,
            www_pending: false,
            bypass_url: true,
            extra_url_chars: Vec::new(),
            extra_capitalize_after: Vec::new(),
            emoji_enabled: false,
            emoji_word: false,
            raw_word_start: 0,
            emojis: UserEmoji::load(),
            emoji_mtime: None,
            suggestions: false,
            rule_cache: None,
            capitalize_next: false,
            cap_word: false,
            backspace_interval_ms: 5,
            post_delay_ms: 10,
            echo_stale_enabled: false,
            echo_stale_ms: Self::DEFAULT_ECHO_STALE_MS,
            idle_commit_enabled: false,
            idle_commit_ms: 700,
            flow_per_key_delay_ms: Self::ECHO_APPLY_GRACE_MS,
            flow_backspace_restore_delay_ms: Self::DEFERRED_FIX_MS,
            landed_region: String::new(),
            region_plain: false,
            deferred_fix: None,
            backspace_down: false,
            undo_key_at: None,
            tail_ends_word: false,
            backspace_tone: BackspaceToneConfig::ToneFirst,
            last_key_at: None,
            idle_committed: None,
            suggest_open: false,
            pick_space: true,
            suggest_learn: false,
            suggest_display: SuggestDisplayConfig::Mirror,
            last_committed_word: None,
            prev_committed_word: None,
            suggest_words: false,
            learn_dir: std::path::PathBuf::new(),
            picks: None,
            password_quiet: false,
            model: None,
            // The builtin rows stand in until apply_config merges the user
            // dictionary below; on a machine without dictionary.txt this
            // stays the whole table.
            suggest_table: SuggestTable::from_rows(crate::suggest::builtin_rows_with(None)),
        };
        engine.apply_config(&Config::default());
        // apply_config applies the config's method/mode; the caller wins here.
        engine.wrapper.set_method(method);
        engine.mode = mode;
        engine
    }

    /// Apply the on-disk config: input method, tone style, free marking,
    /// display features, macros, dictionary and per-app rules. Returns the
    /// actions needed to keep a visible composition alive: the rebuild and
    /// mode switch below swap the bamboo engine underneath it, so a pending
    /// word must be committed first or it silently disappears (config
    /// hot-reload or a menu change mid-word).
    pub fn apply_config(&mut self, config: &Config) -> Vec<Action> {
        let mut actions = Vec::new();
        let pending = self.wrapper.output().to_string();
        if self.preedit_commit_pending() {
            // Word-end path: a macro expands and an invalid word is restored,
            // exactly as on a terminator. No key is forwarded here.
            let raw = self.wrapper.raw().to_string();
            let valid = self.wrapper.is_valid();
            let valid_prefix = self.wrapper.is_valid_prefix();
            // A shortcode candidate word never word-macro-expands here.
            let candidate = if self.emoji_word {
                CandidateEnd::Word
            } else {
                CandidateEnd::None
            };
            let mut finished = self.on_finish(
                Mode::Preedit,
                pending.clone(),
                raw,
                valid,
                valid_prefix,
                candidate,
            );
            finished.retain(|a| !matches!(a, Action::Forward));
            actions.extend(finished);
            self.wrapper.reset();
            self.tail.clear();
            self.repair_delete = None;
            self.release_word_mode();
            // The in-flight word is gone: a strip left open would offer
            // candidates for text that just committed.
            actions.extend(self.suggest_step());
        }
        let (method, requested) = config.engine_settings();
        // A mode hidden in the GUI cannot stay selected: move to the first
        // mode the user still offers. Rules follow the same list (below).
        self.visible_modes = config.visible_modes().into_iter().map(Mode::from).collect();
        let mode = if self.visible_modes.contains(&requested) {
            requested
        } else {
            self.visible_modes.first().copied().unwrap_or(Mode::Preedit)
        };
        let mut rebuilt = self.wrapper.set_config(config.bamboo_config());
        rebuilt |= self.wrapper.set_method(method);
        if rebuilt {
            self.tail.clear();
        }
        self.app_rules = config.app_rules.clone();
        // The cached rule resolution belongs to the previous rule set.
        self.rule_cache = None;
        // The experimental master gate: with it off every experimental
        // feature is forced off here, no matter what its own switch says —
        // the switches and values stay saved for when the gate reopens.
        let experimental = config.experimental_features;
        self.auto_restore = config.auto_restore;
        self.em_dash = config.em_dash;
        self.auto_capitalize = config.auto_capitalize;
        self.extra_capitalize_after = if experimental && config.auto_capitalize_after_enabled {
            // Same filter shape as `extra_url_chars` minus the `-` ban; a
            // hyphen trigger is legitimate here.
            let mut chars: Vec<char> = Vec::new();
            for c in config.auto_capitalize_after.chars() {
                if c.is_alphanumeric() || c.is_whitespace() || chars.contains(&c) {
                    continue;
                }
                chars.push(c);
                if chars.len() >= 10 {
                    break;
                }
            }
            chars
        } else {
            Vec::new()
        };
        self.double_space_period = config.double_space_period;
        self.macro_enabled = config.macro_enabled;
        self.capitalize_macro = config.capitalize_macro;
        self.macro_in_off_mode = config.macro_in_off_mode;
        self.macro_expand_valid_words = experimental && config.macro_expand_valid_words;
        self.x11_delete_without_report = config.x11_delete_without_report;
        self.preedit_for_xim = config.preedit_for_xim;
        // The caret choice applies like every config value; a saved
        // `swallow` keeps swallowing.
        self.nav_key_while_preedit =
            NavKeyWhilePreedit::from_config_value(&config.nav_key_while_preedit);
        self.time_format = config.time_format.clone();
        self.date_format = config.date_format.clone();
        // Reload the auxiliary files only when they changed on disk (see the
        // mtime fields above).
        let macros_time = file_time(Macros::path().as_deref());
        if macros_time != self.macros_mtime {
            self.macros = Macros::load();
            self.macros_mtime = macros_time;
        }
        if config.dictionary_enabled {
            let t = file_time(Dictionary::path().as_deref());
            if self.dictionary.is_none() || t != self.dictionary_mtime {
                self.dictionary_mtime = t;
                self.dictionary = Some(Dictionary::load());
            }
        } else {
            self.dictionary = None;
            self.dictionary_mtime = None;
        };
        // The strip's table merges the user dictionary at a weight that
        // outranks every builtin frequency — independent of
        // `dictionary_enabled` (owner decision: the strip learns from the
        // same file even when the auto-restore whitelist is off). The
        // model opens once per config apply — a fresh open here is the
        // model's reload path (the file changes only at install) — and
        // feeds both the builtin rows and the predict tier; `None` keeps
        // the strip legacy-only. Rebuilt on every config apply, so a new
        // model or dictionary needs no engine restart.
        let model = crate::predict::locate_shared();
        let mut rows = crate::suggest::builtin_rows_with(model.as_deref());
        rows.extend(merge_user_entries(
            Dictionary::load()
                .words()
                .map(|w| (w.to_string(), 0))
                .collect(),
        ));
        self.suggest_table = SuggestTable::from_rows(rows);
        self.model = model;
        self.wrapper.set_abbreviations(config.abbreviations);
        // Experimental master gate: the keep stays off while the gate is
        // closed, whatever the row says (same bake-in as the other
        // experimental features).
        self.wrapper
            .set_keep_composed_words(experimental && config.keep_composed_words);
        self.wrapper
            .set_keep_repeats_raw(experimental && config.keep_repeats_raw);
        // The strip row lives behind "Bật thử nghiệm" like the keeps: gate
        // closed means the strip stays off, whatever the saved switch says.
        self.suggest_words = experimental && config.suggest_words;
        // Learning has no switch of its own: it rides the suggestion switch,
        // so an open strip always personalizes and a closed one touches
        // nothing.
        self.suggest_learn = self.suggest_words;
        self.learn_dir = dirs::config_dir()
            .map(|d| d.join("ibus-mangrs"))
            .unwrap_or_default();
        if self.suggest_learn {
            // Keep unflushed picks across a config reload; a fresh switch-on
            // loads the files once.
            let keep = self.picks.take();
            self.picks =
                Some(keep.unwrap_or_else(|| crate::learn::UserPicks::load(&self.learn_dir)));
        } else {
            // Off: picks made while the switch was on still count — flush,
            // then stop recording. Nothing is deleted (the GUI's clear
            // button owns that).
            if let Some(mut picks) = self.picks.take() {
                picks.flush();
            }
        }
        // Display-only choice: it shapes nothing while the strip is off, so
        // it rides without the gate bake-in.
        self.suggest_display = config.suggest_display;
        self.hide_underline = config.hide_underline;
        self.backspace_interval_ms = config.backspace_interval_ms;
        self.post_delay_ms = config.post_delay_ms;
        self.echo_stale_enabled = experimental && config.echo_stale_enabled;
        self.echo_stale_ms = config.echo_stale_ms;
        self.idle_commit_enabled = experimental && config.idle_commit_enabled;
        self.idle_commit_ms = config.idle_commit_ms;
        // The per-key park delay: plain config, no gate bake-in.
        self.flow_per_key_delay_ms = config.flow_per_key_delay_ms;
        // The Backspace correction's wait: plain config too — the retrack
        // and mark-restore it paces are standard behavior, not experiments.
        self.flow_backspace_restore_delay_ms = config.flow_backspace_restore_delay_ms;
        // The restore state describes one per-key word boundary; a reload can
        // land mid-word, so it starts clean.
        self.landed_region.clear();
        self.region_plain = false;
        self.deferred_fix = None;
        self.undo_key_at = None;
        self.backspace_tone = config.backspace_tone;
        if !self.idle_commit_enabled {
            self.idle_committed = None;
        }
        self.bypass_url = config.bypass_url;
        self.extra_url_chars = if experimental && config.extra_url_chars_enabled {
            // Defensive filter: `-` stays out of the start set (the `--`
            // em dash would break), letters/digits/spaces are not URL
            // context, and a hand-edited file must not smuggle in a huge
            // list — the hot path scans this vec linearly.
            let mut chars: Vec<char> = Vec::new();
            for c in config.extra_url_chars.chars() {
                if c == '-' || c.is_alphanumeric() || c.is_whitespace() || chars.contains(&c) {
                    continue;
                }
                chars.push(c);
                if chars.len() >= 10 {
                    break;
                }
            }
            chars
        } else {
            Vec::new()
        };
        self.emoji_enabled = config.emoji_enabled;
        self.emoji_word = false;
        self.url_word = false;
        self.url_span = false;
        self.scheme_pending = false;
        self.www_pending = false;
        self.raw_word_start = 0;
        let emoji_time = file_time(UserEmoji::path().as_deref());
        if emoji_time != self.emoji_mtime {
            self.emojis = UserEmoji::load();
            self.emoji_mtime = emoji_time;
        }
        self.set_mode(mode);
        self.set_charset(config.charset.into());
        // A reload can close the strip's gate (switch off, digit-tone
        // preset, a mode without a pick transport): a strip left open would
        // accept selections the gate no longer allows.
        if !self.suggest_active() && std::mem::take(&mut self.suggest_open) {
            actions.push(Action::HideCandidates);
        }
        actions
    }

    pub fn set_mode(&mut self, mode: Mode) {
        if mode != self.mode {
            self.wrapper.reset();
            self.tail.clear();
            self.idle_committed = None;
            self.mode = mode;
        }
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }

    pub fn set_method(&mut self, method: Method) {
        if self.wrapper.set_method(method) {
            self.tail.clear();
        }
    }

    pub fn set_charset(&mut self, charset: Charset) {
        if charset != self.charset {
            self.wrapper.reset();
            self.tail.clear();
            self.idle_committed = None;
            self.charset = charset;
        }
    }

    pub fn set_enabled(&mut self, enabled: bool) {
        self.wrapper.set_enabled(enabled);
    }

    pub fn is_enabled(&self) -> bool {
        self.wrapper.is_enabled()
    }

    pub fn set_capabilities(&mut self, caps: u32) {
        self.caps = caps;
    }

    pub fn has_surrounding_text_cap(&self) -> bool {
        self.caps & CAP_SURROUNDING_TEXT != 0
    }

    /// Whether the preedit string should be sent without the underline.
    pub fn hide_underline(&self) -> bool {
        self.hide_underline
    }

    /// Navigation-key behavior while a preedit commit is pending.
    pub fn nav_key_while_preedit(&self) -> NavKeyWhilePreedit {
        self.nav_key_while_preedit
    }

    /// The text we believe is committed right before the cursor.
    pub fn tail(&self) -> &str {
        &self.tail
    }

    /// The focused application identifier, if known (`gtk4-im:chrome`).
    pub fn client(&self) -> Option<&str> {
        self.client.as_deref()
    }

    /// Uinput pacing, from the config (ms between rewrite steps).
    pub fn backspace_interval_ms(&self) -> u64 {
        self.backspace_interval_ms
    }

    pub fn post_delay_ms(&self) -> u64 {
        self.post_delay_ms
    }

    /// The lost-echo stale window: the custom value when the switch is on,
    /// else the built-in default. The key handlers and the main-loop tick
    /// must read the SAME value or a lost echo holds the commit for
    /// different lengths of time — both go through this getter.
    pub fn echo_stale(&self) -> Duration {
        Duration::from_millis(if self.echo_stale_enabled {
            self.echo_stale_ms
        } else {
            Self::DEFAULT_ECHO_STALE_MS
        })
    }

    /// Whether the user enabled a custom lost-echo window: the server's
    /// burst watchdog derives its deadline from that window instead of
    /// the built-in grace.
    pub fn echo_stale_custom(&self) -> bool {
        self.echo_stale_enabled
    }

    /// The per-key display's park delay: the user value clamped to a
    /// sane ceiling. Zero is allowed by owner decision — a too-low value
    /// can only reorder text for the user who set it; the two server-side
    /// rewrite parks stay on the built-in constant and are not tunable.
    pub fn per_key_delay(&self) -> Duration {
        Duration::from_millis(self.flow_per_key_delay_ms.clamp(0, 500))
    }

    /// The client dropped the protocol delete at least once; the server
    /// should inject real key events for deletions from now on.
    pub fn delete_dropped(&self) -> bool {
        self.delete_dropped
    }

    /// Record that `n` Backspace keys were injected; their echoes must be
    /// forwarded, not processed.
    pub fn add_injected(&mut self, n: u32) {
        if n > 0 {
            self.pending_injected += n;
            self.last_inject = Some(Instant::now());
        }
    }

    /// True when injected backspaces have not echoed back within `max` — they
    /// are considered lost (apps that receive injected keys without routing
    /// them through ibus): the pending-echo counter is dropped or real
    /// Backspaces would be eaten as echoes forever, and any deferred commits
    /// become deliverable. Needs no deferred queue — a bare injection (e.g.
    /// Backspace while composing) leaves the counter stuck just the same.
    /// Idempotent: false when nothing is stale.
    pub fn take_stale_injection(&mut self, max: Duration) -> bool {
        let mut stale = false;
        if self.pending_injected > 0 {
            if let Some(t) = self.last_inject {
                if t.elapsed() > max {
                    debug_log::log(&format!(
                        "injection echoes stale ({}ms) → drop {} pending",
                        t.elapsed().as_millis(),
                        self.pending_injected
                    ));
                    self.pending_injected = 0;
                    self.last_inject = None;
                    stale = true;
                }
            }
        }
        stale
    }

    /// Consume one injected-backspace echo. True when the key is an echo and
    /// must go to the application untouched.
    pub fn take_injected_echo(&mut self) -> bool {
        if self.pending_injected > 0 {
            self.pending_injected -= 1;
            true
        } else {
            false
        }
    }

    /// Injected backspace echoes still expected to come back.
    pub fn pending_echoes(&self) -> bool {
        self.pending_injected > 0
    }

    /// Queue `text` to be committed only after `n` more injected-backspace
    /// echoes have been forwarded to the client (D-Bus signals and compositor
    /// key events have no shared ordering — see field docs).
    pub fn defer_commit(&mut self, n: u32, text: String) {
        let threshold = self.echoes_forwarded + n;
        debug_log::log(&format!(
            "defer commit after {n} echoes len={}",
            text.chars().count()
        ));
        self.deferred.push((threshold, text));
    }

    /// Record that one injected backspace was forwarded; returns commits whose
    /// Backspaces have all been delivered, in order.
    pub fn note_echo_forwarded(&mut self) -> Vec<String> {
        self.echoes_forwarded += 1;
        let mut ready = Vec::new();
        while self
            .deferred
            .first()
            .is_some_and(|(t, _)| self.echoes_forwarded >= *t)
        {
            let (_, text) = self.deferred.remove(0);
            debug_log::log(&format!(
                "flush deferred commit len={}",
                text.chars().count()
            ));
            ready.push(text);
        }
        ready
    }

    /// Drop the wait and return everything queued (echoes may have been
    /// lost — a focus change or a real key arriving means delivering late is
    /// better than never).
    pub fn flush_deferred(&mut self) -> Vec<String> {
        let mut all: Vec<String> = self
            .deferred
            .drain(..)
            .map(|(_, text)| {
                debug_log::log(&format!(
                    "flush deferred commit (forced) len={}",
                    text.chars().count()
                ));
                text
            })
            .collect();
        // A forced flush ignores the deadline: the context is going away
        // and nobody will tick for it — delivering late beats never.
        all.extend(self.timed_commits.drain(..).map(|(_, text)| {
            debug_log::log(&format!(
                "flush deferred commit (timed, forced) len={}",
                text.chars().count()
            ));
            text
        }));
        if let Some((_, text)) = self.per_key_park.take() {
            debug_log::log(&format!(
                "flush deferred commit (per-key, forced) len={}",
                text.chars().count()
            ));
            all.push(text);
        }
        all
    }

    pub fn has_deferred(&self) -> bool {
        !self.deferred.is_empty() || !self.timed_commits.is_empty() || self.per_key_park.is_some()
    }

    /// The commit texts still parked (deferred behind echoes, timed parks,
    /// the per-key insert), in park order. Read-only: the server's
    /// focus-bounce stash captures them without disturbing the delivery.
    pub fn parked_commits(&self) -> Vec<&str> {
        let mut texts: Vec<&str> = self.deferred.iter().map(|(_, t)| t.as_str()).collect();
        texts.extend(self.timed_commits.iter().map(|(_, t)| t.as_str()));
        if let Some((_, t)) = &self.per_key_park {
            texts.push(t);
        }
        texts
    }

    /// What the application certainly holds while the per-key batch is in
    /// flight — the state the focus-bounce stash anchors its reconcile on.
    /// `None` whenever no per-key batch is parked, so a value left over from
    /// a landed batch can never be read.
    pub fn in_flight_landed(&self) -> Option<&str> {
        self.per_key_park
            .is_some()
            .then_some(self.in_flight_landed.as_str())
    }

    /// Re-anchor the believed tail after the server repaired the screen
    /// outside the key path (the focus-bounce reconcile): the next word's
    /// coalesce diffs against this belief.
    pub fn reanchor_tail(&mut self, tail: String) {
        self.tail = tail;
    }

    /// Hold `text` back for `delay` before it may be emitted: the relay
    /// applies forwarded keys a beat behind the commit channel, so the
    /// rewrite commit waits past the deletes it depends on.
    pub fn park_commit(&mut self, text: String, delay: Duration) {
        debug_log::log(&format!(
            "park commit for {}ms len={}",
            delay.as_millis(),
            text.chars().count()
        ));
        self.timed_commits.push((Instant::now() + delay, text));
    }

    /// Parked commits whose deadline has passed, in park order.
    fn take_due_commits(&mut self) -> Vec<String> {
        let now = Instant::now();
        let (due, later): (Vec<_>, Vec<_>) = self
            .timed_commits
            .drain(..)
            .partition(|(deadline, _)| *deadline <= now);
        self.timed_commits = later;
        due.into_iter().map(|(_, text)| text).collect()
    }

    /// The per-key batch's parked commit, past its deadline. Emitted as
    /// `CommitNow`: the deferral counter counts this batch's own deletes,
    /// whose echoes have long been forwarded.
    fn take_due_per_key_commit(&mut self) -> Option<Action> {
        let now = Instant::now();
        match &self.per_key_park {
            Some((deadline, _)) if *deadline <= now => {
                let (_, text) = self.per_key_park.take().unwrap();
                Some(Action::CommitNow(text))
            }
            _ => None,
        }
    }

    /// Whether the server can inject keys (called once at startup).
    pub fn set_uinput_available(&mut self, available: bool) {
        self.uinput_available = available;
    }

    /// Test hook: pretend the app appended suggestion text after the caret.
    #[cfg(test)]
    pub fn set_suggestions(&mut self, suggestions: bool) {
        self.suggestions = suggestions;
    }

    /// Whether real Backspace keys can be injected as the delete transport.
    pub fn uinput_available(&self) -> bool {
        self.uinput_available
    }

    /// Whether the last resolution downgraded Flow for lack of an
    /// injection backend: every `effective_mode` run clears the flag
    /// first and re-sets it on a Flow downgrade (the paced backend
    /// re-request reads it).
    pub fn flow_needs_backend(&self) -> bool {
        self.flow_needs_backend
    }

    /// Mark the selected injection backend as compositor-routed (Libei /
    /// wlr virtual keyboard): the compositor itself routes those key
    /// events to the focused client, so the relay ban does not apply.
    pub fn set_injection_relay_safe(&mut self, safe: bool) {
        self.injection_relay_safe = safe;
    }

    /// Whether the selected injection backend is compositor-routed.
    pub fn injection_relay_safe(&self) -> bool {
        self.injection_relay_safe
    }

    /// Forget everything believed about the focused client's text buffer.
    ///
    /// Called on focus changes. The new context may not support surrounding
    /// text at all; keeping the previous client's data made the engine send
    /// protocol `DeleteSurroundingText` to a context that never reported a
    /// buffer, which segfaults mutter (the GNOME Shell search-bar logout).
    pub fn clear_buffer_state(&mut self) {
        self.surrounding = None;
        self.repair_delete = None;
        self.delete_dropped = false;
        self.suggestions = false;
        self.awaiting_delete_proof = None;
    }

    /// Deletion through the IBus protocol is only safe when the client has
    /// actually **reported** surrounding text in this focus session — mutter
    /// then knows the buffer. The capability bit is NOT usable as the signal:
    /// the daemon flips it (0b1001 ↔ 0b101001 in the logs) without the data
    /// flow changing, and on a quick unfocus/refocus it is not re-sent at
    /// all. A report, by contrast, proves the report channel works.
    pub fn surrounding_data_ok(&self) -> bool {
        self.surrounding.is_some()
    }

    /// True when the IBus client id names no application. Every XIM client is
    /// reported as the hard-coded `xim` (ibus-x11 creates all of its input
    /// contexts with that name — `client/x11/main.c`), so an app rule can only
    /// ever match it by accident; the compositor extension or `xprop` has to
    /// name those apps for rules to work.
    pub fn client_name_is_opaque(&self) -> bool {
        self.client.as_deref() == Some("xim")
    }

    /// Clients whose IBus context is owned by the compositor: every Wayland
    /// client is relayed by mutter and shows up as `gnome-shell`; `fake` is
    /// the daemon's hotkey context; `None` means no FocusInId arrived —
    /// indistinguishable from a relay, so treated as one.
    pub fn relay_context(&self) -> bool {
        match self.client.as_deref() {
            None => true,
            Some(c) => {
                let prgname = c.split_once(':').map_or(c, |(_, p)| p);
                prgname == "gnome-shell" || prgname == "mutter" || prgname == "fake"
            }
        }
    }

    /// Whether real Backspace keys may be injected. **Never for mutter-relayed
    /// contexts**: the injected keys do not reach the application there
    /// ("xin chaà" session — raw text kept, commit appended), while the
    /// protocol delete demonstrably works on that path. A `None` client is an
    /// XIM/XWayland app that never sent `FocusInId`; its keys and injected
    /// backspaces do arrive (Sublime Text, JetBrains XToolkit), so it is not
    /// lumped in with the relay. A compositor-routed backend is exempt: the
    /// compositor itself routes its key events, so they cannot be lost the
    /// way device-injected ones are (same exemption as `deletes_allowed`).
    pub fn may_inject(&self) -> bool {
        (!self.injection_forbidden() || self.injection_relay_safe)
            && (self.delete_dropped || self.surrounding.is_none())
    }

    /// Contexts where injected key events are known to be lost.
    fn injection_forbidden(&self) -> bool {
        match self.client.as_deref() {
            None => false,
            Some(c) => {
                let prgname = c.split_once(':').map_or(c, |(_, p)| p);
                prgname == "gnome-shell" || prgname == "mutter" || prgname == "fake"
            }
        }
    }

    /// Clients whose IBus context is owned by the compositor (mutter relays
    /// every Wayland client's text-input; the context id shows up as
    /// `gnome-shell`). Normal rewrite deletes work on this path, but a
    /// **repair** delete sent right after the application rewrote its own
    /// buffer segfaults mutter (coredump 02:37: Chrome omnibox autocomplete,
    /// shell dead 1.1s after the repair) — the compositor applies it against
    /// a buffer it has not caught up with. Repairs are skipped there.
    fn repair_delete_forbidden(&self) -> bool {
        self.relay_context()
    }

    /// Record the focused application (`gtk4-im:chrome`, ...) from `FocusInId`.
    pub fn set_client(&mut self, client: Option<String>) {
        // ibus-daemon reports its hotkey-only context as `fake` between two
        // real focus changes. It never receives typing, but focus can move on
        // to a context without surrounding text (GNOME Shell search) while
        // the last real client's data is still held — stale data re-enables
        // protocol deletes there. Keep the name for app rules, drop the rest.
        if client.as_deref() == Some("fake") {
            self.clear_buffer_state();
            return;
        }
        if client != self.client {
            self.wrapper.reset();
            self.tail.clear();
            self.clear_buffer_state();
            // A new client invalidates the previous app's name; the server
            // re-resolves it right after the focus lookup.
            self.app_name = None;
            self.client = client;
        }
    }

    /// Record a character the application received, updating the
    /// auto-capitalization window (`. `, `! `, `? `, Enter).
    fn note_char(&mut self, c: char) {
        self.last_char = Some(c);
        match c {
            '.' | '!' | '?' | '\n' | '\r' => self.capitalize_next = true,
            ' ' | '\t' => {}
            _ if !self.extra_capitalize_after.is_empty()
                && self.extra_capitalize_after.contains(&c) =>
            {
                self.capitalize_next = true;
            }
            _ => self.capitalize_next = false,
        }
    }

    /// Remember URL evidence from the word that just ended: a scheme-like
    /// word closed by `:` (the next `/` completes `://`) or the literal
    /// `www` closed by `.` opens a raw URL span on the next key (see the
    /// arming block in `process_char`). Both word-end paths feed it; any
    /// other terminator discards the evidence.
    fn note_url_pending(&mut self, raw: &str, c: char) {
        match c {
            ':' if is_scheme_like(raw) => self.scheme_pending = true,
            '.' if raw.eq_ignore_ascii_case("www") => self.www_pending = true,
            _ => {
                self.scheme_pending = false;
                self.www_pending = false;
            }
        }
    }

    /// The focused application's name from the focus extension or the X
    /// active window. Rule matching only — see the field docs.
    pub fn set_app_name(&mut self, name: Option<String>) {
        self.app_name = name;
    }

    /// The name set by `set_app_name`, for callers outside the engine (the
    /// app-rule picker needs the same target the rules will match against).
    pub fn app_name(&self) -> Option<&str> {
        self.app_name.as_deref()
    }

    /// Mode forced by an `AppRule` matching the focused client, if any.
    /// The rule target is `app_name` (the focus extension / X active window)
    /// when present, else the IBus client string. The special pattern `none`
    /// matches clients with neither — the only way to target them, since
    /// there is no name to substring-match.
    fn client_rule_mode(&mut self) -> Option<Mode> {
        // Cache hit without cloning: both name sources compare by reference.
        if let Some((cached_app, cached_client, mode)) = &self.rule_cache {
            if self.app_name.as_deref() == cached_app.as_deref()
                && self.client.as_deref() == cached_client.as_deref()
            {
                return *mode;
            }
        }
        let effective = self.app_name.as_deref().or(self.client.as_deref());
        let resolved = crate::config::app_rule_mode(&self.app_rules, effective)
            .map(Mode::from)
            .filter(|mode| self.visible_modes.contains(mode));
        self.rule_cache = Some((self.app_name.clone(), self.client.clone(), resolved));
        resolved
    }

    /// True when the focused app drops `DeleteSurroundingText`, so non-preedit
    /// rewriting would duplicate characters there.
    fn client_cannot_delete_surrounding(&self) -> bool {
        let Some(client) = self.client.as_deref() else {
            return false;
        };
        // `gtk4-im:<prgname>`; the prefix only says which toolkit glued us in.
        let prgname = client.split_once(':').map_or(client, |(_, p)| p);
        FALLBACK_NO_DELETE_SURROUNDING
            .iter()
            .any(|p| prgname.contains(p))
    }

    /// Delete-based rewriting is only safe once the client has actually
    /// reported surrounding text: mutter segfaults applying
    /// DeleteSurroundingText to a client whose buffer it never saw
    /// (g_utf8_offset_to_pointer out of bounds — the "logout while typing a
    /// tone mark" crash). Without a report the only alternative is key
    /// injection — and never on relayed contexts, where injected keys are
    /// lost. A compositor-routed backend (Libei / wlr virtual keyboard) is
    /// exempt from that ban: the compositor itself routes those events to
    /// the focused client, so the relay race cannot arise. Also several
    /// clients silently drop the protocol delete, which used to duplicate
    /// words.
    fn deletes_allowed(&self) -> bool {
        self.protocol_delete_ok()
            || (self.uinput_available && (!self.injection_forbidden() || self.injection_relay_safe))
    }

    /// Whether the protocol delete may be sent right now.
    ///
    /// A report is the proof that mutter knows the client's buffer, and a
    /// delete sent to a relayed context without one segfaulted the compositor
    /// (02:37, Chrome omnibox). A DIRECT client applies the delete in its own
    /// process instead (GTK clamps it), so with the opt-in the report is not
    /// required there — ibus-bamboo's surrounding mode deletes that way for
    /// every client, which is what makes X11 apps like Sublime work.
    pub fn protocol_delete_ok(&self) -> bool {
        self.surrounding_data_ok()
            || (self.x11_delete_without_report
                && self.caps & CAP_SURROUNDING_TEXT != 0
                && !self.relay_context())
    }

    /// Resolve the mode actually used for the focused client: the capability
    /// fallback walk. Rules feed `requested` like the configured mode does;
    /// the walk then serves the request with the best delete transport the
    /// client allows, ending at the forwarded Backspace transport that
    /// reaches every client. `&mut self` because the rule resolution is
    /// cached (see `client_rule_mode`).
    fn effective_mode(&mut self) -> Mode {
        // The flag describes the last resolution: a Flow downgrade here is
        // cleared by any later run that finds a backend.
        self.flow_needs_backend = false;
        let can_preedit = self.caps & CAP_PREEDIT_TEXT != 0;
        let ruled = self.client_rule_mode();
        let requested = ruled.unwrap_or(self.mode);
        if requested == Mode::Off {
            return Mode::Off;
        }
        if forwards_delete(requested) {
            return requested;
        }
        // Inline autocomplete: the app appended suggestion text after the
        // caret, and its rewrite delete is dropped (repairing crashes
        // mutter). Show the following words as preedit so no rewrite delete
        // is needed. Unruled clients only — a rule pins its mode.
        if ruled.is_none() && self.suggestions && can_preedit && self.relay_context() {
            return Mode::Preedit;
        }
        match requested {
            // Preedit is unusable when the client cannot render it — except
            // for XIM clients, which report no preedit capability even
            // though the XIM server draws the preedit itself
            // (`XIMPreeditPosition`: ibus-x11 shows it in its own window).
            // Downgrading them turned every word into raw keystrokes; the
            // opt-in keeps preedit, as ibus-bamboo does. Without the
            // capability the request walks on to the commit-as-you-type
            // contract below.
            Mode::Preedit => {
                if can_preedit || (self.preedit_for_xim && self.client_name_is_opaque()) {
                    Mode::Preedit
                } else {
                    self.commit_as_you_type_walk(can_preedit, ruled.is_none())
                }
            }
            // The commit-as-you-type contract: the walk picks the best
            // delete transport for the client.
            Mode::Surrounding => self.commit_as_you_type_walk(can_preedit, ruled.is_none()),
            // The uinput modes press real keys through /dev/uinput. Without
            // the virtual device the walk downgrades to the forwarded
            // transport and remembers it (the paced backend re-request
            // reads it).
            Mode::Flow => {
                if self.uinput_available {
                    Mode::Flow
                } else {
                    self.flow_needs_backend = true;
                    if !self.flow_fallback_logged {
                        self.flow_fallback_logged = true;
                        debug_log::log("flow without a backend → forward backspace");
                    }
                    Mode::ForwardBackspace
                }
            }
            // Terminal: carries its own delete transport, walks nowhere.
            Mode::ForwardBackspace => Mode::ForwardBackspace,
            // Unreachable: Off returned first. Kept for exhaustiveness.
            Mode::Off => requested,
        }
    }

    /// The commit-as-you-type contract served by the best working delete
    /// transport, in preference order. `unruled` gates the built-in
    /// delete-dropper list: a rule's whole point is to bypass that
    /// fallback (`config_rule_bypasses_the_delete_fallback`).
    fn commit_as_you_type_walk(&mut self, can_preedit: bool, unruled: bool) -> Mode {
        // Clients known to drop DeleteSurroundingText keep today's outcome:
        // the word shows inline instead of being duplicated.
        if unruled && can_preedit && self.client_cannot_delete_surrounding() {
            return Mode::Preedit;
        }
        // A report (or the direct-client opt-in) proves the protocol delete
        // safe, with drift repair as today. A client PROVEN to swallow
        // rewrite deletes skips it: the terminal transport below serves it.
        if self.protocol_delete_ok() && !self.delete_dropped {
            return Mode::Surrounding;
        }
        // No usable report yet: injected Backspaces serve when a backend
        // exists (the deletes_allowed gate decides per keystroke, unchanged)
        // — but only for a client that carries the word inline at all. A
        // caps-0 context (XIM, XWayland toolkits) renders no preedit and
        // reports nothing: the inline rewrite has no proof it lands there,
        // while the forwarded transport below reaches it by contract.
        if self.uinput_available && can_preedit {
            return Mode::Surrounding;
        }
        // Terminal: the forwarded transport reaches clients with no report
        // and no backend (mutter-relayed Wayland, XIM) — never preedit.
        Mode::ForwardBackspace
    }

    /// Mode for the current keystroke: the word-locked mode when a word is in
    /// flight, else the freshly resolved one (and lock it for this word).
    fn key_mode(&mut self) -> Mode {
        match self.word_mode {
            Some(m) => m,
            None => {
                let m = self.effective_mode();
                self.word_mode = Some(m);
                m
            }
        }
    }

    /// Release the word-mode lock (word ended or state reset).
    fn release_word_mode(&mut self) {
        self.word_mode = None;
    }

    /// The user's Backspace went up. Injected Backspaces are only delivered
    /// once this happened, so the deferred screen correction waits for it.
    pub fn note_backspace_released(&mut self) {
        self.backspace_down = false;
    }

    /// The Backspace was forwarded: the application's buffer lost its last
    /// character. The per-key features keyed on `last_char` (em-dash,
    /// double space, the emoji/URL arms) must not fire on the deleted
    /// character — their pair-rewrite would delete real text in front of it
    /// ("a-" BS "-" turned the whole "a-" into "—"). `tail` mirrors the
    /// screen for the raw forwarders, so its last char is the new context;
    /// when it holds nothing of ours the context is unknown and stays empty.
    fn note_backspace_forwarded(&mut self) {
        self.last_char = self.tail.chars().last();
        self.tail_ends_word = false;
        self.capitalize_next = false;
        // The deleted character invalidates pending URL evidence too.
        self.scheme_pending = false;
        self.www_pending = false;
        // A Backspace that erased the whole raw-forward span kills the
        // bypass: terminators were the only release, so deleting a word
        // typed after `:` left every following letter raw until space/enter.
        // A partial delete keeps it — the word is still on screen.
        if (self.url_word || self.emoji_word) && self.tail.chars().count() <= self.raw_word_start {
            self.url_word = false;
            self.url_span = false;
            self.emoji_word = false;
            self.raw_word_start = 0;
        }
    }

    /// Backspace over the composition. Returns `None` when there is nothing
    /// composed, so the application handles the key itself.
    pub fn backspace(&mut self) -> Option<Vec<Action>> {
        // The key is physically down until the server reports the release.
        self.backspace_down = true;
        // The idle-commit clock, like process_char.
        if self.idle_commit_enabled {
            self.last_key_at = Some(Instant::now());
        }
        // A word in flight follows the word-mode lock: a mid-word mode
        // re-resolution (capability flip, inline suggestions) must not send
        // the Backspace down a different branch than the keys around it —
        // process_char locks for the same reason. With no composition there
        // is no lock, and resolving here must not create one.
        let mode = if self.wrapper.has_composition() {
            self.key_mode()
        } else {
            self.effective_mode()
        };
        // Vietnamese processing is off; the app owns the Backspace.
        if mode == Mode::Off {
            return None;
        }
        // The tone-skip belongs to committed-text transports: the uinput
        // modes show the raw keys, and a skipped tone would desync the
        // screen from the wrapper. They keep the unikey undo.
        self.wrapper.set_backspace_tone(if mode == Mode::Flow {
            BackspaceToneConfig::UnikeyWalk
        } else {
            self.backspace_tone
        });
        // Flow: the application owns the on-screen text under the per-key
        // display, so the Backspace passes through with a reverse-mapped
        // raw buffer — the user's real key, the one transport the relay
        // never drops — and bamboo follows so the composition stays
        // tracked (unikey convention: Backspace undoes the last raw key).
        if mode == Mode::Flow {
            let mut actions = self.backspace_per_key();
            // The Backspace changed the screen word: the strip follows it
            // down (list for the shorter word, hide once it is gone) — the
            // same contract as the other modes' Backspace arms.
            actions.extend(self.suggest_step_after_erase());
            return Some(actions);
        }
        let Some(edit) = self.wrapper.backspace() else {
            // Nothing composed: the application deletes one of its own
            // characters, so the tracked tail has to shrink with it. Left
            // stale, the next macro rewrite deleted more characters than the
            // client held (an out-of-range delete, mutter aborts on it).
            self.tail = replace_tail(&self.tail, 1, "");
            self.note_backspace_forwarded();
            // The user is erasing an idle-committed word — re-entering it
            // would resurrect text they are deleting.
            self.idle_committed = None;
            return None;
        };
        if !self.wrapper.has_composition() {
            self.release_word_mode();
            // Erasing the whole candidate word drops the shortcode flag.
            self.emoji_word = false;
        }
        match mode {
            Mode::Preedit => {
                let text = self.preedit_text();
                let mut actions = if text.is_empty() {
                    self.preedit_visible = false;
                    vec![Action::HidePreedit]
                } else {
                    let cursor = text.chars().count() as u32;
                    vec![Action::UpdatePreedit { text, cursor }]
                };
                // The Backspace changed the in-flight word: the strip
                // follows it down (list for the shorter word, or hide once
                // nothing completes it).
                actions.extend(self.suggest_step_after_erase());
                Some(actions)
            }
            // Off is gated above. Keeps the match exhaustive.
            Mode::Off => None,
            Mode::Flow => None,
            Mode::Surrounding | Mode::ForwardBackspace => {
                if !self.deletes_allowed() && !forwards_delete(mode) {
                    // The delete cannot reach the application: forward the
                    // Backspace so the app deletes the last character itself.
                    // Without a transport the screen holds the raw keys (mark
                    // keys are raw-forwarded, so no tone can move), which is
                    // exactly what the wrapper's raw-key undo models.
                    debug_log::log("backspace forwarded (no delete transport)");
                    self.tail = replace_tail(&self.tail, 1, "");
                    self.note_backspace_forwarded();
                    // The word the strip is showing changed (or is gone): the
                    // step closes it when the gate no longer allows a pick.
                    let mut actions = vec![Action::Forward];
                    actions.extend(self.suggest_step_after_erase());
                    return Some(actions);
                }
                let mut actions = if self.charset == Charset::Unicode {
                    let mut actions = Vec::new();
                    if edit.delete_chars > 0 {
                        let insert_len = edit.insert.chars().count();
                        actions.push(self.delete_action(
                            mode,
                            self.delete_count(mode, edit.delete_chars as u32),
                            insert_len,
                        ));
                    }
                    if !edit.insert.is_empty() {
                        actions.push(Action::Commit(edit.insert.clone()));
                        self.tail = replace_tail(&self.tail, edit.delete_chars, &edit.insert);
                    } else {
                        self.tail = replace_tail(&self.tail, edit.delete_chars, "");
                    }
                    if actions.is_empty() {
                        actions.push(Action::Consumed);
                    }
                    actions
                } else {
                    self.rewrite(mode)
                };
                if !actions.is_empty() && actions != vec![Action::Consumed] {
                    if let Some(c) = self.tail.chars().last() {
                        self.note_char(c);
                    }
                }
                // The Backspace changed the in-flight word: the strip follows
                // it down (list for the shorter word, or hide once nothing
                // completes it) — the preedit arm's contract, inline flavor.
                actions.extend(self.suggest_step_after_erase());
                Some(actions)
            }
        }
    }

    /// Whether a preedit composition is pending that an editing key would
    /// commit right now. On relayed (mutter) contexts the commit and the
    /// forwarded key travel different transports, so a burst of arrow keys
    /// can move the caret before the commit lands — the word is then
    /// inserted mid-text ("em tên tùng" → "etùngm tên").
    pub fn preedit_commit_pending(&mut self) -> bool {
        // The visible preedit itself is what matters: capabilities can flip
        // mid-word and would otherwise make this report false while the word
        // is still on screen.
        !self.wrapper.output().is_empty()
            && (self.preedit_visible || self.effective_mode() == Mode::Preedit)
    }

    /// Commit whatever is composed and clear state. Used before the
    /// application sees a terminator or an editing key, and on a config
    /// reload. A client `Reset` does NOT come here: the preedit is sent with
    /// `IBUS_ENGINE_PREEDIT_COMMIT`, so the client committed the word itself
    /// before the Reset arrived (`on_client_reset`).
    pub fn reset(&mut self) -> Vec<Action> {
        // Decide before hiding: a visible preedit must be committed even when
        // a capability flip resolves the mode to non-preedit now.
        let commit_preedit = self.preedit_visible || self.effective_mode() == Mode::Preedit;
        let pending = self.wrapper.output().to_string();
        debug_log::log(&format!(
            "reset: preedit_visible={} pending={} commit_preedit={commit_preedit}",
            self.preedit_visible,
            !pending.is_empty()
        ));
        let mut actions = Vec::new();
        // In non-preedit modes the text is already in the application; only a
        // preedit composition still needs committing, through the word-end
        // path (macros expand, invalid words restore). Must run before
        // `drop_pending_word` resets the wrapper.
        if commit_preedit && !pending.is_empty() {
            let raw = self.wrapper.raw().to_string();
            let valid = self.wrapper.is_valid();
            let valid_prefix = self.wrapper.is_valid_prefix();
            // A shortcode candidate word never word-macro-expands here.
            let candidate = if self.emoji_word {
                CandidateEnd::Word
            } else {
                CandidateEnd::None
            };
            let mut finished = self.on_finish(
                Mode::Preedit,
                pending.clone(),
                raw,
                valid,
                valid_prefix,
                candidate,
            );
            finished.retain(|a| !matches!(a, Action::Forward));
            actions.extend(finished);
        }
        actions.extend(self.drop_pending_word(true));
        actions
    }

    /// Clear the composition and all word state WITHOUT committing anything.
    /// Used on a client `Reset` and a focus change, where the client has
    /// already committed the preedit itself (`IBUS_ENGINE_PREEDIT_COMMIT`).
    ///
    /// `hide_preedit` is false on a focus change: the client owns the preedit
    /// display there and clears it itself, and our own `HidePreedit` can arrive
    /// before the client's commit in clients that handle `FocusOut` first —
    /// it would wipe the very word they are about to commit. ibus-bamboo's
    /// `FocusOut` is a no-op for the same reason.
    pub fn drop_pending_word(&mut self, hide_preedit: bool) -> Vec<Action> {
        self.wrapper.reset();
        let mut actions = if hide_preedit {
            self.hide_if_visible()
        } else {
            let _ = std::mem::take(&mut self.preedit_visible);
            Vec::new()
        };
        self.tail.clear();
        self.landed_region.clear();
        self.region_plain = false;
        self.deferred_fix = None;
        self.undo_key_at = None;
        self.repair_delete = None;
        self.release_word_mode();
        self.url_word = false;
        self.url_span = false;
        self.scheme_pending = false;
        self.www_pending = false;
        self.emoji_word = false;
        self.raw_word_start = 0;
        self.idle_committed = None;
        self.prev_committed_word = None;
        self.last_committed_word = None;
        // The strip rides the in-flight word; the word is gone here, and a
        // strip left open would offer candidates for text that no longer
        // exists. The lookup-table hide, unlike the preedit one, is safe on
        // focus loss — the client does not own that display.
        if std::mem::take(&mut self.suggest_open) {
            actions.push(Action::HideCandidates);
        }
        actions
    }

    /// End the in-flight word because the app-rule picker is opening (the
    /// pick must never land mid-composition: apply_config on a live word
    /// desynced tail and wrapper — lost letters, duplicated words). Preedit
    /// clients hold nothing on the wire yet — the preedit is only a display —
    /// so the composed text commits here, exactly what is shown; inline
    /// clients already hold their raw characters, so only the bookkeeping
    /// drops and the screen keeps what it shows.
    pub fn end_word_for_picker(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        if self.preedit_visible {
            let text = self.preedit_text();
            if !text.is_empty() {
                actions.push(Action::Commit(text));
            }
        }
        actions.extend(self.drop_pending_word(true));
        actions
    }

    /// Bookkeeping for text the server commits outside the key path (the
    /// pickers' insertions): `last_char` and the capitalize window must
    /// follow what the application received, or the next word mis-reads
    /// the context — a pick right after a literal ':' would leave the emoji
    /// shortcode branch armed and the next word would type raw.
    pub fn note_committed(&mut self, text: &str) {
        if let Some(c) = text.chars().last() {
            self.note_char(c);
        }
    }

    /// The single gate every strip branch consults: Preedit mode (the strip
    /// is a preedit-mode feature), the switch on, and a preset whose digit
    /// keys are not tone keys — the server's selection digits must not
    /// fight the method's own marks.
    /// Whether the strip may open for a word served in `mode`: preedit owns
    /// its display (the client holds nothing on the wire — the pick commits
    /// over it), the inline modes hold the composed text in the application
    /// — a pick rewrites it, so the mode's own delete transport must be able
    /// to. Without one the strip never opens: a pick would duplicate the
    /// word the field already shows.
    fn strip_possible(&self, mode: Mode) -> bool {
        match mode {
            Mode::Preedit => true,
            Mode::Surrounding => self.deletes_allowed(),
            // Forwarded Backspace keys reach every client — that is the
            // mode's whole delete contract.
            Mode::ForwardBackspace => true,
            Mode::Flow => self.uinput_available,
            Mode::Off => false,
        }
    }

    fn suggest_active(&self) -> bool {
        self.suggest_words
            && !self.wrapper.method().uses_digit_tone_keys()
            && self.strip_possible(self.word_mode.unwrap_or(self.mode))
    }

    /// The context bonus for the in-flight word's prefix range: the last
    /// committed word's bigram rows whose follower sits inside the range,
    /// keyed by row id. A keyed row outranks the strangers of its band —
    /// typing `c` right after `xin ` keeps `chào` at the top of the c*
    /// rows, however frequent the others are. Empty without a model, a
    /// context word, or a context that is not a model token.
    fn context_pairs(
        &self,
        model: &crate::predict::PredictModel,
    ) -> std::collections::HashMap<u32, u32> {
        let Some(last) = self.last_committed_word.as_deref() else {
            return std::collections::HashMap::new();
        };
        let Some(id) = model.id_of(last) else {
            return std::collections::HashMap::new();
        };
        model.bigram_iter(id).collect()
    }

    /// The state-A candidate list for the in-flight word — the MODEL only,
    /// the dictionary having left the suggestion business entirely (its
    /// future home is spell-check): no table completions, no corrections,
    /// on any path. Page 1 has a fixed shape, so the digits `1`-`5` always
    /// mean the same thing:
    ///
    /// * slots 1-3 — single words: the model's VN syllables for the toneless
    ///   prefix (context bonus first, then unigram frequency), then its EN
    ///   rows on a pure-ASCII prefix (`hel` offers `hello`; `hè` excludes EN
    ///   outright);
    /// * slot 4 — the strongest compound, two or three words (`sinh` shows
    ///   `sinh viên`);
    /// * slot 5 — a keyword-matching emoji when the word has one, else the
    ///   second compound.
    ///
    /// Past slot 5 the leftovers follow in the same band order, for the
    /// strip's pages. The learning bonus (the personalization task) stays
    /// inside these bands too — it reorders within a band, never across one.
    /// One builder for the strip emit and the pick path, so the two can
    /// never drift apart: the pick indexes into the list the strip emitted.
    fn state_a_candidates(&self, word: &str) -> Vec<String> {
        let cap = crate::suggest::MAX_CANDIDATES;
        let (singles, compounds) = self.vn_material(word);
        let en = self.en_candidates(word);
        // A prefix the model knows nothing about is a mistyped word: the
        // embedded syllable table's distance-1 neighbours ("sinhh" finds
        // "sinh") fill the slots the model left empty. The table scan is the
        // only edit-distance work on the key path, so it runs only here —
        // and only when no band matched, or its near misses would bury the
        // rows the model does know (`hel` losing `hello` to `hè`).
        let corrections: Vec<(String, u32)> = if singles.is_empty()
            && compounds.is_empty()
            && en.is_empty()
            && word.chars().count() >= 3
        {
            crate::suggest::SuggestTable::embedded()
                .candidates_within(word, 1)
                .into_iter()
                .map(|candidate| (candidate.word, candidate.freq))
                .filter(|(candidate, _)| candidate != word)
                .collect()
        } else {
            Vec::new()
        };
        let mut emoji: Vec<String> = crate::emoji::glyphs_for_word(word)
            .into_iter()
            .take(crate::emoji::STRIP_EMOJI_CAP)
            .map(str::to_string)
            .collect();
        // The word slots in order of how common each candidate is: the
        // model's own counts decide, so a strong compound like `sinh viên`
        // outranks the rare `sính` spellings of the same prefix, and inside
        // its own band a common English word outranks a rare one. The VN
        // band stays ahead of the EN band: mixed by count alone, the short
        // prefixes drown in English (`h` offered he/have/has, no Vietnamese
        // at all in the probe).
        let mut words: Vec<Rated> = singles.into_iter().chain(compounds).collect();
        words.extend(
            corrections
                .into_iter()
                .map(|(surface, freq)| (surface, 0, freq)),
        );
        words.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.cmp(&b.0))
        });
        // The EN band keeps following the VN one (see above), ranked the same
        // way.
        words.extend(en);
        // Page 1: the most common words, the keyword emoji holding the last
        // slot. No padding — a thin prefix shows a short page instead of an
        // empty slot.
        let word_slots = if emoji.is_empty() {
            crate::suggest::STRIP_PAGE_SIZE
        } else {
            crate::suggest::STRIP_PAGE_SIZE - 1
        };
        let mut out: Vec<String> = Vec::new();
        let mut rest: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for (surface, _, _) in words {
            // One listing per surface: the same word reaches the list from
            // two bands or from the compound stream.
            if !seen.insert(surface.clone()) {
                continue;
            }
            if out.len() < word_slots {
                out.push(surface);
            } else {
                rest.push(surface);
            }
        }
        if !emoji.is_empty() {
            out.push(emoji.remove(0));
        }
        out.extend(rest);
        out.extend(emoji);
        out.truncate(cap);
        out
    }

    /// The model's VN material for the in-flight word as `(single words,
    /// compounds)`, each `Rated` and ranked: the pair count with the landed
    /// context first (typing `xin ch` puts `chào` on top, `196` pairs), the
    /// unigram then deciding the rest — with no context every pair is zero
    /// and commonness alone ranks the list. The prefix match alone filters
    /// language: VN syllables never start an English prefix (`hel` finds
    /// none) and vice versa.
    fn vn_material(&self, word: &str) -> (Vec<Rated>, Vec<Rated>) {
        let Some(model) = self.model.as_ref() else {
            return (Vec::new(), Vec::new());
        };
        let (start, end) = model.prefix_range(word);
        let pairs = self.context_pairs(model);
        let pair_of = |id: u32| pairs.get(&id).copied().unwrap_or(0);
        let mut singles: Vec<(u32, String, u32, u32)> = (start..end)
            .map(|row| row as u32)
            .filter(|&id| model.flags(id) & crate::predict::FLAG_VN_SYLLABLE != 0)
            .map(|id| {
                (
                    id,
                    model.surface(id).to_string(),
                    pair_of(id),
                    model.unigram(id),
                )
            })
            .filter(|(_, surface, _, _)| surface != word)
            // A landed word narrows the band to what the model actually saw
            // after it: no random neighbour of the typed prefix.
            .filter(|&(_, _, pair, _)| pairs.is_empty() || pair > 0)
            .collect();
        singles.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| b.3.cmp(&a.3))
                .then_with(|| a.1.cmp(&b.1))
        });
        let mut compounds: Vec<Rated> = model
            .compounds_for_prefix(&crate::suggest::toneless(word))
            .into_iter()
            .map(|(surface, strength)| {
                let pair = model.id_of(&surface).map_or(0, pair_of);
                (surface, pair, strength)
            })
            .collect();
        compounds.sort_by(|a, b| {
            b.1.cmp(&a.1)
                .then_with(|| b.2.cmp(&a.2))
                .then_with(|| a.0.cmp(&b.0))
        });
        (
            singles
                .into_iter()
                .map(|(_, surface, pair, unigram)| (surface, pair, unigram))
                .collect(),
            compounds,
        )
    }

    /// The EN band: the model's EN-word rows whose toneless key starts with
    /// the typed prefix, the typed word itself excluded. Empty unless the
    /// word is pure ASCII — a Vietnamese letter or tone mark anywhere
    /// excludes EN (`hè` never offers `hello`).
    fn en_candidates(&self, word: &str) -> Vec<Rated> {
        if !word.chars().all(|c| c.is_ascii_alphabetic()) {
            return Vec::new();
        }
        let Some(model) = self.model.as_deref() else {
            return Vec::new();
        };
        let (start, end) = model.prefix_range(&crate::suggest::toneless(word));
        let pairs = self.context_pairs(model);
        let mut rows: Vec<(u32, String, u32, u32)> = (start as u32..end as u32)
            .filter(|id| model.flags(*id) & crate::predict::FLAG_EN_WORD != 0)
            .map(|id| {
                (
                    id,
                    model.surface(id).to_string(),
                    pairs.get(&id).copied().unwrap_or(0),
                    model.unigram(id),
                )
            })
            .filter(|(_, surface, _, _)| surface != word)
            .filter(|&(_, _, pair, _)| pairs.is_empty() || pair > 0)
            .collect();
        // The same rule the VN band follows: the context pair leads, the rest
        // falls back to frequency with the learning bonus riding on it.
        let picks = self.picks.as_ref();
        let context = self.last_committed_word.as_deref();
        let score = |row: &(u32, String, u32, u32)| {
            row.3 as f64 + picks.map_or(0.0, |p| p.bonus(&row.1, context))
        };
        rows.sort_by(|a, b| {
            b.2.cmp(&a.2)
                .then_with(|| {
                    score(b)
                        .partial_cmp(&score(a))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.1.cmp(&b.1))
        });
        rows.into_iter()
            .map(|(_, surface, pair, uni)| (surface, pair, uni))
            .collect()
    }

    /// Where the strip's word-in-flight shows — the server reads it in the
    /// emit funnel to shape the preedit/auxiliary signals.
    pub fn suggest_display(&self) -> SuggestDisplayConfig {
        self.suggest_display
    }

    /// The focused field's IBus content purpose (the server's ContentType
    /// handler). Only the two hidden-entry purposes quiet the strip — and
    /// with it the learning path; typing itself composes exactly the same.
    pub fn set_content_purpose(&mut self, purpose: u32) {
        self.password_quiet = purpose == PURPOSE_PASSWORD || purpose == PURPOSE_PIN;
    }

    /// Whether the focused field is a password/PIN entry (server tests and
    /// debug logging read this; the engine itself consults the flag inline).
    pub fn password_quiet(&self) -> bool {
        self.password_quiet
    }

    /// The strip refresh after a key changed the in-flight word: emit the
    /// word's full candidate list while it has any, hide once the word is
    /// gone or the table offers nothing. Called only where the word really
    /// changed (the Edit/Finish arms, the Backspace path) — a key that
    /// leaves the word untouched must not re-emit and reset the server's
    /// page.
    /// The strip after a Backspace: it follows the shorter word down and
    /// closes once the box holds nothing. The landed-word list (state B) is
    /// the landing surface — reached after an erase it would offer next-word
    /// candidates under an empty input box (`xin ch` erased to nothing listed
    /// `lỗi`, `được`, `phép`).
    fn suggest_step_after_erase(&mut self) -> Vec<Action> {
        if self.wrapper.output().is_empty() {
            if std::mem::take(&mut self.suggest_open) {
                return vec![Action::HideCandidates];
            }
            return Vec::new();
        }
        self.suggest_step()
    }

    fn suggest_step(&mut self) -> Vec<Action> {
        // A password field: the strip never opens — and with it the accept
        // path (learning) is unreachable. Typing itself is untouched.
        if self.password_quiet {
            if std::mem::take(&mut self.suggest_open) {
                return vec![Action::HideCandidates];
            }
            return Vec::new();
        }
        if !self.suggest_active() {
            // The gate turned off while the strip is up (a config reload mid-
            // word): close it here so no stale strip survives to accept picks.
            if std::mem::take(&mut self.suggest_open) {
                return vec![Action::HideCandidates];
            }
            return Vec::new();
        }
        // The landed-word surface (nothing in flight) is the state-B
        // next-word list; from the FIRST typed letter on, the list is the
        // context+prefix fusion of state A. The old below-two-chars silence
        // was the dictionary's full-table scan guard — the model's bounded
        // range scan has no such degenerate case, and a frozen context list
        // that ignores the letter being typed reads as a hung strip.
        let word = self.wrapper.output();
        let state_b = word.is_empty();
        let items = if state_b {
            self.state_b_candidates(
                self.prev_committed_word.as_deref(),
                self.last_committed_word.as_deref().unwrap_or(""),
            )
        } else {
            self.state_a_candidates(&word)
        };
        if items.is_empty() {
            // Idle keys cost nothing: the hide fires only on the open→closed
            // edge, everything else is a no-op.
            if self.suggest_open {
                self.suggest_open = false;
                return vec![Action::HideCandidates];
            }
            return Vec::new();
        }
        self.suggest_open = true;
        // The box's input line: the word being typed, or — on the state-B
        // surface — the context word the predictions continue.
        let display_word = if state_b {
            self.last_committed_word.clone().unwrap_or_default()
        } else {
            word.clone()
        };
        vec![Action::UpdateCandidates {
            word: display_word,
            items,
        }]
    }

    /// The state-B next-word list for the pair `(prev, last)` — the fused
    /// stupid-backoff ranking of [`scoring::fused_state_b`]: the pair's
    /// trigram rows lead (or the last word's bigram rows when no pair is
    /// known), the last word's bigram rows back off at 0.4, and the model's
    /// unigram stream fills at 0.16. Surfaces only, capped. Empty when
    /// there is no model, the last word is not a model token, or nothing
    /// follows it — the strip then goes quiet.
    fn state_b_candidates(&self, prev: Option<&str>, last: &str) -> Vec<String> {
        let Some(model) = self.model.as_deref() else {
            return Vec::new();
        };
        let Some(id2) = model.id_of(last) else {
            return Vec::new();
        };
        let bigram_rows: Vec<(u32, u32)> = model.bigram_iter(id2).collect();
        let bigram_total: u32 = bigram_rows.iter().map(|(_, c)| c).sum();
        // The pair's trigram rows take the context seat when they exist;
        // otherwise the bigram rows are the context tier and no backoff
        // tier exists (the evaluator's one-token shape).
        let pair = prev
            .and_then(|w| model.id_of(w))
            .map(|id1| model.trigram_iter(id1, id2).collect::<Vec<(u32, u32)>>());
        let uni_factor = if pair.as_ref().is_some_and(|rows| !rows.is_empty()) {
            scoring::BACKOFF2
        } else {
            scoring::BACKOFF
        };
        let (ctx, backoff) = match &pair {
            Some(rows) if !rows.is_empty() => {
                let total: u32 = rows.iter().map(|(_, c)| c).sum();
                (
                    scoring::Tier {
                        rows,
                        total: total as f64,
                        factor: 1.0,
                    },
                    scoring::Tier {
                        rows: &bigram_rows,
                        total: bigram_total as f64,
                        factor: scoring::BACKOFF,
                    },
                )
            }
            _ => (
                scoring::Tier {
                    rows: &bigram_rows,
                    total: bigram_total as f64,
                    factor: 1.0,
                },
                scoring::Tier {
                    rows: &[],
                    total: 1.0,
                    factor: 0.0,
                },
            ),
        };
        let picks = self.picks.as_ref();
        let lift = |id: u32| -> f64 {
            picks.map_or(1.0, |p| {
                1.0 + scoring::LIFT_PAIR * p.pair_count(last, model.surface(id)) as f64
                    + scoring::LIFT_FREQ * f64::from(p.picks_count(model.surface(id)).unwrap_or(0))
            })
        };
        let ids = scoring::fused_state_b(
            ctx,
            backoff,
            scoring::UniStream {
                order: model.uni_order(),
                count: |id| model.unigram(id),
                total: model.uni_total() as f64,
                factor: uni_factor,
            },
            |x, y| model.surface(x).cmp(model.surface(y)),
            lift,
            crate::suggest::MAX_CANDIDATES,
        );
        ids.into_iter()
            .map(|id| model.surface(id).to_string())
            .collect()
    }

    /// Commit the strip candidate at `index` — the position in the FULL
    /// list the last `UpdateCandidates` carried (the server pages the list
    /// and maps page + label to this global index; the digit never reaches
    /// the engine, so the in-flight word — and the list — still match).
    /// Resets the composition through the same word-end state drop and
    /// records the candidate as the context word. A closed strip or an
    /// out-of-range index is a race between the server's strip state and
    /// the engine's (the word moved on between the last update and the
    /// key): commit nothing rather than a stale or wrong word — the caller
    /// forwards the key instead.
    /// The pick the `` ` `` key makes: the word lands with no trailing space,
    /// so the next `` ` `` types the character right after it. The strip
    /// closes instead of chaining — there is no separator for a next word.
    pub fn accept_candidate_tight(&mut self, index: usize) -> Vec<Action> {
        self.pick_space = false;
        let actions = self.accept_candidate(index);
        self.pick_space = true;
        actions
    }

    /// The picked word as it lands: charset-encoded, with the trailing space
    /// unless the pick asked for none.
    fn pick_text(&self, word: &str) -> String {
        let word = encode(self.charset, word);
        if self.pick_space {
            format!("{word} ")
        } else {
            word
        }
    }

    pub fn accept_candidate(&mut self, index: usize) -> Vec<Action> {
        if !self.suggest_open || self.password_quiet {
            return Vec::new();
        }
        let word = self.wrapper.output();
        if word.is_empty() {
            // State B: nothing is in flight — the strip holds the last
            // committed word's next-word list and the pick commits that
            // word wholesale, then chains the next context's list.
            return self.accept_state_b(index);
        }
        let Some(candidate) = self.state_a_candidates(&word).into_iter().nth(index) else {
            return Vec::new();
        };
        // Learning records the PICKED candidate — never the keystrokes —
        // with the ranking context (the last committed word) as the pair
        // key. A quiet (password) field never reaches this: the gate above.
        if let Some(picks) = self.picks.as_mut() {
            picks.record_pick(&candidate, self.last_committed_word.as_deref());
        }
        // The candidate replaces the word wholesale: no macro, no
        // auto-restore — exactly what the strip offered. The commit is
        // charset-encoded like every word end. A sentence-start word takes
        // the composition path's own capitalization decision (`cap_word`,
        // recorded at the word's first key), first character only.
        let word = if std::mem::take(&mut self.cap_word) {
            capitalize_first(&candidate)
        } else {
            candidate
        };
        let text = self.pick_text(&word);
        // The mode the word was served in decides the rewrite: preedit holds
        // nothing on the wire (drop and commit), the inline modes hold the
        // composed text in the application — the pick erases it through the
        // mode's own delete transport first, exactly like the inline word-end
        // rewrites (macro, emoji). Without a transport the commit would land
        // on top of the held word and duplicate it; the strip gate never
        // opens there, and a race is refused rather than committed.
        let mode = self.word_mode.unwrap_or(self.mode);
        if !self.strip_possible(mode) {
            return Vec::new();
        }
        let inline = mode != Mode::Preedit;
        // The erase length comes off the tail (the screen the transports
        // model) — `drop_pending_word` clears it, so read it first. Flow's
        // last batch may still be parked: land it before the erase so the
        // Backspaces count against the screen they model.
        let held = self.tail.chars().count();
        let parked = if mode == Mode::Flow {
            self.per_key_park.take()
        } else {
            None
        };
        let mut actions = self.drop_pending_word(true);
        if let Some((_, parked_text)) = parked {
            actions.push(Action::CommitNow(parked_text));
        }
        if inline && held > 0 {
            if mode == Mode::Flow {
                // Flow's transport is the injected Backspaces (see the
                // per-key coalesce), paced like every Flow rewrite.
                actions.push(Action::InjectBackspaces(held as u32));
                if self.post_delay_ms > 0 {
                    actions.push(Action::PostDelay(self.post_delay_ms));
                }
            } else {
                actions.push(self.delete_action(mode, held as u32, text.chars().count()));
            }
        }
        // The context word is the commit as it landed — like `on_finish`
        // recording the word as Finish carried it, capital included. The
        // space rides the commit (ibus intelligent pinyin's shape: a pick
        // ends the word and the next one starts fresh), so the context stays
        // the bare word for the next round's ranking.
        self.prev_committed_word = self.last_committed_word.take();
        self.last_committed_word = Some(word);
        // The candidate's last char (the space) is now on screen: keep the
        // capitalize window and `last_char` in step, like every committed
        // word — a space re-opens the em-dash/double-space windows.
        self.note_committed(&text);
        actions.push(Action::Commit(text));
        actions
    }

    /// The state-B pick: nothing is in flight, so the candidate commits
    /// wholesale — a plain commit, no delete transport exists or is needed —
    /// and becomes the next context word; the strip chains into the new
    /// context's predictions in place, hiding when the model offers nothing
    /// further.
    fn accept_state_b(&mut self, index: usize) -> Vec<Action> {
        let Some(context) = self.last_committed_word.clone() else {
            return Vec::new();
        };
        let Some(candidate) = self
            .state_b_candidates(self.prev_committed_word.as_deref(), &context)
            .into_iter()
            .nth(index)
        else {
            return Vec::new();
        };
        // The pair (context, picked) is exactly what the state-B bonus reads.
        if let Some(picks) = self.picks.as_mut() {
            picks.record_pick(&candidate, Some(&context));
        }
        let text = self.pick_text(&candidate);
        // The pick's trailing space re-opens the em-dash/double-space
        // windows like every committed word.
        self.note_committed(&text);
        let mut actions = vec![Action::Commit(text)];
        self.prev_committed_word = self.last_committed_word.take();
        self.last_committed_word = Some(candidate);
        if !self.pick_space {
            self.suggest_open = false;
            actions.push(Action::HideCandidates);
            return actions;
        }
        let items = self.state_b_candidates(
            self.prev_committed_word.as_deref(),
            self.last_committed_word.as_deref().unwrap_or(""),
        );
        if items.is_empty() {
            self.suggest_open = false;
            actions.push(Action::HideCandidates);
        } else {
            let word = self.last_committed_word.clone().unwrap_or_default();
            actions.push(Action::UpdateCandidates { word, items });
        }
        actions
    }

    /// The client `Reset` method. GTK fires it when the user clicks in a text
    /// field and on focus changes. The preedit is sent with
    /// `IBUS_ENGINE_PREEDIT_COMMIT`, so GTK has already committed the pending
    /// word to the field itself before this Reset arrives — committing again
    /// here would duplicate it. Only the internal composition is cleared,
    /// exactly like ibus-bamboo's `Reset`.
    pub fn on_client_reset(&mut self) -> Vec<Action> {
        debug_log::log("client reset → drop the pending word");
        self.drop_pending_word(true)
    }

    /// The client `FocusOut`/`Disable`/`FocusOutId`. The preedit is sent with
    /// `IBUS_ENGINE_PREEDIT_COMMIT`, so the client commits the pending word
    /// itself on the way out and clears its own preedit display; the engine
    /// only discards its internal composition — no commit, and no HidePreedit,
    /// which in clients that handle `FocusOut` before committing could arrive
    /// first and wipe the very word they are about to commit (ibus-bamboo's
    /// `FocusOut` is a no-op for the same reason).
    pub fn focus_out(&mut self) -> Vec<Action> {
        // The purpose belongs to the field we just left: without this, one
        // password field would quiet every later normal field until the
        // daemon happened to resend a ContentType.
        self.password_quiet = false;
        debug_log::log("focus-out → drop the pending word, no signals");
        self.drop_pending_word(false)
    }

    /// Server-loop tick: flush deferred commits whose injected-key echoes
    /// never came back — the uinput rewrite's commit would otherwise never
    /// land when the user simply stops typing (the word stays raw latin).
    /// With `idle_commit` on, a preedit word left alone past the configured
    /// pause commits here too, through the normal word-end path (macros,
    /// auto-restore, capitalization — exactly like a terminator); the next
    /// word character can re-enter the composition via `idle_committed`.
    pub fn tick(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        // Pick files flush on tick — the key path never writes — and
        // re-read when a file moved underneath us (external edit, the
        // GUI's clear button).
        if let Some(picks) = self.picks.as_mut() {
            picks.flush();
            picks.reload_if_changed();
        }
        // Parked commits past their deadline become deliverable here —
        // the main loop woke for exactly this deadline (ms_until_tick).
        for text in self.take_due_commits() {
            debug_log::log(&format!("timed commit flush len={}", text.chars().count()));
            actions.push(Action::Commit(text));
        }
        if let Some(action) = self.take_due_per_key_commit() {
            debug_log::log("timed commit flush (per-key)");
            actions.push(action);
        }
        // The Flow Backspace's correction: by now the user's own
        // Backspace has landed, so injected deletes reach the application —
        // but only once the key is UP. Injected Backspaces sent while it is
        // still down are dropped by the input stack (measured in the owner's
        // log: no echo, only the parked commit landed). A release the engine
        // never sees (focus loss) must not leave the fix hanging, so a long
        // overdue fire is allowed.
        if let Some((deadline, want)) = self.deferred_fix.clone() {
            let overdue =
                Instant::now() >= deadline + Duration::from_millis(Self::DEFERRED_FIX_MS * 4);
            if deadline <= Instant::now() && (!self.backspace_down || overdue) {
                self.deferred_fix = None;
                debug_log::log("deferred backspace fix: rewrite the region");
                actions.extend(self.per_key_coalesce(want));
            }
        }
        if self.take_stale_injection(self.echo_stale()) {
            for text in self.flush_deferred() {
                actions.push(Action::Commit(text));
            }
        }
        // The floor keeps a hand-edited 0 from committing between every
        // keystroke; the GUI range starts at 300 ms.
        let idle_ms = self.idle_commit_ms.max(50);
        if self.idle_commit_enabled
            && self.preedit_commit_pending()
            && self
                .last_key_at
                .is_some_and(|t| t.elapsed().as_millis() as u64 >= idle_ms)
        {
            let pending = self.wrapper.output().to_string();
            let raw = self.wrapper.raw().to_string();
            let valid = self.wrapper.is_valid();
            let valid_prefix = self.wrapper.is_valid_prefix();
            let candidate = if self.emoji_word {
                CandidateEnd::Word
            } else {
                CandidateEnd::None
            };
            let mut finished = self.on_finish(
                Mode::Preedit,
                pending.clone(),
                raw.clone(),
                valid,
                valid_prefix,
                candidate,
            );
            finished.retain(|a| !matches!(a, Action::Forward));
            self.wrapper.reset();
            self.tail.clear();
            self.repair_delete = None;
            self.release_word_mode();
            self.idle_committed = Some((raw, encode(self.charset, &pending)));
            debug_log::log(&format!("idle commit len={}", pending.chars().count()));
            actions.extend(finished);
            // The in-flight word is gone: close the strip with the commit
            // (the next word's first key would hide it only one key later).
            actions.extend(self.suggest_step());
        }
        actions
    }

    /// Milliseconds the main loop should wait before the next `tick` — the
    /// lost-echo stale window, the idle-commit deadline, or None when
    /// nothing is pending (the loop falls back to its 1 s poll).
    pub fn ms_until_tick(&self) -> Option<u64> {
        // The stale window wakes the loop for lost-echo flushes.
        let stale_ms = self.echo_stale().as_millis();
        let mut deadlines: Vec<u64> = Vec::new();
        if self.pending_injected > 0 {
            if let Some(t) = self.last_inject {
                deadlines.push(stale_ms as u64 - t.elapsed().as_millis().min(stale_ms) as u64);
            }
        }
        // The idle-commit deadline: a paused preedit word must wake the loop.
        if self.idle_commit_enabled && self.preedit_visible && !self.wrapper.output().is_empty() {
            if let Some(t) = self.last_key_at {
                let idle_ms = self.idle_commit_ms.max(50) as u128;
                deadlines.push(idle_ms.saturating_sub(t.elapsed().as_millis()).max(1) as u64);
            }
        }
        // The earliest parked-commit deadline must wake the loop for its
        // timed flush.
        if let Some((deadline, _)) = self.timed_commits.iter().min_by_key(|(d, _)| *d) {
            deadlines.push(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .max(1) as u64,
            );
        }
        if let Some((deadline, _)) = &self.per_key_park {
            deadlines.push(
                deadline
                    .saturating_duration_since(Instant::now())
                    .as_millis()
                    .max(1) as u64,
            );
        }
        if let Some((deadline, _)) = &self.deferred_fix {
            // While the Backspace is still down the fix cannot land; the
            // release event itself wakes the loop, so do not spin on it.
            if !self.backspace_down {
                deadlines.push(
                    deadline
                        .saturating_duration_since(Instant::now())
                        .as_millis()
                        .max(1) as u64,
                );
            }
        }
        deadlines.into_iter().min().map(|ms| ms.max(1))
    }

    fn hide_if_visible(&mut self) -> Vec<Action> {
        if std::mem::take(&mut self.preedit_visible) {
            vec![Action::HidePreedit]
        } else {
            Vec::new()
        }
    }
}
