//! The main key path: `process_char`, the Off path, the uinput
//! (Flow/Turbo/Select) paths, and the word-end macro/auto-restore/rewrite machinery.

use super::*;

/// How a word end relates to a shortcode candidate word: `None` is a normal
/// word; `Word` is a candidate ending on another terminator (it never
/// word-macro-expands); `Close` is the closing ':' of `:name:` (it also
/// restores the raw keys when the lookup matched nothing).
pub(super) enum CandidateEnd {
    None,
    Word,
    Close,
}

impl Engine {
    /// Process one character (already decoded from a keysym).
    pub fn process_char(&mut self, c: char) -> Vec<Action> {
        // The idle-commit clock: only stamped while the feature is on, so
        // the default path costs nothing.
        if self.idle_commit_enabled {
            self.last_key_at = Some(Instant::now());
        }
        // Re-enter an idle-committed word: the next word character un-commits
        // it (a protocol delete erases the committed text) and replays the
        // raw keys into a live composition, so the word continues as if the
        // pause never happened. Only when the delete is provably safe AND
        // can undo the whole word (the last report must have seen it — the
        // clamp hard rule): otherwise the commit stays final and the key
        // starts a fresh word. Any terminator discards the re-entry instead.
        let mut reentry_delete: Option<Action> = None;
        if let Some((raw, committed)) = self.idle_committed.take() {
            let need = committed.chars().count() as u32;
            if !crate::bamboo_wrapper::is_word_terminator(c)
                && self.effective_mode() == Mode::Preedit
                && self.protocol_delete_ok()
                && self.clamp_delete(need) == need
            {
                debug_log::log("idle commit re-entry → un-commit and re-compose");
                self.wrapper.replay_raw(&raw);
                reentry_delete = Some(Action::DeleteBefore(need));
            }
        }
        // Emoji shortcode candidate: the word typed directly after ':'
        // composes like any word — the closing ':' looks the raw keys up
        // (macros first, then the emoji table) and rewrites the shown text
        // to the emoji; any other word end leaves the composed word, so
        // Vietnamese typing after ':' works. Runs even with Vietnamese
        // processing off — shortcodes are not marks, they are replacements
        // (see `process_char_off`).
        if !self.emoji_word
            && self.emoji_enabled
            && !self.wrapper.has_composition()
            && !self.url_word
            && self.last_char == Some(':')
            && !crate::bamboo_wrapper::is_word_terminator(c)
        {
            self.emoji_word = true;
        }

        if !self.wrapper.is_enabled() || self.effective_mode() == Mode::Off {
            return self.process_char_off(c);
        }

        let mode = self.key_mode();

        // URL bypass, two ways in: a real URL (`://` completed by this `/`,
        // or the word `www` ending at the previous `.`) opens a span that
        // keeps the whole address raw until a sentence boundary; the user's
        // own opt-in `extra_url_chars` keeps a word raw for that word only.
        // No built-in character starts a bypass — punctuation in prose never
        // blocks marks. The URL evidence comes from the word that just ended
        // (`note_url_pending`); `last_char` is its terminator.
        if !self.url_word && !self.wrapper.has_composition() {
            let scheme_arm =
                c == '/' && self.scheme_pending && matches!(self.last_char, Some(':') | Some('/'));
            let www_arm = self.www_pending && !crate::bamboo_wrapper::is_word_terminator(c);
            let per_word = !self.extra_url_chars.is_empty()
                && self
                    .last_char
                    .is_some_and(|p| self.extra_url_chars.contains(&p));
            if self.bypass_url && (scheme_arm || www_arm || per_word) {
                self.url_word = true;
                self.url_span = scheme_arm || www_arm;
                self.raw_word_start = self.tail.chars().count();
            }
        }
        // The URL evidence is only good for the key right after it.
        self.scheme_pending = false;
        self.www_pending = false;
        if self.url_word {
            let mut actions = Vec::new();
            if self.preedit_visible {
                self.preedit_visible = false;
                actions.push(Action::HidePreedit);
            }
            // Span mode: the URL-internal separators belong to the address
            // and keep it raw (`?x=1&y=2#top`, hyphens). Per-word mode: only
            // `-`/`+` stay inside (`--` must still become an em dash in
            // normal text).
            let keep = if self.url_span {
                URL_SPAN_KEEP.contains(&c)
            } else {
                matches!(c, '-' | '+')
            };
            if crate::bamboo_wrapper::is_word_terminator(c) && !keep {
                self.url_word = false;
                self.url_span = false;
                self.emoji_word = false;
                self.tail.clear();
            } else {
                self.tail.push(c);
            }
            self.note_char(c);
            actions.push(Action::Forward);
            return actions;
        }

        // The uinput modes: the raw key goes straight to the application and
        // bamboo keeps composing internally; nothing is committed until the
        // word ends (see `process_char_uinput`). Per-key display features
        // (em-dash, double space) stay off here — these modes are for
        // terminals and games, where rewriting before the word ends cannot
        // work anyway.
        if matches!(mode, Mode::Flow | Mode::Turbo | Mode::Select) {
            return self.process_char_uinput(mode, c);
        }

        // `--` -> `—`: the first `-` was already forwarded by the terminator
        // path, so this key replaces it.
        // The rewrite needs a delete transport; without one the second '-'
        // stays literal instead of duplicating the replacement.
        if self.em_dash
            && (self.deletes_allowed() || forwards_delete(mode))
            && c == '-'
            && self.last_char == Some('-')
        {
            let mut actions = vec![self.delete_action(mode, 1, 1)];
            actions.push(Action::Commit("—".into()));
            self.tail = replace_tail(&self.tail, 1, "—");
            self.note_char('—');
            return actions;
        }

        // Double space -> ". ": drop the first space, insert the period, let
        // this space through.
        if self.double_space_period
            && (self.deletes_allowed() || forwards_delete(mode))
            && c == ' '
            && self.last_char == Some(' ')
        {
            let mut actions = self.hide_if_visible();
            actions.push(self.delete_action(mode, 1, 1));
            actions.push(Action::Commit(".".into()));
            self.tail = replace_tail(&self.tail, 1, ".");
            self.note_char(' ');
            actions.push(Action::Forward);
            return actions;
        }

        // If the resolved mode is not preedit but a stale preedit string is
        // still on screen (mode resolved per client, ST data arriving later),
        // clear it first so the client never shows both preedit and commits.
        let mut actions = Vec::new();
        if let Some(del) = reentry_delete {
            // The un-commit delete must reach the client before the preedit
            // string that replaces the committed text.
            actions.push(del);
        }
        if mode != Mode::Preedit && self.preedit_visible {
            actions.push(Action::HidePreedit);
        }

        // Surrounding-text verification runs before the key changes anything,
        // while the composition still matches `tail`.
        actions.extend(self.verify_surrounding(mode));

        // Auto-capitalization: first letter of a word that starts a sentence.
        // The uppercase character is fed to the composition itself, so
        // bamboo-core keeps the word's case for later transformations.
        let cap = self.auto_capitalize
            && c.is_alphabetic()
            && !self.wrapper.has_composition()
            && self.capitalize_next;
        let c = if cap {
            c.to_uppercase().next().unwrap_or(c)
        } else {
            c
        };

        match self.wrapper.process_char(c) {
            Outcome::PassThrough => {
                if cap {
                    // The app would insert the raw (lowercase) key itself, so
                    // commit the uppercase letter instead of forwarding.
                    let upper: String = c.to_uppercase().collect();
                    self.tail.push_str(&upper);
                    self.note_char(c);
                    actions.push(Action::Commit(upper));
                    actions
                } else {
                    self.tail.push(c);
                    self.note_char(c);
                    actions.push(Action::Forward);
                    actions
                }
            }
            Outcome::Finish {
                word,
                raw,
                valid,
                valid_prefix,
            } => {
                if self.emoji_word && c == ':' {
                    // The closing ':' of a shortcode: try the rewrite while
                    // the flag is still up (`uinput_rewrite` keys its
                    // per-key branch on it). No URL evidence is recorded —
                    // a shortcode context is not prose.
                    let close = self.emoji_close_rewrite(mode, &word, &raw);
                    self.emoji_word = false;
                    if let Some(actions) = close {
                        return actions;
                    }
                    // Miss: the shortcode stays as typed — the raw keys, not
                    // the composed word, and no word-macro expansion.
                    actions.extend(self.on_finish(
                        mode,
                        word,
                        raw,
                        valid,
                        valid_prefix,
                        CandidateEnd::Close,
                    ));
                    self.note_char(c);
                    return actions;
                }
                let candidate = if self.emoji_word {
                    CandidateEnd::Word
                } else {
                    CandidateEnd::None
                };
                self.emoji_word = false;
                // The word just ended on this terminator: record URL
                // evidence (`://` scheme, `www.`) before the state is cleared.
                self.note_url_pending(&raw, c);
                actions.extend(self.on_finish(mode, word, raw, valid, valid_prefix, candidate));
                // The terminator itself reaches the application last.
                self.note_char(c);
                actions
            }
            Outcome::Edit(edit) => {
                actions.extend(self.on_edit(mode, edit));
                actions
            }
        }
    }

