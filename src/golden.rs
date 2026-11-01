//! Golden behaviour tests (TODO.MD Giai đoạn 0): a table of keystroke
//! sequences and the exact text the engine must leave in the application.
//!
//! Every row runs through the real `Engine` in non-preedit mode with Unicode
//! output and is replayed against a simulated application buffer, so a
//! failure means the on-screen text would be wrong, not just the internal
//! state. Three case variants per word: lowercase, Capitalized, UPPER.

use crate::bamboo_wrapper::Method;
use crate::engine::{Action, Engine, Mode};

/// (keystrokes, expected word) for Telex.
#[cfg(test)]
const TELEX: &[(&str, &str)] = &[
    ("tieengs", "tiếng"),
    ("vieejt", "việt"),
    ("dd", "đ"),
    ("nam", "nam"),
    ("nuwocs", "nước"),
    ("hoangf", "hoàng"),
    ("khoong", "không"),
    ("cacs", "các"),
    ("truwongf", "trường"),
    ("hoar", "hỏa"),
    ("cuoocj", "cuộc"),
    ("suwowngs", "sướng"),
    ("tuooir", "tuổi"),
    ("chuyeen", "chuyên"),
    ("lamf", "làm"),
    ("dduwongf", "đường"),
    ("buooir", "buổi"),
    ("muwowif", "mười"),
    ("ddi", "đi"),
    ("anw", "ăn"),
    ("nguwowif", "người"),
    ("tooi", "tôi"),
    ("cho", "cho"),
    ("vowis", "với"),
    ("vaf", "và"),
    ("laf", "là"),
    ("cur", "củ"),
    ("ddax", "đã"),
    ("nayf", "này"),
    ("ddos", "đó"),
    ("raatj", "r\u{1ead}t"),
    ("nhieeuf", "nhiều"),
    ("trung", "trung"),
    ("quoocs", "quốc"),
    ("gioowf", "giờ"),
    ("caanf", "cần"),
    ("phair", "phải"),
    ("vowj", "vợ"),
    ("nhaf", "nhà"),
    ("song", "song"),
    ("hoawcj", "hoặc"),
    ("moojt", "một"),
    ("ddieefu", "điều"),
    ("saau", "sâu"),
    ("hanj", "hạn"),
    ("naof", "nào"),
    ("vaof", "vào"),
    ("daau", "dâu"),
    ("ddee", "đê"),
    ("ddees", "đế"),
    ("oo", "ô"),
    ("oos", "ố"),
    ("oor", "ổ"),
    ("hangf", "hàng"),
    ("saif", "sài"),
    ("goij", "gọi"),
    ("cayf", "cày"),
    ("loaji", "loại"),
    ("ddoongj", "động"),
    ("sair", "sải"),
    ("chuoojng", "chuộng"),
    ("trair", "trải"),
    ("ngayf", "ngày"),
    ("dduwocj", "được"),
    ("ddeem", "đêm"),
    ("tre", "tre"),
    ("quee", "quê"),
    ("ddaa", "đâ"),
    ("nghieemj", "nghiệm"),
    ("howji", "hợi"),
    ("mowf", "mờ"),
    ("daaus", "dấu"),
    ("ruwowuj", "rượu"),
    ("ddowji", "đợi"),
    ("bayf", "bày"),
    ("bowf", "bờ"),
    ("lawxm", "l\u{1eb5}m"),
    ("dduws", "đứ"),
    ("thuwjc", "thực"),
    ("xao", "xao"),
    ("xaof", "xào"),
    ("gaf", "gà"),
    ("gacs", "gác"),
    ("nhor", "nhỏ"),
    ("moong", "mông"),
    ("nguoofn", "nguồn"),
    ("aan", "ân"),
    ("aans", "ấn"),
    ("tieengj", "tiệng"),
    ("vieejc", "việc"),
    ("moor", "mổ"),
    ("trowif", "trời"),
    ("quen", "quen"),
    ("oong", "ông"),
    ("ooms", "ốm"),
    ("chaof", "chào"),
    ("caamf", "c\u{1ea7}m"),
    ("nhawns", "nhắn"),
    ("booif", "bồi"),
    ("ddocj", "đọc"),
    ("xong", "xong"),
    ("quyeern", "quyển"),
];

