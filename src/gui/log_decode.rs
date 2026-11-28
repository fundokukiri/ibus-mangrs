//! Reading the debug log back. The engine writes `<epoch_ms> <tag> <k=v>…`, so
//! a line says which part of the engine spoke and the viewer only has to read
//! that tag; the fields are decoded here. Kept out of the page so it can be
//! tested without a window, and so a change here can never touch the way the
//! engine writes the file.

/// What a line is about; the chip row filters on this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Key,
    Insert,
    Delete,
    Flush,
    Error,
    Other,
}

/// A decoded line, ready to be drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Line {
    pub kind: Kind,
    /// Gap to the previous line, in milliseconds. `None` for a line without a
    /// timestamp, which is how a hand-pasted fragment reads.
    pub rel_ms: Option<u64>,
    /// The line's own timestamp, so the next line can measure against it.
    pub abs_ms: Option<u64>,
    pub body: Body,
    /// Position in the viewer's cache, assigned as lines are appended. The
    /// pane uses it to tell what it has already drawn: a cache at its cap
    /// drops its oldest lines every poll, so a line count would never move.
    pub seq: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Body {
    /// A key event, decoded: what was pressed and whether it went down.
    Key {
        keysym: String,
        code: u32,
        release: bool,
        mods: Vec<&'static str>,
    },
    /// Everything else, kept as the engine wrote it.
    Text(String),
}

impl Line {
    /// The content without a press/release word: what the filter matches and
    /// what a report quotes.
    pub(super) fn plain(&self) -> String {
        self.body.text("", "").trim().to_string()
    }
}

impl Body {
    /// The one line the viewer shows. The press/release words are arguments
    /// because they are translated; the keysym and its code are not.
    pub(super) fn text(&self, press: &str, release: &str) -> String {
        match self {
            Body::Key {
                keysym,
                code,
                release: is_release,
                mods,
            } => {
                let mut out = String::new();
                for m in mods {
                    out.push_str(m);
                    out.push('+');
                }
                if keysym.starts_with("0x") {
                    out.push_str(keysym);
                } else {
                    out.push_str(&format!("{keysym} (0x{code:x})"));
                }
                out.push(' ');
                out.push_str(if *is_release { release } else { press });
                out
            }
            Body::Text(text) => text.clone(),
        }
    }
}

/// Split `<epoch_ms> <text>` and decode it. `prev_ms` is the timestamp of the
/// line above, which turns a wall clock into a gap a human can read.
pub(super) fn parse(raw: &str, prev_ms: Option<u64>) -> Option<Line> {
    let (ms, msg) = match raw.split_once(' ') {
        Some((head, rest)) if is_ms(head) => (head.parse::<u64>().ok()?, rest),
        _ => (0, raw),
    };
    let rel_ms = if ms == 0 {
        None
    } else {
        Some(ms.saturating_sub(prev_ms.unwrap_or(ms)))
    };
    let kind = classify(msg);
    let body = if kind == Kind::Key {
        match key_body(msg) {
            Some(body) => body,
            None => Body::Text(msg.to_string()),
        }
    } else {
        Body::Text(msg.to_string())
    };
    Some(Line {
        kind,
        rel_ms,
        abs_ms: (ms != 0).then_some(ms),
        body,
        seq: 0,
    })
}

fn is_ms(head: &str) -> bool {
    head.len() >= 10 && head.bytes().all(|b| b.is_ascii_digit())
}

/// Failure first: a line that both backspaces and fails is a failure, and the
/// red chip has to count it. After that the tag decides — the engine writes it
/// as the first word of every line (see `debug_log::TAGS`).
pub(super) fn classify(msg: &str) -> Kind {
    if msg.split_whitespace().any(|field| field == "ok=0") {
        return Kind::Error;
    }
    match msg.split(' ').next().unwrap_or("") {
        "key" => Kind::Key,
        "ins" | "st" => Kind::Insert,
        "del" => Kind::Delete,
        "buf" => Kind::Flush,
        // A line from an engine older than the tagged format still reads: it
        // has no tag, so the old prefixes and failure words still decide.
        _ if lower_words(msg) => Kind::Error,
        _ if msg.starts_with("timed commit flush") || msg.contains("commit flush") => Kind::Flush,
        _ if msg.starts_with("st report")
            || msg.starts_with("verify:")
            || msg.starts_with("surrounding") =>
        {
            Kind::Insert
        }
        _ => Kind::Other,
    }
}

/// The failure words an untagged line may still carry.
fn lower_words(msg: &str) -> bool {
    let lower = msg.to_lowercase();
    ["error", "panic", "failed", "denied", "refused", "lỗi"]
        .iter()
        .any(|word| lower.contains(word))
}

