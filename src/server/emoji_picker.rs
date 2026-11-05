//! The emoji picker, an IBus lookup table at the caret — the same
//! ibus-bamboo-style flow as the app-rule picker (`picker.rs`), which is
//! what replaced the short-lived GTK window (its clipboard copy failed
//! without `wl-copy`/`xclip`, and every hotkey press spawned another
//! window). The menu opens at the caret with one flat page of ten: the
//! recently picked emoji, falling back to the table head. While the menu
//! is up the keys ARE the search field — letters build the query (huyền
//! hỏi folded away, so `cuoi` finds "cười"), digits 1..9/0 pick the page
//! entry, Enter picks the first, Page Up/Down turn pages, Backspace edits
//! the query, Escape or any other key closes the menu and lets the key
//! type on; the hotkey pressed again toggles the menu closed. A pick
//! commits the glyph straight into the application — no clipboard —
//! records the name in the recents file and closes the menu.

use super::*;
use crate::emoji::{search_rows, RecentEmoji, UserEmoji};

/// One page holds every candidate the popup shows; the digits 1..9/0 map
/// onto it and the pager (keys or the panel's buttons) turns pages.
pub(super) const EMOJI_PAGE_SIZE: u32 = 10;

/// Keys the emoji picker consumes while open. Pure mapping, so tests can
/// drive it without a server.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum EmojiKey {
    Select(usize),
    Query(char),
    Backspace,
    PageUp,
    PageDown,
    Cancel,
    /// Any other key: close the menu and let the key reach the engine.
    Ignore,
}

pub(super) struct EmojiPickerState {
    /// (name, glyph, folded blob) of the whole table, built once at open —
    /// filtering never touches the filesystem.
    entries: Vec<(String, String, String)>,
    /// The search query (what the user typed) and the page cursor —
    /// pub(super) so the tests can drive the state directly.
    pub(super) query: String,
    pub(super) page: usize,
    /// The picks before, newest first, resolved to (name, glyph) once at
    /// open — the initial page without per-keystroke file reads.
    recents: Vec<(String, String)>,
}

impl EmojiPickerState {
    pub fn new() -> Self {
        let rows = search_rows(&UserEmoji::load().map);
        let recents = RecentEmoji::load()
            .names()
            .iter()
            .filter_map(|name| {
                rows.iter()
                    .find(|r| &r.name == name)
                    .map(|r| (name.clone(), r.glyph.clone()))
            })
            .collect();
        Self::with(rows, recents)
    }

    /// Injection seam for the tests: the developer's real recents file and
    /// user table must not decide what a test sees.
    pub fn with(rows: Vec<crate::emoji::SearchRow>, recents: Vec<(String, String)>) -> Self {
        Self {
            entries: rows
                .into_iter()
                .map(|r| (r.name, r.glyph, r.blob))
                .collect(),
            query: String::new(),
            page: 0,
            recents,
        }
    }

    /// The current result rows: the recents while the query is empty (the
    /// table head before the first pick — the common smileys), else every
    /// entry whose folded blob contains the folded query.
    fn results(&self) -> Vec<(String, String)> {
        if self.query.is_empty() {
            if !self.recents.is_empty() {
                return self.recents.clone();
            }
            // No picks yet: the whole table in its order — the common
            // smileys lead page one (the page window is page_results' job).
            return self
                .entries
                .iter()
                .map(|(n, g, _)| (n.clone(), g.clone()))
                .collect();
        }
        let q = crate::emoji::fold_ascii(&self.query);
        self.entries
            .iter()
            .filter(|(_, _, blob)| blob.contains(&q))
            .map(|(n, g, _)| (n.clone(), g.clone()))
            .collect()
    }

    /// The page's candidate labels: "glyph name" — the name is the only
    /// hint the popup has for what a glyph is.
    pub(super) fn page_labels(&self) -> Vec<String> {
        self.page_results()
            .into_iter()
            .map(|(name, glyph)| format!("{glyph} {name}"))
            .collect()
    }

    fn page_results(&self) -> Vec<(String, String)> {
        let start = self.page * EMOJI_PAGE_SIZE as usize;
        self.results()
            .into_iter()
            .skip(start)
            .take(EMOJI_PAGE_SIZE as usize)
            .collect()
    }

    /// The auxiliary line under the popup: the query state and the count.
    pub(super) fn auxiliary(&self) -> String {
        if self.query.is_empty() {
            "Emoji — 1-9/0 chọn, gõ chữ để tìm, Esc đóng".to_string()
        } else {
            format!(
                "Tìm \"{}\" — {} kết quả, 1-9/0 chọn, Esc đóng",
                self.query,
                self.results().len()
            )
        }
    }

    /// The (name, glyph) a pick lands on — page-relative, only the visible
    /// entries are pickable (a digit never reaches past the popup).
    pub fn pick(&self, index: usize) -> Option<(String, String)> {
        if index >= EMOJI_PAGE_SIZE as usize {
            return None;
        }
        self.page_results().into_iter().nth(index)
    }

    pub fn page_down(&mut self) {
        let total = self.results().len().div_ceil(EMOJI_PAGE_SIZE as usize);
        if self.page + 1 < total {
            self.page += 1;
        }
    }