/// (keystrokes, expected word) for VNI. Digits sit right after the vowel
/// they transform (6 = â/ê/ô, 7 = ơ/ư, 8 = ă, 9 = đ; tones 1..5).
#[cfg(test)]
const VNI: &[(&str, &str)] = &[
    ("tie6ng1", "tiếng"),
    ("vie6t5", "việt"),
    ("d9", "đ"),
    ("nam", "nam"),
    ("nu7o1c", "nước"),
    ("hoa2ng", "hoàng"),
    ("kho6ng", "không"),
    ("ca1c", "các"),
    ("tru72ong", "trường"),
    ("ho3a", "hỏa"),
    ("cuo65c", "cuộc"),
    ("suo71ng", "sướng"),
    ("tuo63i", "tuổi"),
    ("chuye61n", "chuyến"),
    ("la2m", "làm"),
    ("d9u7o2ng", "đường"),
    ("buo63i", "buổi"),
    ("muo72i", "mười"),
    ("d9i", "đi"),
    ("a8n", "ăn"),
    ("ngu7o2i", "người"),
    ("to6i", "tôi"),
    ("vo71i", "với"),
    ("va2", "và"),
    ("la2", "là"),
    ("cu3", "củ"),
    ("d9a4", "đã"),
    ("na2y", "này"),
    ("d9o1", "đó"),
    ("ra65t", "r\u{1ead}t"),
    ("nhie62u", "nhiều"),
    ("trung", "trung"),
    ("quo61c", "quốc"),
    ("gio72", "giờ"),
    ("ca62n", "cần"),
    ("pha3i", "phải"),
    ("vo75", "vợ"),
    ("nha2", "nhà"),
    ("song", "song"),
    ("hoa85c", "hoặc"),
    ("mo65t", "một"),
    ("die62u", "diều"),
    ("sa6u", "sâu"),
    ("ha5n", "hạn"),
    ("na2o", "nào"),
    ("va2o", "vào"),
    ("da6u", "dâu"),
    ("d9e61", "đế"),
    ("o6", "ô"),
    ("o61", "ố"),
    ("o63", "ổ"),
    ("ha2ng", "hàng"),
    ("sa2i", "sài"),
    ("go5i", "gọi"),
    ("ca2y", "cày"),
    ("lo5ai", "loại"),
    ("d9o65ng", "động"),
    ("tu7o1ng", "tướng"),
    ("nguo62n", "nguồn"),
    ("tro72i", "trời"),
    ("bo62i", "bồi"),
    ("d9o75i", "đợi"),
    ("da61u", "dấu"),
    ("ruo75u", "rượu"),
    ("nho7", "nhơ"),
    ("nho74", "nhỡ"),
    ("mo72", "mờ"),
    ("bo72", "bờ"),
    ("d9u71", "đứ"),
    ("thu75c", "thực"),
    ("xao", "xao"),
    ("xao2", "xào"),
    ("ga2", "gà"),
    ("ga1c", "gác"),
    ("nho3", "nhỏ"),
    ("mo6ng", "mông"),
    ("a6n", "ân"),
    ("a61n", "ấn"),
    ("tie65ng", "tiệng"),
    ("vie65c", "việc"),
    ("mo63", "mổ"),
    ("que6", "quê"),
    ("d9a6", "đâ"),
    ("nghie65m", "nghiệm"),
    ("ho75i", "hợi"),
    ("chao2", "chào"),
    ("nha61n", "nhấn"),
    ("d9o5c", "đọc"),
    ("sa1ch", "sách"),
    ("o6ng", "ông"),
    ("o61m", "ốm"),
    ("quye63n", "quyển"),
    ("ma61y", "mấy"),
    ("la81m", "lắm"),
    ("sa3i", "sải"),
    ("bay2", "bày"),
];

/// (keystrokes, expected word) for VIQR. Marks are punctuation keys
/// (`' \` ? ~ .` tones, `^ + * (` shapes, `0` remove-tone), so they must
/// reach bamboo-core instead of ending the word; `d` twice still strokes.
/// bamboo-core applies a mark only where the syllable can take one and
/// leaves the key literal otherwise; typing the same key twice yields a
/// literal character (`hoi..`), the classic VIQR punctuation escape.
#[cfg(test)]
const VIQR: &[(&str, &str)] = &[
    ("toi'", "tói"),
    ("toi`", "tòi"),
    ("hoi?", "hỏi"),
    ("nga~", "ngã"),
    ("ddem.", "đẹm"),
    ("dda(ng.", "đặng"),
    ("ca^", "câ"),
    ("de^", "dê"),
    ("to^i", "tôi"),
    ("do^", "dô"),
    ("bo+", "bơ"),
    ("tu+", "tư"),
    ("mo+", "mơ"),
    ("sa(ng", "săng"),
    ("vie^t.", "việt"),
    ("toi'0", "toi"),
    ("hoi..", "hoi."),
    ("em.", "ẹm"),
    ("(em", "(em"),
    ("ok?", "ok?"),
    ("don't", "don't"),
    ("nam", "nam"),
    ("toi", "toi"),
    ("ddi", "đi"),
];