/// `key keyval=0x69 state=0x40000000` into its parts. The values are hex
/// because that is what the engine writes and what issue reports quote.
fn key_body(msg: &str) -> Option<Body> {
    let mut code = None;
    let mut state = 0u32;
    for token in msg.split_whitespace() {
        if let Some(v) = token.strip_prefix("keyval=") {
            code = Some(u32::from_str_radix(v.trim_start_matches("0x"), 16).ok()?);
        } else if let Some(v) = token.strip_prefix("state=") {
            state = u32::from_str_radix(v.trim_start_matches("0x"), 16).unwrap_or(0);
        }
    }
    let code = code?;
    Some(Body::Key {
        keysym: keysym_name(code),
        code,
        release: state & RELEASE != 0,
        mods: modifier_names(state),
    })
}

/// Whether the newest report in the log says a word is being composed, which
/// is what the typing test shows beside its buffer line. `None` when no line
/// carries the flag.
pub(super) fn composing(lines: &[Line]) -> Option<bool> {
    for line in lines.iter().rev() {
        let text = line.plain();
        let Some(pos) = text.find("comp=") else {
            continue;
        };
        return Some(text[pos + "comp=".len()..].starts_with('1'));
    }
    None
}

/// X11 state bit 30: the key came back up.
const RELEASE: u32 = 1 << 30;

/// The modifiers a state mask names, in the order a shortcut is written.
pub(super) fn modifier_names(state: u32) -> Vec<&'static str> {
    let mut out = Vec::new();
    for (bit, name) in [
        (1 << 0, "Shift"),
        (1 << 2, "Ctrl"),
        (1 << 3, "Alt"),
        (1 << 6, "Super"),
    ] {
        if state & bit != 0 {
            out.push(name);
        }
    }
    out
}

/// The X11 name of a keysym, or its hex when it has none this page knows. The
/// printable range maps to the character itself, which is what a reader
/// expects to see for `0x69`.
pub(super) fn keysym_name(code: u32) -> String {
    if let Some(name) = match code {
        0xff08 => Some("BackSpace"),
        0xff09 => Some("Tab"),
        0xff0d => Some("Return"),
        0xff1b => Some("Escape"),
        0xff20 => Some("space"),
        0xff51 => Some("Left"),
        0xff52 => Some("Up"),
        0xff53 => Some("Right"),
        0xff54 => Some("Down"),
        0xff55 => Some("Page_Up"),
        0xff56 => Some("Page_Down"),
        0xff50 => Some("Home"),
        0xff57 => Some("End"),
        0xffff => Some("Delete"),
        0xffe1 => Some("Shift_L"),
        0xffe2 => Some("Shift_R"),
        0xffe3 => Some("Control_L"),
        0xffe4 => Some("Control_R"),
        0xffe5 => Some("Caps_Lock"),
        0xffe9 => Some("Alt_L"),
        0xffea => Some("Alt_R"),
        0xffeb => Some("Super_L"),
        0xffec => Some("Super_R"),
        _ => None,
    } {
        return name.to_string();
    }
    // ASCII and Latin-1 printable keysyms are their own character.
    if (0x20..=0x7e).contains(&code) || (0xa0..=0xff).contains(&code) {
        if let Some(c) = char::from_u32(code) {
            return c.to_string();
        }
    }
    format!("0x{code:04x}")
}

