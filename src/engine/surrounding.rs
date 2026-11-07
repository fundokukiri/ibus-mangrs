//! Surrounding-text bookkeeping: ingest client reports, then verify and
//! repair the believed `tail` against them.

use super::*;

impl Engine {
    /// Record the client's `SetSurroundingText(text, cursor, anchor)` report.
    /// Returns actions when the report resolves a deferred client `Reset`
    /// (the click-mid-preedit detection, see `on_client_reset`).
    pub fn set_surrounding(&mut self, text: String, cursor: u32) -> Vec<Action> {
        self.set_surrounding_with_anchor(text, cursor, cursor)
    }

    /// Same, with the selection anchor from the protocol report. A report
    /// with `anchor != cursor` describes a selection, not the word we
    /// committed: comparing it against `tail` and repairing would delete the
    /// selected text, so the report is dropped and the next caret-only one
    /// re-syncs.
    pub fn set_surrounding_with_anchor(
        &mut self,
        text: String,
        cursor: u32,
        anchor: u32,
    ) -> Vec<Action> {
        if anchor != cursor {
            debug_log::log(&format!(
                "st report with a selection anchor={anchor} cursor={cursor} → ignore"
            ));
            self.surrounding = None;
            return Vec::new();
        }
        let cursor = cursor as usize;
        let len = text.chars().count();
        // Cursor and length only — never the text. Per-report site: gate the
        // format so clients that report on every keystroke do not allocate
        // while the log is off.
        if debug_log::enabled() {
            debug_log::log(&format!(
                "st report cursor={cursor} len={len} composing={}",
                self.wrapper.has_composition()
            ));
        }
        // Settle the rewrite-delete delivery proof: a report whose prefix is
        // exactly the swallowed shape (delete missing, insert landed) proves
        // this client drops rewrite deletes. Without real-key injection
        // there is then no working delete transport, and effective_mode()
        // moves the following words to preedit. A stale report shows neither
        // delete nor insert, so it never matches the swallowed shape.
        if let Some((healthy, swallowed)) = self.awaiting_delete_proof.take() {
            if swallowed != healthy && cursor.min(len) == swallowed {
                self.delete_dropped = true;
                debug_log::log(
                    "rewrite delete swallowed (report proof) → following words leave delete rewriting",
                );
            }
        }
        // The previous report (only the inline-suggestion check below still
        // needs it); the incoming text is cloned once for the store.
        let pre_click = self.surrounding.replace((text.clone(), cursor as u32));
        // Inline autocomplete: the app appends suggestion text after the
        // caret without the caret moving past it. Only the transition from
        // "caret at the end" counts — text after the caret is normal when
        // editing mid-line from the first report on.
        if !self.suggestions && self.wrapper.has_composition() {
            if let Some((old_text, old_cursor)) = &pre_click {
                if *old_cursor as usize >= old_text.chars().count()
                    && cursor < len
                    && text.starts_with(old_text.as_str())
                {
                    self.suggestions = true;
                    debug_log::log(
                        "inline suggestion after the caret → preedit for following words",
                    );
                }
            }
        }
        Vec::new()
    }