/// (keystrokes, expected word) for the Microsoft layout. The layout is
/// insert-style: digit rules append or place the marked vowel (`ti3ng8`,
/// `n1ng`), tones apply at the rhyme (`sach9`, `me5`), `0` inserts đ, and
/// ư/ơ exist only on the bracket keys — which must reach bamboo-core for
/// this method regardless of the bracket option. The insert rules place
/// lowercase glyphs regardless of the typed case (`TI3NG8` stays `TIếNG`
/// upstream), so these rows run without the case variants.
#[cfg(test)]
const MS_LAYOUT: &[(&str, &str)] = &[
    ("ti3ng8", "tiếng"),
    ("n1ng", "năng"),
    ("0i", "đi"),
    ("t[", "tư"),
    ("t]", "tơ"),
    ("vi3t9", "việt"),
    ("me5", "mè"),
    ("sach9", "sạch"),
    ("hoi6", "hỏi"),
    ("ngai7", "ngãi"),
    ("nam", "nam"),
    ("toi", "toi"),
];

/// Feed the keystrokes to the engine, simulating the application buffer: a
/// key whose actions contain `Forward` inserts the raw character.
#[cfg(test)]
fn typed_word(method: Method, keys: &str) -> String {
    let mut e = Engine::new(method, Mode::NonPreedit);
    // Non-relayed in-process client: golden cases exercise the protocol path.
    e.set_client(Some("gtk4-im:golden".into()));
    e.set_capabilities(0b100001); // PREEDIT_TEXT | SURROUNDING_TEXT
    let mut buf: Vec<char> = Vec::new();
    for c in keys.chars() {
        let actions = e.process_char(c);
        let forwarded = actions.contains(&Action::Forward);
        for a in &actions {
            match a {
                Action::DeleteBefore(n) => buf.truncate(buf.len().saturating_sub(*n as usize)),
                Action::DeleteSurrounding { offset, n } => {
                    let start = (buf.len() as i64 + *offset as i64).max(0) as usize;
                    let end = (start + *n as usize).min(buf.len());
                    buf.drain(start..end);
                }
                Action::Commit(t) => buf.extend(t.chars()),
                _ => {}
            }
        }
        if forwarded {
            buf.push(c);
        }
        let text: String = buf.iter().collect();
        e.set_surrounding(text, buf.len() as u32);
    }
    buf.into_iter().collect()
}

#[cfg(test)]
fn variants(keys: &str, expected: &str) -> Vec<(String, String)> {
    let mut out = vec![(keys.to_string(), expected.to_string())];
    // Capitalized: first letter (the word itself, tone keys stay lowercase).
    let mut cap_keys = keys.to_string();
    if let Some(first) = cap_keys.chars().next() {
        cap_keys = first.to_uppercase().collect::<String>() + &cap_keys[first.len_utf8()..];
    }
    let cap_expected: String = expected
        .char_indices()
        .next()
        .map(|(i, c)| {
            let mut s = String::new();
            s.extend(c.to_uppercase());
            s + &expected[i + c.len_utf8()..]
        })
        .unwrap_or_else(|| expected.to_string());
    out.push((cap_keys, cap_expected));
    // UPPER: letters upper, tone keys keep their case-insensitive meaning.
    let upper_keys: String = keys
        .chars()
        .map(|c| c.to_uppercase().next().unwrap_or(c))
        .collect();
    let upper_expected: String = expected.to_uppercase();
    out.push((upper_keys, upper_expected));
    out
}

#[cfg(test)]
fn run_table(method: Method, table: &[(&str, &str)], case_variants: bool) -> Vec<String> {
    let mut failures = Vec::new();
    for (keys, expected) in table {
        let rows = if case_variants {
            variants(keys, expected)
        } else {
            vec![(keys.to_string(), expected.to_string())]
        };
        for (k, want) in rows {
            let got = typed_word(method, &k);
            if got != want {
                failures.push(format!("{k} -> {got:?}, expected {want:?}"));
            }
        }
    }
    failures
}

// ---------------------------------------------------------------------------
// FakeClient: the golden buffer generalized into a client that can be broken
// in every way real clients are broken (audit Bước 2). Each fault bit models
// one implicit engine assumption; the matrix below drives modes x faults x
// key sequences and checks the invariants on every keystroke.
// ---------------------------------------------------------------------------

/// One client misbehavior bit. `default()` is the ideal client: synchronous,
/// ordered, always-fresh surrounding text.
#[cfg(test)]
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
struct Faults {
    /// Never sends SetSurroundingText (XWayland / Sublime / XIM clients).
    no_st: bool,
    /// Lacks IBUS_CAP_PREEDIT_TEXT: preedit must fall back to NonPreedit.
    no_preedit_cap: bool,
    /// Swallows protocol deletes (Chrome/Electron-style clients).
    drop_delete: bool,
    /// The ST report lags one keystroke behind the real buffer (fast typing).
    stale_st: bool,
    /// The app appends a suggestion character after the caret on every
    /// commit without the caret moving past it (Chrome inline autocomplete).
    app_edit: bool,
    /// The caret jumps to the middle of the buffer before every key (a
    /// click or an arrow) — the engine's belief about the caret goes stale.
    caret_moved: bool,
    /// No FocusInId ever arrives: App Rules cannot match (Sublime/XIM).
    no_client_id: bool,
}