    /// The closing ':' of a shortcode candidate: look the raw name up —
    /// macros first, then the emoji tables — and rewrite the shown
    /// `:<shown>` text through the same delete+commit machinery as the
    /// macro expansion. `shown` is the text between the colons as the
    /// application holds it (the composed word in the inline modes, the raw
    /// keys in the uinput and Off paths); `raw` is what the user typed, and
    /// what the lookup uses. `None` = no match (or no delete transport):
    /// the caller leaves the text as typed and lets the ':' close it.
    /// Nothing about the typed text is logged (engine rule).
    fn emoji_close_rewrite(&mut self, mode: Mode, shown: &str, raw: &str) -> Option<Vec<Action>> {
        let name = raw.to_lowercase();
        // A user macro keyed ":name:" wins over the emoji table, the
        // same precedence macros have over every other replacement.
        let replacement = self
            .macros
            .expand(
                &format!(":{name}:"),
                false,
                &self.time_format,
                &self.date_format,
            )
            .or_else(|| self.emojis.lookup(&name).map(str::to_string))?;
        debug_log::log("emoji shortcode: matched");
        if self.flow_per_key && matches!(mode, Mode::Flow | Mode::Turbo) {
            // None: the closing ':' is part of the shortcode and must not
            // reach the application. Land the in-flight batch first: the
            // erase must count against the screen the tail models. The
            // committed ':' sits one char before the candidate's region, so
            // the erase covers both — and the commit stays synchronous,
            // riding the server's echo wait (the per-key coalesce would
            // diff against the region alone and leave the ':' on screen).
            let mut actions = Vec::new();
            if let Some((_, text)) = self.per_key_park.take() {
                actions.push(Action::CommitNow(text));
            }
            let n = (1 + self.tail.chars().count()) as u32;
            if n > 1 {
                actions.push(Action::InjectBackspaces(n));
            }
            match self.unicode_channel {
                UnicodeChannel::Commit => actions.push(Action::Commit(replacement.clone())),
                UnicodeChannel::Clipboard => actions.push(Action::Paste(replacement.clone())),
            }
            if self.post_delay_ms > 0 {
                actions.push(Action::PostDelay(self.post_delay_ms));
            }
            self.tail = replacement;
            if let Some(ch) = self.tail.chars().last() {
                self.note_char(ch);
            }
            return Some(actions);
        }
        if matches!(mode, Mode::Flow | Mode::Turbo | Mode::Select) {
            // Per-key off (plain display: the screen holds the raw keys) or
            // Select (its own erase shape): the ':' is covered by erasing
            // one more key than the name.
            let mut actions = Vec::new();
            actions.extend(self.uinput_rewrite(mode, &format!(":{shown}"), &replacement, None));
            return Some(actions);
        }
        if !(self.deletes_allowed() || forwards_delete(mode)) {
            // No delete transport: the rewrite would duplicate the
            // replacement over the composed text — keep the text as typed.
            return None;
        }
        let mut actions = Vec::new();
        if mode == Mode::Preedit {
            // The candidate word is still live preedit: hide it and erase
            // only the committed leading ':'.
            actions.extend(self.hide_if_visible());
            actions.push(self.delete_action(mode, 1, replacement.chars().count()));
        } else {
            actions.push(self.delete_action(
                mode,
                (1 + shown.chars().count()) as u32,
                replacement.chars().count(),
            ));
        }
        actions.push(Action::Commit(replacement));
        self.note_char(':');
        Some(actions)
    }