    /// Compare what the client reports before the cursor with what we believe
    /// we committed (`tail`) and repair the difference.
    ///
    /// Works on string slices, never collecting the report: the report can be
    /// a whole paragraph and this runs on every keystroke. Only the short
    /// `tail` walks its own characters; every offset in the report is derived
    /// from the caret's byte position or stepped from the end.
    pub(super) fn verify_surrounding(&mut self, mode: Mode) -> Vec<Action> {
        // Only Surrounding verifies: plain NonPreedit must never send a
        // repair delete (the client did not ask for it, and a wrong repair
        // eats real text).
        if mode != Mode::Surrounding || self.tail.is_empty() {
            return Vec::new();
        }
        let Some((text, cursor)) = &self.surrounding else {
            return Vec::new();
        };
        // Byte offset of the caret inside the report — past the end when the
        // cursor overshoots, the same clamp a `take(cursor)` collect had.
        let prefix_end = text
            .char_indices()
            .nth(*cursor as usize)
            .map_or(text.len(), |(byte, _)| byte);
        let actual: &str = &text[..prefix_end];
        let tail: &str = &self.tail;
        if actual.ends_with(tail) {
            self.repair_delete = None;
            return Vec::new();
        }

        if !self.deletes_allowed() {
            return Vec::new();
        }
        // A report that does not even contain the believed word is stale (it
        // predates our last commit — fast typing) or reflects an app-side
        // deletion we cannot safely undo; either way, do nothing rather than
        // tear the composition down (this used to eat characters).
        let contains = actual.contains(tail);
        // The caret is the insertion point, so the believed word must end at
        // or right before it. Chrome's find bar reports a page fragment that
        // merely contains the typed word, with the caret far past it;
        // adopting that text deleted the real input ("em tun" → "g").
        let gap = contains.then(|| suffix_gap(actual, tail)).flatten();
        if gap.is_some_and(|gap| gap > 2) {
            debug_log::log(&format!(
                "verify: ignore unanchored tail_len={} gap={}",
                tail.chars().count(),
                gap.unwrap_or(0),
            ));
            debug_log::log_text(&format!(
                "verify: ignore unanchored tail={:?} actual={:?}",
                brief(tail),
                brief(actual),
            ));
            return Vec::new();
        }
        let k = common_suffix_len(tail, actual);
        let a2 = &actual[..byte_before_suffix(actual, k)];
        let t2 = &tail[..byte_before_suffix(tail, k)];
        let a2_starts_t2 = a2.starts_with(t2);
        // `a2 longer than t2`: once a2 starts with t2 the counts differ
        // exactly when a char exists past t2's length — bounded by the tail,
        // never a full walk of the report.
        let extra_in_middle =
            a2_starts_t2 && a2.chars().take(t2.chars().count() + 1).count() > t2.chars().count();
        if !extra_in_middle && !contains {
            debug_log::log(&format!(
                "verify: ignore stale tail_len={} actual_bytes={}",
                tail.chars().count(),
                actual.len(),
            ));
            debug_log::log_text(&format!(
                "verify: ignore stale tail={:?} actual={:?}",
                brief(tail),
                brief(actual),
            ));
            return Vec::new();
        }
        if extra_in_middle {
            // The client holds extra characters right before the common
            // suffix — a `DeleteSurroundingText` it swallowed earlier.
            let extra = a2.chars().count() - t2.chars().count();
            debug_log::log(&format!("verify: drift suffix={k} extra={extra}"));
            debug_log::log_text(&format!(
                "verify: drift tail={:?} actual={:?}",
                brief(tail),
                brief(actual),
            ));
            if self.repair_delete_forbidden() {
                // Mutter-relayed contexts: a repair delete here segfaults the
                // compositor. Accept the drift (the next healthy report or a
                // rebuild clears it) instead of risking the session.
                self.repair_delete = None;
                return Vec::new();
            }
            // Copied before any `&mut self`: `actual` borrows the stored
            // report, and the borrow must end before the engine state moves.
            let commit = actual[a2.len()..].to_string();
            let extra = extra as u32;
            let offset = -((k as i32) + extra as i32);
            if self.repair_delete.is_some() || self.delete_dropped {
                // A repair was already attempted since the last verified
                // state and the text is still off: the client drops the
                // protocol delete (the drift shape may differ each time).
                // Press real Backspace keys instead — (k + extra) presses
                // remove the extra characters and the common suffix, then the
                // suffix is re-typed.
                self.delete_dropped = true;
                self.repair_delete = None;
                return vec![Action::RewriteByBackspace {
                    backspaces: k as u32 + extra,
                    commit,
                }];
            }
            self.repair_delete = Some((offset, extra));
            return vec![Action::DeleteSurrounding { offset, n: extra }];
        }
        // Some other desync (the user edited the text, another IME wrote to
        // it...). Never delete blindly: rebuild the composition from what the
        // application actually holds.
        debug_log::log(&format!(
            "verify: sync-from-surrounding prefix_len={}",
            actual.chars().count(),
        ));
        debug_log::log_text(&format!(
            "verify: sync-from-surrounding actual={:?}",
            brief(actual),
        ));
        // Owned copy for the same borrow reason as above; the sync path is
        // the rare app-edited-the-text case, not the per-keystroke path.
        let prefix = actual.to_string();
        self.sync_from_surrounding(&prefix);
        Vec::new()
    }

    /// Align the engine with the application's text after an unrecoverable
    /// desync (the app rewrote its own text, a suggestion replaced the word,
    /// the user edited by hand...). The wrapper is rebuilt from the real text
    /// so tone keys keep working on the word that is actually there; the
    /// wrapper's own `last_output` tracking keeps the next delta correct
    /// despite the rebuild.
    fn sync_from_surrounding(&mut self, actual_prefix: &str) {
        let text = actual_prefix.to_string();
        self.repair_delete = None;
        if self.charset == Charset::Unicode {
            self.wrapper.rebuild(&text);
            self.tail = self.wrapper.output().to_string();
        } else {
            // Encoded text cannot be rebuilt faithfully; just adopt it.
            self.wrapper.reset();
            self.tail = text
                .rsplit(|c: char| crate::bamboo_wrapper::is_word_terminator(c))
                .next()
                .unwrap_or("")
                .to_string();
        }
    }
}

/// Chars from the end of the last `tail` occurrence to the end of `actual`,
/// or `None` when `actual` does not contain `tail`.
fn suffix_gap(actual: &str, tail: &str) -> Option<usize> {
    let pos = actual.rfind(tail)?;
    Some(actual[pos + tail.len()..].chars().count())
}

/// Length of the common character suffix of `tail` and `actual`.
fn common_suffix_len(tail: &str, actual: &str) -> usize {
    tail.chars()
        .rev()
        .zip(actual.chars().rev())
        .take_while(|(a, b)| a == b)
        .count()
}

/// Byte offset of the boundary before the last `k` characters of `s`
/// (the whole string when `k` is 0) — steps from the end, O(k).
fn byte_before_suffix(s: &str, k: usize) -> usize {
    if k == 0 {
        return s.len();
    }
    s.char_indices()
        .rev()
        .nth(k - 1)
        .map_or(0, |(byte, _)| byte)
}