#[cfg(test)]
struct FakeClient {
    buf: Vec<char>,
    caret: usize,
    faults: Faults,
    /// The report the client sent last time — resent when `stale_st` is on.
    last_report: Option<(String, u32)>,
    /// Protocol deletes (DeleteBefore/DeleteSurrounding) the client received.
    protocol_deletes: u32,
    /// Deletes the client swallowed (drop_delete).
    deletes_swallowed: u32,
}

impl FakeClient {
    fn new(faults: Faults) -> Self {
        Self {
            buf: Vec::new(),
            caret: 0,
            faults,
            last_report: None,
            protocol_deletes: 0,
            deletes_swallowed: 0,
        }
    }

    fn text(&self) -> String {
        self.buf.iter().collect()
    }

    /// Apply one action batch to the buffer. `typed` is the raw key when the
    /// batch forwards it. Returns the actions so callers can assert on them.
    fn apply(&mut self, actions: &[Action], typed: Option<char>) {
        for a in actions {
            match a {
                Action::Commit(t) => {
                    let n = t.chars().count();
                    self.buf
                        .splice(self.caret..self.caret, t.chars().collect::<Vec<char>>());
                    self.caret += n;
                    if self.faults.app_edit {
                        // Chrome-style inline autocomplete: suggestion text
                        // appears after the caret, caret stays put.
                        self.buf.insert(self.caret, 'q');
                    }
                }
                Action::Forward => {
                    if let Some(c) = typed {
                        self.buf.insert(self.caret, c);
                        self.caret += 1;
                    }
                }
                Action::DeleteBefore(n) => {
                    self.protocol_deletes += 1;
                    if self.faults.drop_delete {
                        self.deletes_swallowed += n;
                    } else {
                        let start = self.caret.saturating_sub(*n as usize);
                        self.buf.drain(start..self.caret);
                        self.caret = start;
                    }
                }
                Action::DeleteSurrounding { offset, n } => {
                    self.protocol_deletes += 1;
                    if self.faults.drop_delete {
                        self.deletes_swallowed += n;
                    } else {
                        let start = (self.caret as i64 + *offset as i64).max(0) as usize;
                        let end = (start + *n as usize).min(self.buf.len());
                        self.buf.drain(start..end);
                        self.caret = self.caret.min(start);
                    }
                }
                // Real Backspace presses always reach the buffer, protocol
                // or not — that is their whole point.
                Action::RewriteByBackspace { backspaces, commit } => {
                    let start = self.caret.saturating_sub(*backspaces as usize);
                    self.buf.drain(start..self.caret);
                    self.caret = start;
                    self.buf.splice(
                        self.caret..self.caret,
                        commit.chars().collect::<Vec<char>>(),
                    );
                    self.caret += commit.chars().count();
                }
                Action::InjectBackspaces(n) => {
                    let start = self.caret.saturating_sub(*n as usize);
                    self.buf.drain(start..self.caret);
                    self.caret = start;
                }
                Action::SelectLeft(n) => {
                    self.caret = self.caret.saturating_sub(*n as usize);
                }
                Action::PressDelete => {
                    self.buf.drain(self.caret..self.caret);
                }
                Action::Paste(t) => {
                    self.buf
                        .splice(self.caret..self.caret, t.chars().collect::<Vec<char>>());
                    self.caret += t.chars().count();
                }
                // Preedit text never sits in the application buffer; commits
                // are the only way it lands.
                Action::UpdatePreedit { .. } | Action::HidePreedit => {}
                Action::Consumed | Action::PostDelay(_) => {}
            }
        }
    }

    fn send_report(&mut self, e: &mut Engine) {
        if self.faults.no_st {
            return;
        }
        let current = (self.text(), self.caret as u32);
        let (text, cursor) = if self.faults.stale_st {
            self.last_report
                .clone()
                .unwrap_or_else(|| (String::new(), 0))
        } else {
            current.clone()
        };
        self.last_report = Some(current);
        e.set_surrounding(text, cursor);
    }

    /// One text keystroke through the engine, then the buffer update and
    /// the surrounding-text report — the full client turn.
    fn key(&mut self, e: &mut Engine, c: char) -> Vec<Action> {
        if self.faults.caret_moved && self.caret > 0 {
            self.caret /= 2;
        }
        let actions = e.process_char(c);
        self.apply(&actions, Some(c));
        self.send_report(e);
        actions
    }