    /// Clamp a delete count to what the client's last surrounding-text report
    /// says exists before the cursor. Defensive against app-side rewrites
    /// racing between our verify and the delete (mutter segfaults on
    /// out-of-range deletes even with the capability present).
    fn clamp_delete(&self, n: u32) -> u32 {
        match &self.surrounding {
            Some((text, cursor)) => {
                let have = text.chars().take(*cursor as usize).count() as u32;
                n.min(have)
            }
            None => n,
        }
    }

    /// Prefix length (in chars) of the last ST report — what the client
    /// showed before the cursor when it last spoke.
    fn report_prefix_len(&self) -> usize {
        self.surrounding.as_ref().map_or(0, |(text, cursor)| {
            (*cursor as usize).min(text.chars().count())
        })
    }

    /// The delete half of a rewrite for this keystroke: the protocol delete
    /// (or the server's uinput fallback) in the inline modes, key events
    /// forwarded through IBus in the two ForwardKeyEvent modes.
    pub(super) fn delete_action(&mut self, mode: Mode, n: u32, insert_len: usize) -> Action {
        match mode {
            Mode::ForwardKeyEvent => Action::ForwardBackspaces(n),
            Mode::ForwardSelect => Action::ForwardSelectLeft(n),
            _ => self.tracked_delete(n, insert_len),
        }
    }

    /// Delete count for this keystroke. The forward modes delete with real key
    /// events, which remove exactly what is on screen; clamping to the last
    /// surrounding-text report would under-delete there (the report can
    /// predate the commit). The protocol path keeps the clamp.
    pub(super) fn delete_count(&self, mode: Mode, n: u32) -> u32 {
        if forwards_delete(mode) {
            n
        } else {
            self.clamp_delete(n)
        }
    }

    /// A `DeleteBefore` whose delivery the next ST report proves or refutes
    /// (see `awaiting_delete_proof`). Pure deletions (no paired insert)
    /// cannot be told apart from a stale report, so they stay untracked.
    fn tracked_delete(&mut self, n: u32, insert_len: usize) -> Action {
        // Hard rule (see `delete_count`): never ask for more characters than
        // the last report says sit before the caret. An out-of-range delete
        // aborts mutter (`before >= start`) and hangs the desktop.
        let n = self.clamp_delete(n);
        if insert_len == 0 {
            return Action::DeleteBefore(n);
        }
        let prefix = self.report_prefix_len();
        let healthy = prefix.saturating_sub(n as usize) + insert_len;
        let swallowed = prefix + insert_len;
        if swallowed != healthy {
            self.awaiting_delete_proof = Some((healthy, swallowed));
        }
        Action::DeleteBefore(n)
    }