    pub fn page_up(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    /// Map one key event onto the picker. Digits 1..9 pick entries 1..9 and
    /// 0 picks the tenth (keypad digits included); Enter picks the first;
    /// letters, space and underscore build the query.
    pub fn on_key(&self, keyval: u32) -> EmojiKey {
        match keyval {
            65293 | 65421 => EmojiKey::Select(0), // Return / KP_Enter
            65307 => EmojiKey::Cancel,            // Escape
            65288 => EmojiKey::Backspace,
            65365 => EmojiKey::PageUp,
            65366 => EmojiKey::PageDown,
            k @ 0x31..=0x39 => EmojiKey::Select((k - 0x31) as usize),
            0x30 => EmojiKey::Select(9),
            k @ 0xffb1..=0xffb9 => EmojiKey::Select((k - 0xffb1) as usize), // KP_1..KP_9
            0xffb0 => EmojiKey::Select(9),                                  // KP_0
            _ => match keymap::keysym_to_char(keyval) {
                Some(c) if c.is_ascii_alphabetic() || c == ' ' || c == '_' => EmojiKey::Query(c),
                _ => EmojiKey::Ignore,
            },
        }
    }
}

impl EngineHandler {
    /// Open the menu at the caret (or toggle it closed — the hotkey is a
    /// toggle, like the app-rule picker's). Opening ends the in-flight
    /// word first, the same contract as the rule picker: a pick must never
    /// land over a live composition.
    pub(super) fn open_emoji_picker(&mut self, out: &mut Vec<Message>) {
        if self.emoji_picker.take().is_some() {
            out.push(signal("HideLookupTable"));
            out.push(signal("HideAuxiliaryText"));
            return;
        }
        // The two menus are mutually exclusive; a rule picker left open
        // would keep eating digits.
        if self.picker.take().is_some() {
            out.push(signal("HideLookupTable"));
            out.push(signal("HideAuxiliaryText"));
        }
        {
            let mut engine = lock_engine(&self.engine);
            let actions = engine.end_word_for_picker();
            emit(&mut engine, &actions, out);
        }
        self.emoji_picker = Some(EmojiPickerState::new());
        self.emit_emoji_table(out);
    }

    /// Push the current page plus the auxiliary line.
    pub(super) fn emit_emoji_table(&self, out: &mut Vec<Message>) {
        let Some(state) = self.emoji_picker.as_ref() else {
            return;
        };
        let labels = state.page_labels();
        if labels.is_empty() {
            // No results: keep the query visible, drop the empty popup.
            out.push(signal("HideLookupTable"));
        } else {
            let table = ibus_prop::lookup_table_variant(EMOJI_PAGE_SIZE, 0, &labels);
            out.push(signal("UpdateLookupTable").append2(table, true));
        }
        out.push(
            signal("UpdateAuxiliaryText")
                .append2(crate::ibus_text::plain_text(&state.auxiliary()), true),
        );
    }

    /// Handle a key while the menu is open. Returns whether the key was
    /// consumed; `Ignore` closes the menu first so the key types on.
    pub(super) fn emoji_picker_key(
        &mut self,
        keyval: u32,
        state: u32,
        out: &mut Vec<Message>,
    ) -> bool {
        // Releases never act: the combo's own releases arrive the moment
        // the menu opens (which used to close it instantly). Swallow them.
        if state & keymap::RELEASE_MASK != 0 {
            return true;
        }
        let Some(picker) = self.emoji_picker.as_ref() else {
            return false;
        };
        match picker.on_key(keyval) {
            EmojiKey::Select(i) => {
                self.apply_emoji_pick(i, out);
                true
            }
            EmojiKey::Query(c) => {
                if let Some(picker) = self.emoji_picker.as_mut() {
                    picker.query.push(c.to_ascii_lowercase());
                    picker.page = 0;
                }
                self.emit_emoji_table(out);
                true
            }
            EmojiKey::Backspace => {
                if let Some(picker) = self.emoji_picker.as_mut() {
                    picker.query.pop();
                    picker.page = 0;
                }
                self.emit_emoji_table(out);
                true
            }
            EmojiKey::PageUp => {
                if let Some(picker) = self.emoji_picker.as_mut() {
                    picker.page_up();
                }
                self.emit_emoji_table(out);
                true
            }
            EmojiKey::PageDown => {
                if let Some(picker) = self.emoji_picker.as_mut() {
                    picker.page_down();
                }
                self.emit_emoji_table(out);
                true
            }
            EmojiKey::Cancel => {
                self.close_emoji_picker(out);
                true
            }
            EmojiKey::Ignore => {
                self.close_emoji_picker(out);
                false
            }
        }
    }

    /// Commit the picked glyph straight into the application (the menu is
    /// at the caret — no clipboard involved), record the name in the
    /// recents file and close the menu.
    pub(super) fn apply_emoji_pick(&mut self, index: usize, out: &mut Vec<Message>) {
        let Some((name, glyph)) = self.emoji_picker.as_ref().and_then(|p| p.pick(index)) else {
            return; // past the end of the results: the menu stays open
        };
        self.emoji_picker = None;
        RecentEmoji::push(&name);
        debug_log::log("emoji picker: pick committed");
        {
            let mut engine = lock_engine(&self.engine);
            // Nothing else may be in flight (opening ended the word and the
            // menu swallowed every key since), so the commit is the whole
            // story — and the engine's last-char bookkeeping must follow it.
            engine.note_committed(&glyph);
        }
        out.push(signal("CommitText").append1(crate::ibus_text::plain_text(&glyph)));
        out.push(signal("HideLookupTable"));
        out.push(signal("HideAuxiliaryText"));
    }

    /// Close the menu without committing anything.
    pub(super) fn close_emoji_picker(&mut self, out: &mut Vec<Message>) {
        if self.emoji_picker.take().is_some() {
            out.push(signal("HideLookupTable"));
            out.push(signal("HideAuxiliaryText"));
        }
    }
}