    /// The Backspace key: the engine handles it when it owns the
    /// composition, otherwise the application deletes one character.
    fn backspace(&mut self, e: &mut Engine) {
        match e.backspace() {
            None => {
                let start = self.caret.saturating_sub(1);
                self.buf.drain(start..self.caret);
                self.caret = start;
            }
            Some(actions) => {
                self.apply(&actions, None);
            }
        }
        self.send_report(e);
    }

    /// A navigation/control key (arrows, Home, End...): the server resets
    /// the engine before forwarding, swallowing the first key on relays.
    fn nav_key(&mut self, e: &mut Engine) {
        let actions = e.reset();
        self.apply(&actions, None);
        self.send_report(e);
    }

    /// Server FocusOut: flush deferred commits, reset, disable, forget the
    /// client and its buffer data.
    fn focus_out(&mut self, e: &mut Engine) {
        for text in e.flush_deferred() {
            self.apply(&[Action::Commit(text)], None);
        }
        let actions = e.reset();
        self.apply(&actions, None);
        e.set_enabled(false);
        e.set_client(None);
        e.clear_buffer_state();
    }

    /// Server FocusInId (client empty under the no_client_id fault).
    fn focus_in(&mut self, e: &mut Engine, client: &str) {
        let client = if self.faults.no_client_id {
            None
        } else {
            Some(client.to_string())
        };
        e.set_client(client);
        // GTK-style clients report the preedit and surrounding caps; the
        // no_preedit_cap fault drops the preedit bit.
        let caps = if self.faults.no_preedit_cap {
            0b100000
        } else {
            0b100001
        };
        e.set_capabilities(caps);
        e.clear_buffer_state();
        e.set_enabled(true);
    }
}

/// Safety invariants checked on every action batch of the matrix (audit
/// Bước 2): no protocol delete without a real ST report, no protocol delete
/// to a relayed context, and the engine must never be waiting on echoes or
/// deferred commits after a focus change.
#[cfg(test)]
fn check_safety_invariants(e: &Engine, actions: &[Action], where_: &str) {
    let no_transport = !e.surrounding_data_ok() && !e.uinput_available();
    for a in actions {
        match a {
            Action::DeleteBefore(_) => {
                assert!(
                    !no_transport,
                    "{where_}: DeleteBefore without ST and without uinput: {actions:?}"
                );
            }
            Action::DeleteSurrounding { .. } => {
                assert!(
                    e.surrounding_data_ok() && !e.relay_context(),
                    "{where_}: repair DeleteSurrounding for relay/no-ST client: {actions:?}"
                );
            }
            _ => {}
        }
    }
}

/// After a focus change nothing may be left pending (audit invariant:
/// "sau focus out/reset không còn preedit, không còn chờ echo").
#[cfg(test)]
fn assert_clean_after_focus_out(e: &Engine, where_: &str) {
    assert!(
        !e.preedit_commit_pending(),
        "{where_}: preedit still pending after focus out"
    );
    assert!(
        !e.has_deferred(),
        "{where_}: deferred commits after focus out"
    );
    assert!(
        !e.pending_echoes(),
        "{where_}: pending echoes after focus out"
    );
}

/// The mode × fault × keystroke matrix. Ideal rows must reproduce the exact
/// text the golden tables expect; faulted rows only have to hold the safety
/// invariants (no panic, no protocol-delete violation, clean focus out).
#[cfg(test)]
mod matrix {
    use super::*;

    /// One matrix row: a full client session. Returns the final buffer text.
    fn run_session(method: Method, mode: Mode, faults: Faults, client: &str, keys: &str) -> String {
        let mut e = Engine::new(method, mode);
        let mut fc = FakeClient::new(faults);
        fc.focus_in(&mut e, client);
        for c in keys.chars() {
            match c {
                '\u{8}' => fc.backspace(&mut e), // {b} backspace
                '\u{9}' => fc.nav_key(&mut e),   // {t} navigation key
                _ => {
                    let actions = fc.key(&mut e, c);
                    check_safety_invariants(&e, &actions, &format!("{mode:?} {faults:?} '{c}'"));
                }
            }
        }
        fc.focus_out(&mut e);
        assert_clean_after_focus_out(&e, &format!("{mode:?} {faults:?}"));
        fc.text()
    }

    /// (method, keys, expected text on an ideal client) — one word per
    /// input method plus the tone-undo and Backspace scenarios.
    const SEQUENCES: &[(&str, &str, &str)] = &[
        ("telex", "tieengs ", "tiếng "),
        ("telex", "aa s ", "â s "),
        ("telex", "tie\u{8}n ", "tin "),
        ("telex", "window ", "window "),
        ("vni", "tie6ng1 ", "tiếng "),
        ("vni", "a6 s ", "â s "),
        ("viqr", "hoi? ", "hỏi "),
        ("viqr", "a^ s ", "â s "),
        ("ms", "ti3ng8 ", "tiếng "),
        ("ms", "t[ ", "tư "),
    ];