    /// English mode: forward everything, but macros still work when
    /// `macro_in_off_mode` is set (the wrapper tracks the raw keys).
    fn process_char_off(&mut self, c: char) -> Vec<Action> {
        match self.wrapper.process_char(c) {
            Outcome::Finish { raw, valid, .. } => {
                if self.emoji_word && c == ':' {
                    // The closing ':' of a shortcode in Off mode: the name
                    // is the tracked tail (everything is raw here).
                    let shown = std::mem::take(&mut self.tail);
                    let mode = self.effective_mode();
                    let close = self.emoji_close_rewrite(mode, &shown, &raw);
                    self.emoji_word = false;
                    if let Some(actions) = close {
                        return actions;
                    }
                    // Miss: fall through — the raw text stays and the ':'
                    // closes the shortcode as typed.
                }
                let candidate = self.emoji_word;
                self.emoji_word = false;
                // Expansion rewrites committed text; without a delete
                // transport the commit would duplicate the raw keys.
                // A shortcode candidate word never word-macro-expands.
                if !candidate
                    && self.macro_enabled
                    && self.macro_in_off_mode
                    && !raw.is_empty()
                    && self.deletes_allowed()
                {
                    if let Some(exp) = self.expand_macro(&raw, valid) {
                        let mut actions = Vec::new();
                        if !self.tail.is_empty() {
                            actions.push(self.tracked_delete(
                                self.tail.chars().count() as u32,
                                exp.chars().count(),
                            ));
                        }
                        actions.push(Action::Commit(exp.clone()));
                        // Word ended: drop the tracked tail (same reason as
                        // `on_finish`).
                        if let Some(c) = exp.chars().last() {
                            self.note_char(c);
                        }
                        self.tail.clear();
                        actions.push(Action::Forward);
                        return actions;
                    }
                }
                self.tail.clear();
                self.note_char(c);
                vec![Action::Forward]
            }
            _ => {
                self.tail.push(c);
                self.note_char(c);
                vec![Action::Forward]
            }
        }
    }

    fn expand_macro(&self, raw: &str, valid: bool) -> Option<String> {
        if !self.macro_enabled {
            return None;
        }
        // A key sequence that already forms a valid Vietnamese word is much
        // more likely normal typing than a shorthand ("cam", "moi") — do
        // not expand it. `macro_expand_valid_words` opts out of the guard:
        // a colliding shorthand then makes that word reachable only by
        // backspacing one key out.
        if valid && !self.macro_expand_valid_words {
            return None;
        }
        self.macros.expand(
            raw,
            self.capitalize_macro,
            &self.time_format,
            &self.date_format,
        )
    }

    /// The preedit string shown to the client. The preedit is sent with
    /// `IBUS_ENGINE_PREEDIT_COMMIT`, so the client commits whatever it
    /// displays when it resets the context (a mouse click, a focus change):
    /// the display must therefore already be the final text — the macro
    /// expansion and the auto-restore to the raw keys, exactly like
    /// ibus-bamboo shows the processed string in its preedit.
    pub(super) fn preedit_text(&self) -> String {
        let word = self.wrapper.output();
        let raw = self.wrapper.raw();
        if raw.is_empty() {
            return word;
        }
        let valid = self.wrapper.is_valid();
        if let Some(exp) = self.expand_macro(raw, valid) {
            return exp;
        }
        if self.should_auto_restore(&word, raw, self.wrapper.is_valid_prefix()) {
            return raw.to_string();
        }
        word
    }

    /// Auto Non-VN restore decision: the composed word differs from the raw
    /// keys and is not a Vietnamese syllable, so the raw keys go back. The
    /// check is the prefix one — what ibus-bamboo uses while typing — so a
    /// still-extendable state like `đuo` stays composed instead of flickering
    /// back to `dduo`. A word with an undone mark (`tê` + 'e' → `tee`), or one
    /// in the dictionary, is never restored. Shared by the preedit display and
    /// the word end so the client commits the same string a word break would.
    fn should_auto_restore(&self, word: &str, raw: &str, valid_prefix: bool) -> bool {
        if !self.auto_restore || word.is_empty() || word == raw || valid_prefix {
            return false;
        }
        let dict_hit = self
            .dictionary
            .as_ref()
            .is_some_and(|d| d.contains(&word.to_lowercase()));
        !dict_hit && !self.wrapper.undo_in_word()
    }