/// Hide the text a user typed. Two log fields carry it: `surrounding` prints
/// `tail=` and `text=` with the surrounding buffer, which is the document
/// itself — a password field included. The shape stays (one `*` per character)
/// so a reader can still see that something was there.
pub(super) fn mask_private(msg: &str) -> String {
    let mut out = String::with_capacity(msg.len());
    let mut rest = msg;
    while let Some(pos) = ["text=\"", "tail=\""]
        .iter()
        .filter_map(|field| rest.find(field))
        .min()
    {
        let field_start = pos;
        let Some(quote_start) = rest[field_start..].find('"') else {
            break;
        };
        out.push_str(&rest[..field_start + quote_start + 1]);
        let inner_start = field_start + quote_start + 1;
        let mut end = inner_start;
        // Every step lands on a character boundary, so the walk cannot end up
        // inside a multi-byte character: an escape may be followed by one
        // (`\à`), and this slice is the only place that would notice.
        while end < rest.len() {
            match rest.as_bytes()[end] {
                b'\\' => {
                    end += 1;
                    if let Some(c) = rest[end..].chars().next() {
                        end += c.len_utf8();
                    }
                }
                b'"' => break,
                _ => end += rest[end..].chars().next().map_or(1, char::len_utf8),
            }
        }
        let inner = &rest[inner_start..end.min(rest.len())];
        out.push_str(&"*".repeat(inner.chars().count()));
        rest = &rest[end.min(rest.len())..];
    }
    out.push_str(rest);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_backspace_release_line_reads_as_the_owner_asked() {
        let line = parse(
            "1795619563084 key keyval=0xff08 state=0x40000000",
            Some(1795619563000),
        )
        .expect("a log line");
        assert_eq!(line.kind, Kind::Key);
        assert_eq!(line.rel_ms, Some(84));
        assert_eq!(line.body.text("nhấn", "thả"), "BackSpace (0xff08) thả");
    }

    #[test]
    fn a_letter_key_names_its_character_and_its_code() {
        let line = parse("1795619563084 key keyval=0x69 state=0x0", None).expect("a log line");
        assert_eq!(line.body.text("nhấn", "thả"), "i (0x69) nhấn");
        let shifted = parse("1795619563084 key keyval=0x49 state=0x1", None).expect("a log line");
        assert_eq!(shifted.body.text("nhấn", "thả"), "Shift+I (0x49) nhấn");
    }

    #[test]
    fn the_state_mask_splits_release_from_modifiers() {
        assert!(modifier_names(0x40000000).is_empty());
        assert_eq!(modifier_names(0x5), vec!["Shift", "Ctrl"]);
        assert_eq!(modifier_names(0x48), vec!["Alt", "Super"]);
        // Lock and Mod5 are not modifiers this page reports.
        assert!(modifier_names(0x82).is_empty());
    }

    #[test]
    fn the_kind_follows_the_tag_the_engine_wrote() {
        assert_eq!(classify("key keyval=0x69 state=0x0"), Kind::Key);
        assert_eq!(classify("st cur=3 len=5 comp=1"), Kind::Insert);
        assert_eq!(classify("ins n=2 tail=***"), Kind::Insert);
        assert_eq!(classify("del mode=backspace n=3"), Kind::Delete);
        assert_eq!(classify("buf flush=per_key"), Kind::Flush);
        // A tag the chip row has no family for stays under "Tất cả" alone.
        assert_eq!(classify("ctx caps=0b1001"), Kind::Other);
        assert_eq!(classify("cfg ignore=foo err=bar"), Kind::Other);
    }

    #[test]
    fn a_failed_line_counts_as_an_error_whatever_its_family() {
        // `ok=0` is the writer's failure mark: the family tag stays, the chip
        // turns red, so a delete that failed is counted as a failure.
        assert_eq!(
            classify("del ok=0 mode=backspace why=inject_failed n=3"),
            Kind::Error
        );
        assert_eq!(classify("ins ok=0 why=echo_stale ms=800 n=2"), Kind::Error);
        assert_eq!(classify("buf ok=0 why=burst_full cap=40"), Kind::Error);
        // A line from an engine older than the tagged format still classifies.
        assert_eq!(classify("timed commit flush (per-key)"), Kind::Flush);
        assert_eq!(
            classify("st report cursor=3 len=5 composing=true"),
            Kind::Insert
        );
        assert_eq!(
            classify("delete-before inject FAILED → protocol=-1"),
            Kind::Error
        );
    }

    #[test]
    fn the_composing_flag_comes_from_the_report_field() {
        let log = vec![
            parse("1795619563000 st cur=0 len=0 comp=0", None).expect("a line"),
            parse("1795619563020 st cur=2 len=2 comp=1", None).expect("a line"),
        ];
        assert_eq!(composing(&log), Some(true));
        // A truncated field must not panic the page.
        let short = vec![parse("1795619563040 st cur=2 len=2 comp=", None).expect("a line")];
        assert_eq!(composing(&short), Some(false));
        assert_eq!(composing(&[]), None);
    }

    #[test]
    fn the_gap_is_measured_against_the_line_above() {
        let first = parse("1795619563084 hello", None).expect("a log line");
        assert_eq!(first.rel_ms, Some(0));
        let next = parse("1795619563154 hello", Some(1795619563084)).expect("a log line");
        assert_eq!(next.rel_ms, Some(70));
        // A clock that jumps back reports zero rather than a huge negative.
        let back = parse("1795619563000 hello", Some(1795619563154)).expect("a log line");
        assert_eq!(back.rel_ms, Some(0));
    }

    #[test]
    fn a_pasted_fragment_without_a_timestamp_still_decodes() {
        let line = parse("surrounding cursor=3 text=\"a\"", None).expect("a fragment");
        assert_eq!(line.rel_ms, None);
        assert_eq!(line.kind, Kind::Insert);
    }

    #[test]
    fn masking_removes_the_typed_text_and_keeps_the_shape() {
        let masked = mask_private("surrounding cursor=3 tail=\"Gia\" text=\"Gia\\n\\n\"");
        assert_eq!(
            // `Gia\n\n` is seven characters as written, escapes included.
            masked,
            "surrounding cursor=3 tail=\"***\" text=\"*******\"",
            "the fields stay, their content does not"
        );
        assert!(!masked.contains("Gia"), "{masked}");
        // A line with no private field is untouched.
        assert_eq!(
            mask_private("key keyval=0x69 state=0x0"),
            "key keyval=0x69 state=0x0"
        );
        // Both fields on one line are masked, not just the first.
        let two = mask_private("tail=\"ab\" text=\"cd\" extra=1");
        assert_eq!(two, "tail=\"**\" text=\"**\" extra=1");
    }

    #[test]
    fn masking_survives_an_escape_before_a_multibyte_character() {
        // A backslash in front of a Vietnamese letter is three bytes across
        // two characters; the walk has to stay on character boundaries so the
        // masked shape counts characters, escapes included.
        let masked = mask_private("text=\"\\à chào\"");
        assert_eq!(
            masked, "text=\"*******\"",
            "seven characters, escape included"
        );
        assert!(!masked.contains('à'), "{masked}");
    }
}