    fn method_of(name: &str) -> Method {
        match name {
            "telex" => Method::Telex,
            "vni" => Method::Vni,
            "viqr" => Method::Viqr,
            _ => Method::MsLayout,
        }
    }

    fn expected_for(method: Method, keys: &str) -> Option<&'static str> {
        let table: &[(&str, &str)] = match method {
            Method::Telex => &[
                ("tieengs ", "tiếng "),
                ("aa s ", "â s "),
                ("tie\u{8}n ", "tin "),
                ("window ", "window "),
            ],
            Method::Vni => &[("tie6ng1 ", "tiếng "), ("a6 s ", "â s ")],
            Method::Viqr => &[("hoi? ", "hỏi "), ("a^ s ", "â s ")],
            Method::MsLayout => &[("ti3ng8 ", "tiếng "), ("t[ ", "tư ")],
        };
        table.iter().find(|(k, _)| *k == keys).map(|(_, w)| *w)
    }

    #[test]
    fn ideal_clients_reproduce_the_golden_text_in_every_mode() {
        // The core invariant: with a perfect client, all three main display
        // modes leave exactly the same text for the same keystrokes.
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
            for (method, keys, _) in SEQUENCES {
                let method = method_of(method);
                let mut e = Engine::new(method, mode);
                let mut fc = FakeClient::new(Faults::default());
                fc.focus_in(&mut e, "gtk4-im:matrix");
                for c in keys.chars() {
                    match c {
                        '\u{8}' => fc.backspace(&mut e),
                        '\u{9}' => fc.nav_key(&mut e),
                        _ => {
                            let actions = fc.key(&mut e, c);
                            check_safety_invariants(
                                &e,
                                &actions,
                                &format!("ideal {mode:?} {method:?} '{c}'"),
                            );
                        }
                    }
                }
                let got = fc.text();
                let want = expected_for(method, keys)
                    .unwrap_or_else(|| panic!("no expectation for {method:?} {keys:?}"));
                assert_eq!(got, want, "ideal {mode:?} {method:?} {keys:?}");
                fc.focus_out(&mut e);
                assert_clean_after_focus_out(&e, &format!("ideal {mode:?} {method:?}"));
            }
        }
    }

    #[test]
    fn faulted_clients_hold_the_safety_invariants() {
        let fault_rows: &[(&str, Faults)] = &[
            (
                "no_st",
                Faults {
                    no_st: true,
                    ..Faults::default()
                },
            ),
            (
                "drop_delete",
                Faults {
                    drop_delete: true,
                    ..Faults::default()
                },
            ),
            (
                "stale_st",
                Faults {
                    stale_st: true,
                    ..Faults::default()
                },
            ),
            (
                "app_edit",
                Faults {
                    app_edit: true,
                    ..Faults::default()
                },
            ),
            (
                "caret_moved",
                Faults {
                    caret_moved: true,
                    ..Faults::default()
                },
            ),
            (
                "no_preedit_cap",
                Faults {
                    no_preedit_cap: true,
                    ..Faults::default()
                },
            ),
            (
                "no_client_id",
                Faults {
                    no_client_id: true,
                    ..Faults::default()
                },
            ),
            (
                "kitchen_sink",
                Faults {
                    stale_st: true,
                    drop_delete: true,
                    app_edit: true,
                    ..Faults::default()
                },
            ),
        ];
        for mode in [Mode::NonPreedit, Mode::Preedit, Mode::Surrounding] {
            for (name, faults) in fault_rows {
                for (method, keys, _) in SEQUENCES {
                    let method = method_of(method);
                    let client = match *name {
                        "no_client_id" => "gtk4-im:nosuch",
                        _ => "gtk4-im:matrix",
                    };
                    let got = run_session(method, mode, *faults, client, keys);
                    let _ = got; // faulted rows: safety invariants only
                }
            }
        }
    }

    #[test]
    fn ideal_client_without_preedit_cap_still_types_the_word() {
        // The no-preedit-cap fallback is not just safe, it is productive:
        // the word must still be typed (as NonPreedit commits).
        for (method, keys, _) in SEQUENCES {
            let method = method_of(method);
            let got = run_session(
                method,
                Mode::Preedit,
                Faults {
                    no_preedit_cap: true,
                    ..Faults::default()
                },
                "gtk4-im:matrix",
                keys,
            );
            let want = expected_for(method, keys).unwrap();
            assert_eq!(got, want, "no-preedit-cap fallback {method:?} {keys:?}");
        }
    }

    #[test]
    fn drop_delete_clients_still_get_the_right_text() {
        // Chrome-style clients swallow the protocol delete. Listed clients
        // are auto-switched to preedit by the name heuristic; an UNLISTED
        // client in NonPreedit gets its mark-key rewrites swallowed. The
        // engine must detect the swallowed delete and heal: following
        // words go through preedit and type correctly. The corrupted first
        // word is the documented cost of the already-swallowed delete.
        for mode in [Mode::NonPreedit, Mode::Surrounding] {
            // First word: "aa " under Telex — the swallowed delete may
            // corrupt it ("aâ"); nothing can undo what the app never
            // deleted. Second word must be exact.
            let got = run_session(
                Method::Telex,
                mode,
                Faults {
                    drop_delete: true,
                    ..Faults::default()
                },
                "gtk4-im:matrix",
                "aa tieengs ",
            );
            assert!(
                got.ends_with("tiếng "),
                "swallowed-delete detection must heal following words: \
                 {mode:?} final {got:?}"
            );
        }
    }

    /// Deterministic xorshift64* — no new dependency (the audit forbids
    /// adding crates without need); the seed is fixed so failures replay.
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            let mut x = self.0;
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            self.0 = x;
            x
        }
        fn pick<'a, T>(&mut self, items: &'a [T]) -> &'a T {
            &items[(self.next() % items.len() as u64) as usize]
        }
    }

    /// Seeded fuzz: random keystrokes (letters, tones, digits, punctuation,
    /// Backspace, navigation keys, Space) with focus in/out and mode changes
    /// interleaved, over all fault combinations. Nothing may panic, the
    /// safety invariants must hold on every batch, and every focus change
    /// must leave the engine clean. Seed 0xC0FFEE — fixed so a failure
    /// replays deterministically.
    #[test]
    fn fuzz_random_keystrokes_hold_the_invariants() {
        const SEED: u64 = 0xC0FFEE;
        let letters: Vec<char> = "aaeeoouuddwwssffjjxxrrz6789^~`'+*(.[{t i e n g h o l }"
            .chars()
            .collect();
        let terminators: Vec<char> = vec![' ', '_', '5', '.', ',', '!', '\''];
        let modes: Vec<Mode> = vec![
            Mode::NonPreedit,
            Mode::Preedit,
            Mode::Surrounding,
            Mode::Off,
            Mode::DefaultTyping,
            Mode::Smooth,
            Mode::SuperSmooth,
            Mode::Select,
        ];
        let fault_sets: Vec<Faults> = vec![
            Faults::default(),
            Faults {
                no_st: true,
                ..Faults::default()
            },
            Faults {
                drop_delete: true,
                ..Faults::default()
            },
            Faults {
                stale_st: true,
                ..Faults::default()
            },
            Faults {
                app_edit: true,
                ..Faults::default()
            },
            Faults {
                caret_moved: true,
                ..Faults::default()
            },
            Faults {
                no_preedit_cap: true,
                ..Faults::default()
            },
            Faults {
                no_client_id: true,
                ..Faults::default()
            },
        ];
        let mut rng = Rng(SEED);
        for iteration in 0..300u32 {
            let faults = *rng.pick(&fault_sets);
            let mode = *rng.pick(&modes);
            let client = if faults.no_client_id {
                "gtk4-im:fuzz"
            } else {
                "gtk4-im:matrix"
            };
            let mut e = Engine::new(Method::Telex, mode);
            let mut fc = FakeClient::new(faults);
            fc.focus_in(&mut e, client);
            for _ in 0..50 {
                match rng.next() % 12 {
                    0..=5 => {
                        let c = *rng.pick(&letters);
                        let actions = fc.key(&mut e, c);
                        check_safety_invariants(
                            &e,
                            &actions,
                            &format!("fuzz {SEED} it{iteration} {mode:?} {faults:?} '{c}'"),
                        );
                    }
                    6 => {
                        let c = *rng.pick(&terminators);
                        let actions = fc.key(&mut e, c);
                        check_safety_invariants(
                            &e,
                            &actions,
                            &format!("fuzz {SEED} it{iteration} {mode:?} {faults:?} '{c}'"),
                        );
                    }
                    7 => fc.backspace(&mut e),
                    8 => fc.nav_key(&mut e),
                    9 => {
                        fc.focus_out(&mut e);
                        assert_clean_after_focus_out(
                            &e,
                            &format!("fuzz {SEED} it{iteration} {mode:?} {faults:?}"),
                        );
                        fc.focus_in(&mut e, client);
                    }
                    10 | 11 => {
                        let mode = *rng.pick(&modes);
                        e.apply_config(&crate::config::Config {
                            mode: mode.into(),
                            ..crate::config::Config::default()
                        });
                        e.set_enabled(true);
                    }
                    _ => unreachable!(),
                }
            }
            fc.focus_out(&mut e);
            assert_clean_after_focus_out(
                &e,
                &format!("fuzz {SEED} it{iteration} final {mode:?} {faults:?}"),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn telex_golden() {
        assert!(
            TELEX.len() * 3 >= 300,
            "need >=300 telex cases, have {}",
            TELEX.len() * 3
        );
        let failures = run_table(Method::Telex, TELEX, true);
        assert!(
            failures.is_empty(),
            "{} failures:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn vni_golden() {
        assert!(
            VNI.len() * 3 >= 150,
            "need >=150 vni cases, have {}",
            VNI.len() * 3
        );
        let failures = run_table(Method::Vni, VNI, true);
        assert!(
            failures.is_empty(),
            "{} failures:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn viqr_golden() {
        let failures = run_table(Method::Viqr, VIQR, true);
        assert!(
            failures.is_empty(),
            "{} failures:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }

    #[test]
    fn microsoft_golden() {
        let failures = run_table(Method::MsLayout, MS_LAYOUT, false);
        assert!(
            failures.is_empty(),
            "{} failures:\n{}",
            failures.len(),
            failures.join("\n")
        );
    }
}

#[cfg(test)]
mod fingerprint {
    use super::*;

    /// Engine action fingerprints for the two audit cases — the exact
    /// command sequence each display mode emits, captured from the real
    /// engine and pinned here as a regression contract (audit Bước 1.2):
    /// (a) Telex `t i e e n g s` + Space -> "tiếng ";
    /// (b) `a a s` + Space -> "ấ " (aa -> â, then the sac mark).
    #[test]
    fn modes_emit_the_documented_action_fingerprints() {
        // (key, expected action names)
        let word: &[(&str, &[&str])] = &[
            ("t", &["Commit(t)"]),
            ("i", &["Commit(i)"]),
            ("e", &["Commit(e)"]),
            ("e", &["DeleteBefore(1)", "Commit(ê)"]),
            ("n", &["Commit(n)"]),
            ("g", &["Commit(g)"]),
            ("s", &["DeleteBefore(3)", "Commit(ếng)"]),
            (" ", &["Forward"]),
        ];
        let mark: &[(&str, &[&str])] = &[
            ("a", &["Commit(a)"]),
            ("a", &["DeleteBefore(1)", "Commit(â)"]),
            ("s", &["DeleteBefore(1)", "Commit(ấ)"]),
            (" ", &["Forward"]),
        ];
        // NonPreedit and Surrounding share the per-key rewrite fingerprint; preedit
        // holds the composition and has its own block below.
        for mode in [Mode::NonPreedit, Mode::Surrounding] {
            for (name, keys, want) in [("word", "tieengs ", word), ("mark", "aas ", mark)] {
                let mut e = Engine::new(Method::Telex, mode);
                let mut fc = FakeClient::new(Faults::default());
                fc.focus_in(&mut e, "gtk4-im:fp");
                let mut it = want.iter();
                for c in keys.chars() {
                    let actions = fc.key(&mut e, c);
                    let Some((kc, expected)) = it.next() else {
                        panic!("fingerprint table shorter than the keys");
                    };
                    assert_eq!(c.to_string(), *kc, "{mode:?} {name}");
                    let names: Vec<String> = actions
                        .iter()
                        .map(|a| match a {
                            Action::Commit(t) => format!("Commit({t})"),
                            Action::DeleteBefore(n) => format!("DeleteBefore({n})"),
                            Action::Forward => "Forward".into(),
                            other => format!("{other:?}"),
                        })
                        .collect();
                    assert_eq!(
                        names,
                        expected.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
                        "{mode:?} {name} key {c}"
                    );
                }
                assert_eq!(fc.text(), if name == "word" { "tiếng " } else { "ấ " });
            }
        }
        // The preedit fingerprint differs by construction: every key is an
        // UpdatePreedit of the composed text; the terminator commits once.
        let mut e = Engine::new(Method::Telex, Mode::Preedit);
        let mut fc = FakeClient::new(Faults::default());
        fc.focus_in(&mut e, "gtk4-im:fp");
        let mut shown = String::new();
        for c in "tieengs ".chars() {
            let actions = fc.key(&mut e, c);
            let names: Vec<String> = actions
                .iter()
                .map(|a| match a {
                    Action::UpdatePreedit { text, .. } => {
                        shown = text.clone();
                        format!("Preedit({text})")
                    }
                    Action::Commit(t) => format!("Commit({t})"),
                    Action::HidePreedit => "HidePreedit".into(),
                    Action::Forward => "Forward".into(),
                    other => format!("{other:?}"),
                })
                .collect();
            if c == ' ' {
                assert_eq!(names, ["HidePreedit", "Commit(tiếng)", "Forward"]);
            } else {
                assert_eq!(names.len(), 1, "{names:?}");
                assert!(names[0].starts_with("Preedit("), "{names:?}");
            }
        }
        assert_eq!(shown, "tiếng");
    }
}