    /// The uinput modes (Flow / Turbo / Select): the application
    /// keeps the raw keystroke; bamboo transforms internally. The rewrite
    /// happens once, at word end — unless the per-key display is on, where
    /// every composition-changing key applies immediately.
    fn process_char_uinput(&mut self, mode: Mode, c: char) -> Vec<Action> {
        let mut actions = Vec::new();
        // Auto-capitalize: the application keeps the raw key, so the capital
        // can only appear in the word-end rewrite. Decide at the first key —
        // the per-key `note_char` below clears `capitalize_next` as soon as
        // the word starts.
        if !self.wrapper.has_composition() {
            self.cap_word = self.auto_capitalize && self.capitalize_next && c.is_alphabetic();
            // Per-key display: the previous word's region has landed (its
            // own commit or the batch's) — this word's region starts empty,
            // so the coalesce diffs and erases stay INSIDE the word and can
            // never touch what came before it. A batch still in flight
            // spans the boundary: keep coalescing onto it — EXCEPT for a
            // shortcode candidate: its closing-':' erase must cover the
            // committed ':' one char before the region, so a spanning batch
            // is landed now and the region starts at the word.
            if self.flow_per_key {
                if self.emoji_word {
                    if let Some((_, text)) = self.per_key_park.take() {
                        actions.push(Action::CommitNow(text));
                    }
                    self.tail.clear();
                } else if self.per_key_park.is_none() {
                    self.tail.clear();
                }
            }
        }
        let per_key = self.flow_per_key && matches!(mode, Mode::Flow | Mode::Turbo);
        match self.wrapper.process_char(c) {
            Outcome::PassThrough => {
                // The key produced no composition change (an inapplicable
                // mark key, say) — it is still a literal on screen.
                if per_key {
                    // The literal joins the coalesced batch; the wrapper
                    // re-syncs its own delta tracking to the raw keys on
                    // this path. `want` is built from the pre-key tail —
                    // coalescing with the tail already grown would look
                    // like "no change" and cancel the pending insert.
                    let want = format!("{}{}", self.tail, c);
                    actions.extend(self.per_key_coalesce(want));
                    return actions;
                }
                self.tail.push(c);
                self.note_char(c);
                actions.push(Action::Forward);
                actions
            }
            Outcome::Edit(edit) => {
                // The transformed output stays internal; the application
                // shows the raw key until the word ends. With the per-key
                // display the composed delta applies immediately through
                // the injected transport.
                if per_key {
                    let want = if self.charset == Charset::Unicode {
                        replace_tail(&self.tail, edit.delete_chars, &edit.insert)
                    } else {
                        // Legacy encodings change the character count: the
                        // screen must hold the whole re-encoded tail.
                        encode(self.charset, &self.wrapper.output())
                    };
                    actions.extend(self.per_key_coalesce(want));
                    return actions;
                }
                actions.push(Action::Forward);
                actions
            }
            Outcome::Finish {
                word, raw, valid, ..
            } => {
                actions.extend(self.on_finish_uinput(mode, word, raw, valid, c));
                actions
            }
        }
    }

    /// The per-key display's single in-flight batch: `want` is the text the
    /// screen must hold once the batch lands. The diff against what the
    /// application certainly holds (the tail minus the pending insert)
    /// becomes extra injected Backspaces, and the pending commit's text is
    /// REPLACED — one commit per batch, emitted by the main-loop tick past
    /// the deadline, so deletes can never overtake it and fast typing
    /// coalesces instead of mis-ordering. An empty diff cancels the park.
    fn per_key_coalesce(&mut self, want: String) -> Vec<Action> {
        let pending_len = self
            .per_key_park
            .as_ref()
            .map_or(0, |(_, t)| t.chars().count());
        let h_len = self.tail.chars().count().saturating_sub(pending_len);
        let h: String = self.tail.chars().take(h_len).collect();
        let common = h
            .chars()
            .zip(want.chars())
            .take_while(|(a, b)| a == b)
            .count();
        let d = h.chars().count() - common;
        let insert: String = want.chars().skip(common).collect();
        let mut actions = Vec::new();
        if d > 0 {
            actions.push(Action::InjectBackspaces(d as u32));
        }
        if insert.is_empty() {
            // The pending insert is fully retracted: cancel the park — the
            // deletes erase back to the common prefix on their own.
            self.per_key_park = None;
        } else {
            let deadline = Instant::now() + self.per_key_delay();
            match &mut self.per_key_park {
                Some((dl, text)) => {
                    text.clear();
                    text.push_str(&insert);
                    *dl = deadline;
                }
                slot @ None => *slot = Some((deadline, insert)),
            }
        }
        self.tail = want;
        if let Some(c) = self.tail.chars().last() {
            self.note_char(c);
        }
        actions
    }

    /// The uinput modes' Backspace under the per-key display. The screen
    /// holds the committed word, so the user's real Backspace — the one key
    /// that never needs injecting — deletes the last character through the
    /// pass-through path: on a mutter relay the injected Backspaces lost
    /// their echo presses while the commit landed, and the word duplicated.
    /// The raw↔screen mapping survives by reverse-mapping: raw keys are
    /// popped until the recomposition matches the post-delete screen, so
    /// later keys keep transforming the word (`tieng` BS `g` `s` still
    /// gives `tiếng`). A word whose post-delete screen no raw prefix
    /// reproduces (no raw keys at all — the rebuild repair path — or an
    /// encoding that changes character counts) is released — plain text
    /// from there, the accepted cost of dropping the injection.
    pub(super) fn backspace_per_key(&mut self) -> Vec<Action> {
        let mut actions = Vec::new();
        // The pass-through Backspace deletes one character of the FINAL
        // screen: land the in-flight batch first, or the key deletes a
        // character the batch was still going to add.
        if let Some((_, text)) = self.per_key_park.take() {
            debug_log::log(&format!(
                "per-key backspace lands the batch len={}",
                text.chars().count()
            ));
            actions.push(Action::CommitNow(text));
        }
        let target = replace_tail(&self.tail, 1, "");
        let raw_len = self.wrapper.raw().chars().count();
        let mut matched = false;
        for _ in 0..raw_len {
            if self.wrapper.backspace().is_none() {
                break;
            }
            if encode(self.charset, &self.wrapper.output()) == target {
                matched = true;
                break;
            }
        }
        if !matched && self.flow_backspace_retrack {
            // The pop search only deletes whole raw keys — a screen whose
            // tone outlives its key (`tùng` -> `tùn`) never matches. The
            // re-encoder rebuilds a live composition for it instead, so the
            // next mark key keeps editing here too.
            matched = self.wrapper.retrack_from_screen(&target);
        }
        if matched {
            // Tracking survives: the recomposition IS the post-delete
            // screen, so the next keys keep transforming the word.
            self.tail = target;
            if !self.wrapper.has_composition() {
                self.release_word_mode();
            }
        } else {
            // The word cannot stay tracked; reset also clears the pops a
            // failed search left behind. The screen edit is still the
            // user's own Backspace — the word just becomes plain text.
            self.wrapper.reset();
            self.release_word_mode();
            self.tail = target;
            self.idle_committed = None;
        }
        self.note_backspace_forwarded();
        actions.push(Action::Forward);
        actions
    }

    /// Word end in a uinput mode: erase the latin tail with real keys and
    /// deliver the Vietnamese word. When the word equals the raw keys
    /// (English text, auto-restored words) nothing is touched — the keys
    /// already sit in the application.
    fn on_finish_uinput(
        &mut self,
        mode: Mode,
        word: String,
        raw: String,
        valid: bool,
        c: char,
    ) -> Vec<Action> {
        let _ = valid;
        // Per-key display: `tail` is the text the application holds (the
        // progressively committed word) — the word-end rewrites below erase
        // by it, so it must survive.
        let per_key = self.flow_per_key && matches!(mode, Mode::Flow | Mode::Turbo);
        let candidate = self.emoji_word;
        self.emoji_word = false;
        if candidate && c == ':' {
            // The closing ':' of a shortcode: the rewrite erases the shown
            // name plus the committed ':' before it, and no URL evidence is
            // recorded — a shortcode context is not prose.
            if let Some(actions) = self.emoji_close_rewrite(mode, &raw, &raw) {
                return actions;
            }
            // Miss: the shortcode stays as typed. Per-key: the composed
            // name is on screen, so the raw keys + ':' go through the batch.
            // Plain: the raw keys are already there — forward the ':'.
            if per_key {
                return self.uinput_rewrite(mode, &raw, &raw, Some(c));
            }
            self.note_char(c);
            return vec![Action::Forward];
        }
        // The word just ended on this terminator: record URL evidence
        // (`://` scheme, `www.`) before the state is cleared.
        self.note_url_pending(&raw, c);
        // Taken before `note_char` below updates the flag for the next word.
        let cap = std::mem::take(&mut self.cap_word);
        self.release_word_mode();
        if !per_key {
            self.tail.clear();
        }
        self.note_char(c);

        // Macros win, as in every mode: the expansion replaces the raw keys.
        // A shortcode candidate word never word-macro-expands — the ':name:'
        // key is the emoji path's own lookup.
        if !raw.is_empty() && !candidate {
            if let Some(exp) = self.expand_macro(&raw, valid) {
                return self.uinput_rewrite(mode, &raw, &exp, Some(c));
            }
        }

        let word = if cap { capitalize_first(&word) } else { word };
        if word == raw || word.is_empty() {
            // Nothing was transformed: no rewrite, no flash. Per-key: the
            // terminator still joins the in-flight batch — a forwarded key
            // would race the pending commit.
            if per_key {
                return self.uinput_rewrite(mode, &raw, &word, Some(c));
            }
            return vec![Action::Forward];
        }

        // Auto Non-VN restore: bamboo "corrected" a word that is not a
        // Vietnamese syllable (English typing). The raw keys already sit in
        // the application — keep them instead of rewriting the word into a
        // syllable the user never meant (same gate as the inline modes).
        let dict_hit = self
            .dictionary
            .as_ref()
            .is_some_and(|d| d.contains(&word.to_lowercase()));
        if self.auto_restore && !valid && !dict_hit && !self.wrapper.undo_in_word() {
            self.wrapper.reset();
            // The app holds the raw keys, so auto-capitalize still needs the
            // rewrite — otherwise the sentence starts lowercase. Per-key: the
            // app holds the composed word, so even an unchanged `text` needs
            // the rewrite (the coalesce diffs against what really landed).
            let text = if cap {
                capitalize_first(&raw)
            } else {
                raw.clone()
            };
            if text == raw && !per_key {
                return vec![Action::Forward];
            }
            return self.uinput_rewrite(mode, &raw, &text, Some(c));
        }

        // Flow (unlike Turbo) defers to app autofill: the app
        // appended suggestion text after the caret, and a rewrite would eat
        // it. Turbo rewrites regardless — minimum latency, the owner
        // accepted the risk. Per-key rewrites are backward-only (the
        // suggestion sits after the caret), so the hold-back is moot.
        if mode == Mode::Flow && self.suggestions && !per_key {
            self.wrapper.reset();
            return vec![Action::Forward];
        }

        self.uinput_rewrite(mode, &raw, &word, Some(c))
    }

    /// The shared word-end rewrite: erase `raw` with real keys, deliver
    /// `text` over the configured Unicode channel. `terminator` is the key
    /// that ended the word; it travels inside the parked commit so it is
    /// applied strictly after the deletes on every display server (a
    /// mutter relay delivered a forwarded terminator ahead of the
    /// Backspaces, the erase ate it and one raw letter survived — and the
    /// terminator's own injected-key echo never returned through the
    /// relay's ibus path at all, so a key-event terminator is unusable
    /// there).
    fn uinput_rewrite(
        &mut self,
        mode: Mode,
        raw: &str,
        text: &str,
        terminator: Option<char>,
    ) -> Vec<Action> {
        // Per-key display: the whole rewrite coalesces into the in-flight
        // batch — the diff against what the app certainly holds becomes
        // extra Backspaces and the pending commit's text is replaced. The
        // terminator joins the batch too (a forwarded key would race it).
        if self.flow_per_key && matches!(mode, Mode::Flow | Mode::Turbo) {
            let want = match terminator {
                Some(c) => format!("{text}{c}"),
                None => text.to_string(),
            };
            return self.per_key_coalesce(want);
        }
        let mut actions = Vec::new();
        match mode {
            Mode::Select => {
                // No text report -> mode resolution fell back to Preedit, so
                // the report is here. Select the tail, erase it, then commit.
                let n = raw.chars().count() as u32;
                if n > 0 {
                    actions.push(Action::SelectLeft(n));
                    actions.push(Action::PressDelete);
                }
            }
            _ => {
                let n = raw.chars().count() as u32;
                if n > 0 {
                    actions.push(Action::InjectBackspaces(n));
                }
            }
        }
        let encoded = encode(self.charset, text);
        match self.unicode_channel {
            UnicodeChannel::Commit => actions.push(Action::Commit(match terminator {
                Some(c) => format!("{encoded}{c}"),
                None => encoded,
            })),
            UnicodeChannel::Clipboard => actions.push(Action::Paste(match terminator {
                Some(c) => format!("{encoded}{c}"),
                None => encoded,
            })),
        }
        if self.post_delay_ms > 0 {
            actions.push(Action::PostDelay(self.post_delay_ms));
        }
        self.tail = text.to_string();
        // The application's last character is the terminator, not the
        // word's — the per-key features read it from there.
        self.note_char(terminator.unwrap_or_else(|| text.chars().last().unwrap_or(' ')));
        actions
    }

    /// Word end: macros and auto-restore replace the committed word, preedit
    /// commits the word it still holds, and everything moves on. A shortcode
    /// candidate word never word-macro-expands — the ':name:' key is the
    /// emoji path's own lookup — and a missed close puts the raw keys back
    /// instead of the composed word.
    pub(super) fn on_finish(
        &mut self,
        mode: Mode,
        word: String,
        raw: String,
        valid: bool,
        valid_prefix: bool,
        candidate: CandidateEnd,
    ) -> Vec<Action> {
        let mut actions = self.hide_if_visible();
        let inline = mode != Mode::Preedit;
        let skip_macro = !matches!(candidate, CandidateEnd::None);
        let force_raw = matches!(candidate, CandidateEnd::Close) && word != raw;

        // Macros win over auto-restore. Inline expansion rewrites committed
        // text; without a delete transport the commit would duplicate the
        // raw keys the application already holds.
        if !raw.is_empty() && !skip_macro {
            let expansion = if inline && !self.deletes_allowed() && !forwards_delete(mode) {
                None
            } else {
                self.expand_macro(&raw, valid)
            };
            if let Some(exp) = expansion {
                if inline && !self.tail.is_empty() {
                    actions.push(self.delete_action(
                        mode,
                        self.tail.chars().count() as u32,
                        exp.chars().count(),
                    ));
                }
                actions.push(Action::Commit(exp.clone()));
                // The word ended and its terminator is forwarded next, so
                // nothing of ours sits before the caret any more. Keeping the
                // expansion here made the next macro rewrite delete it a
                // second time — a stale, too-long delete that reached past the
                // text the client held (mutter aborts on that).
                if let Some(c) = exp.chars().last() {
                    self.note_char(c);
                }
                self.tail.clear();
                self.release_word_mode();
                actions.push(Action::Forward);
                return actions;
            }
        }

        // Auto Non-VN restore: the composed word is not a Vietnamese syllable
        // and differs from the raw keys -> put the raw keys back (see
        // `should_auto_restore`; the prefix verdict keeps `đuo` composed).
        // `force_raw` (a shortcode close that matched nothing) restores the
        // raw keys regardless of validity.
        if (force_raw || self.should_auto_restore(&word, &raw, valid_prefix))
            // Inline restore rewrites committed text; without a delete
            // transport the raw keys are already on screen, so there is
            // nothing to restore.
            && (!inline || self.deletes_allowed() || forwards_delete(mode))
        {
            if inline && !self.tail.is_empty() {
                actions.push(self.delete_action(
                    mode,
                    self.tail.chars().count() as u32,
                    raw.chars().count(),
                ));
            }
            actions.push(Action::Commit(raw.clone()));
            // Word ended (see the macro branch above): the forwarded terminator
            // is not ours, so the tracked tail must go.
            if let Some(c) = raw.chars().last() {
                self.note_char(c);
            }
            self.tail.clear();
            self.release_word_mode();
            actions.push(Action::Forward);
            return actions;
        }

        if mode == Mode::Preedit && !word.is_empty() {
            let text = if force_raw { &raw } else { &word };
            actions.push(Action::Commit(encode(self.charset, text)));
        }
        self.tail.clear();
        self.release_word_mode();
        actions.push(Action::Forward);
        actions
    }

    /// One keystroke's edit, per display mode.
    fn on_edit(&mut self, mode: Mode, edit: crate::bamboo_wrapper::Edit) -> Vec<Action> {
        if edit.is_empty() {
            return vec![Action::Consumed];
        }
        if mode == Mode::Preedit {
            let text = self.preedit_text();
            let cursor = text.chars().count() as u32;
            self.preedit_visible = true;
            return vec![Action::UpdatePreedit { text, cursor }];
        }
        match self.charset {
            // Unicode is one char per char, so the delta from bamboo-core can
            // be replayed directly.
            Charset::Unicode => {
                if edit.delete_chars > 0 && !self.deletes_allowed() && !forwards_delete(mode) {
                    // No surrounding data yet: never send a delete (mutter
                    // segfaults on clients whose buffer it never saw). Keep
                    // the composition and pass the key through; the client's
                    // SetSurroundingText (sent right after our
                    // RequireSurroundingText) unblocks rewriting.
                    if debug_log::enabled() {
                        debug_log::log(&format!(
                            "raw-forward (no delete transport) client={:?}",
                            self.client
                        ));
                    }
                    // The application keeps the raw keystrokes; re-align the
                    // tracking so later keys edit the text actually shown.
                    self.wrapper.resync_output_to_raw();
                    self.tail = self.wrapper.raw().to_string();
                    return vec![Action::Forward];
                }
                let mut actions = Vec::new();
                if edit.delete_chars > 0 {
                    let insert_len = edit.insert.chars().count();
                    actions.push(self.delete_action(
                        mode,
                        self.delete_count(mode, edit.delete_chars as u32),
                        insert_len,
                    ));
                }
                let insert = edit.insert;
                if !insert.is_empty() {
                    actions.push(Action::Commit(insert.clone()));
                }
                self.tail = replace_tail(&self.tail, edit.delete_chars, &insert);
                if let Some(c) = self.tail.chars().last() {
                    self.note_char(c);
                }
                actions
            }
            // Legacy encodings change the character count (VNI `â` -> `aâ`),
            // so bamboo-core's Unicode delta is unusable; rewrite the word.
            _ if !self.deletes_allowed() && !forwards_delete(mode) => {
                // Same gate as the Unicode arm: the rewrite delete cannot
                // reach the application, and the paired commit would
                // duplicate the word from the second keystroke on.
                if debug_log::enabled() {
                    debug_log::log(&format!(
                        "raw-forward (no delete transport, legacy) client={:?}",
                        self.client
                    ));
                }
                self.wrapper.resync_output_to_raw();
                self.tail = self.wrapper.raw().to_string();
                vec![Action::Forward]
            }
            _ => {
                let actions = self.rewrite(mode);
                if let Some(c) = self.tail.chars().last() {
                    self.note_char(c);
                }
                actions
            }
        }
    }

    /// Replace the whole composition with its legacy-encoded form. Needed
    /// because those encodings change the character count (VNI `â` -> `aâ`),
    /// so bamboo-core's Unicode delta is unusable.
    pub(super) fn rewrite(&mut self, mode: Mode) -> Vec<Action> {
        let text = encode(self.charset, &self.wrapper.output());
        let mut actions = Vec::new();
        if !self.tail.is_empty() {
            actions.push(self.delete_action(
                mode,
                self.tail.chars().count() as u32,
                text.chars().count(),
            ));
        }
        if !text.is_empty() {
            actions.push(Action::Commit(text.clone()));
        }
        self.tail = text;
        actions
    }
}
