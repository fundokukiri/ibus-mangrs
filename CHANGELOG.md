# Changelog

All notable, user-visible changes to ibus-mangrs (MăngRS) are documented here, newest first.
The project versions independently of `bamboo-core`; each release gets a tag `ibus-mangrs-<version>`.
Detailed round-by-round debugging history lives in `DEVLOG.md` (internal).

## [0.1.4] — 02/11/2026

In Preedit mode (the default), typing a Vietnamese word and then clicking somewhere else or
switching to another window no longer loses the word: the app now commits it, instead of clearing
it, because the preedit tells the app to commit it on a reset — the same thing ibus-bamboo does.
The extra "click behavior" and "auto-release time" settings are gone, since they existed only to
work around that loss. Core: `bamboo-core 0.5.3`.

### Changed

- Removed the "Thời gian tự nhả" and "Hành vi khi click" settings (the `preedit_commit_timeout_ms`
  and `click_action` keys). They were workarounds for the word loss fixed below and are no longer
  needed; the keys are ignored if they remain in `config.toml`.
- While typing in Preedit mode, a macro key now shows its expansion before the word ends: typing
  `gs` shows `giờ sáng` underlined. The app commits what the preedit shows, so without this a
  click would have committed the raw `gs`.
- A half-typed word whose syllable is still incomplete (`đuo`, `đuoc`) now stays composed instead
  of briefly reverting to the raw keys at each keystroke. This is what ibus-bamboo does; the
  revert is kept for words that are not Vietnamese at all (`windowf`).

### Fixed

- Preedit mode: typing a Vietnamese word without space or Enter, then clicking elsewhere (or
  switching window) before it committed, made the typed word disappear. The preedit is now sent
  with `IBUS_ENGINE_PREEDIT_COMMIT`, so the client commits the word itself when it resets the
  input context, exactly like ibus-bamboo. Verified over D-Bus (the preedit carries the commit
  mode, and a Reset/FocusOut no longer makes the engine commit, so nothing is duplicated); a real
  mouse click in a real application has not been tested yet.
- Typing `được` (Telex `dduocwj`) flickered through `dduo` and `dduoc` before the finished word
  appeared. A word that is still a valid Vietnamese prefix no longer reverts to its raw keys
  mid-word, so `đuo` stays `đuo`; non-Vietnamese words like `windowf` are still restored.
  Verified over D-Bus, not yet in a real application.

## [0.1.3] — 01/11/2026

Fixes across every typing mode: a macro word no longer breaks when typing continues after a
pause, auto-capitalize works in the three uinput modes, one bad line in `config.toml` no longer
resets all settings, and the clipboard channel gives your clipboard back even when the paste
fails. Core: `bamboo-core 0.5.3`. Download:
https://github.com/fundokukiri/ibus-mangrs/releases/tag/v0.1.3

### Changed

- The spell-check word list is gone from the settings window ("Giới thiệu" page) and from the
  installers. It never changed what you type — spelling correction is the engine's syllable
  check — so typing is unaffected. If you created `~/.config/ibus-mangrs/vn-words.txt` to extend
  that list, the file now has no effect.

### Fixed

- Typing a macro key, pausing, then typing another key corrupted the word: with
  `gs = "giờ sáng"`, typing `gs`, waiting for the commit, then `s` left `giờ sá` on screen and
  ate the following character. The late key now starts a new word instead of rewriting the
  expansion from the raw keys.
- "Tự viết hoa" did nothing in the Smooth, Super Smooth and Select modes: `xin chao. tieengs`
  produced `tiếng` instead of `Tiếng`.
- One typo in `config.toml` (for example `w2u = "everywhre"`) silently reset every setting to
  its default, and the settings window then wrote those defaults over the file. Only the
  offending line is ignored now, and it is reported in `~/.cache/ibus-mangrs/debug.log`.
- The "Không gạch chân" mode could delete text around the caret when the application reported
  surrounding text that did not match, although only "Surrounding text" is meant to repair. It
  no longer does.
- With `unicode_channel = "clipboard"`, a failed Ctrl+V left your clipboard holding the
  Vietnamese word instead of what you had copied. The original content is restored now.
- With the same setting, the Vietnamese word could appear and then be deleted again when the
  paste fell back to a plain commit ("the accent flashes and reverts"). The commit now waits for
  the injected Backspace keys.
- Losing focus, or changing a setting, in the middle of a macro word committed the raw keys
  (`gs` instead of `giờ sáng`). Verified over D-Bus with a synthetic client, not yet in a real
  application.
- In the "Select" mode, a failed Delete key press left the typed tail selected and the next word
  replaced it. The tail is now deleted with a Backspace in that case.
- On a machine with more than one registry file in `~/.config/ibus/bus/`, the engine could
  connect to a daemon that is no longer running and stop receiving keys. It now uses the newest
  file.
- Importing a settings backup (JSON) in the settings window left the switches at their old
  values, so the next save wrote those back over the imported settings. The widget mapping now
  has a test; the import itself has not been clicked through by hand yet.

## [0.1.1] — 31/10/2026

Three uinput display modes (Smooth, Super Smooth, Select) that press real keys instead of
rewriting over the IBus protocol, plus OFF and Default Typing as selectable modes; a bundled
38 350-word Vietnamese spell-check dictionary; raw-key bypass for URLs/emails/paths and safer
macro expansion; the settings window redesigned with an About page and a balanced tab bar;
`/dev/uinput` permissions set up automatically by the installer. Core: bamboo-core 0.5.3.
Download:
[v0.1.1](https://github.com/fundokukiri/ibus-mangrs/releases/tag/v0.1.1).

### Added

- Bundled Vietnamese word list (38 350 words) for spell checking, installed to
  `/usr/share/ibus-mangrs/vn-words.txt`; a file at `~/.config/ibus-mangrs/vn-words.txt` overrides
  it (data source and license: `data/README.md`).
- The install script and the Arch package now set up `/dev/uinput` permissions automatically
  (udev rule + `input` group), so backspace-based tone repair works out of the box.
- Three uinput display modes that press real keys instead of rewriting over the IBus protocol —
  **Smooth**, **Super Smooth** and **Select** — for terminals, games and apps that mishandle
  protocol deletes. Raw keys are forwarded while typing; the latin tail is erased with real
  Backspaces (or Shift+Left selection) at word end and the Vietnamese word is delivered over
  CommitText or, optionally, the clipboard (with automatic restore). Pacing is configurable
  (defaults 20/5/10 ms). Modes fall back to preedit wherever real-key injection is impossible.
- **OFF** and **Default Typing** as selectable display modes: OFF keeps macros and
  auto-capitalize while typing raw; Default Typing forwards every key untouched. Both reachable
  from the menu, the GUI, the cycle hotkey and App Rules.
- Words typed directly after a URL/email/path character (`/ : @ . _ ~ = # & % ?`) stay raw —
  `example.tieeng` no longer becomes `example.tiêng` while you type web addresses, emails and
  file paths (toggle in the Typing tab, on by default).
- Shorthand keys that form a valid Vietnamese word (e.g. a macro keyed `cam`) no longer expand
  while you are typing that word normally; macro keys must not be valid Vietnamese words.
- The settings window has an **About page** (logo, author, MIT license, credits) and the
  quick-settings popup is no longer stretched wide by long labels; hint text is small and dimmed.
  The menu entry that opens the window is now called *Cài đặt…*, and the *MăngRS* entry opens the
  GitHub page.

## [0.1.0] — 31/10/2026

First public release. MăngRS types Vietnamese on IBus with Telex, VNI, VIQR and Microsoft
layouts, shows the composition inline with an underline (preedit, the default) or without one,
and repairs itself on apps that swallow text deletions. Comes with a GTK4 settings window, a
per-app rule system, macros, and a custom dictionary. Core: bamboo-core 0.5.3. Download:
[ibus-mangrs-0.1.0](https://github.com/fundokukiri/ibus-mangrs/releases/tag/v0.1.0).

### Added

- - Vietnamese typing for IBus with four layouts: Telex, VNI, VIQR, and Microsoft layout; switching
  from the input-source menu or the settings window.
- Three display modes: preedit (underline, the default), non-preedit (no underline, rewrites the
  word tail), and surrounding text (verifies what the app reports and repairs drift). Switch from
  the menu, the GUI, or an assignable cycle hotkey such as `Control+Alt+v`.
- Output charsets: Unicode plus 10 legacy encodings (NFD, TCVN3, VNI, VIQR, VISCII, VPS, CP1258,
  BKHCM, Vietware X, Vietware Full).
- Typing options: modern tone placement (`hòa` instead of `hoà`), free tone marking (`hoangf` →
  `hoàng`), spell check, auto-restore of non-Vietnamese words, w→ư, `[`/`]` → `ơ`/`ư`, `--` →
  `—`, auto-capitalize after `. ! ?` and Enter, double space → period.
- Macros in `~/.config/ibus-mangrs/macros.toml` with `$TIME`/`$DATE` and capitalization, plus a
  custom dictionary in `~/.config/ibus-mangrs/dictionary.txt` whose words are never auto-restored.
- Per-application rules (GUI Applications tab): each app can get its own display mode; known
  delete-swallowing apps (Chrome, Electron, VS Code, Slack…) default to preedit, Sublime Text to
  surrounding text.
- GTK4 settings window (6 tabs, Vietnamese labels); config changes apply within ~1 second
  without restarting IBus.
- Backup/restore of config, macros and dictionary as one JSON file.
- Debug logging (GUI General tab or `debug_log = true`) writing engine decisions to
  `~/.cache/ibus-mangrs/debug.log` — attach this file when reporting bugs.

### Changed

- - The engine is known as **MăngRS** (`ibus-mangrs`); old `unikey`/`r-unikey` configurations in
  `~/.config/` are migrated automatically on first run.
- Preedit is the install default display mode (previously non-preedit).

### Fixed

- - GNOME Shell no longer crashes to the login screen when typing tone marks. The engine used to
  send text deletions to apps whose buffer it had never seen; it now deletes over the protocol
  only when the app reports its surrounding text, otherwise presses real Backspace keys through
  `/dev/uinput`.
- Characters no longer disappear or duplicate after Backspace (`tie` → Backspace → `n` produced
  `tn`): the typing engine tracks its own output instead of trusting bamboo-core's stale internal delta.
- Undoing a tone mark no longer poisons the next keystrokes (`too` + `l` produced `toool`): the
  word now commits exactly what is on screen.
- Typing digits next to a word (Telex/VIQR) no longer erases the word (`x1` kept `x1` instead of
  deleting it): digits end the word instead of reaching the typing engine.
- Chrome and Electron apps no longer duplicate characters (`tungùng`): they auto-switch to
  preedit, and text rewritten by Chrome's search suggestions no longer eats marks (`tùng` → `tungf`).
- The Chrome search bar no longer swallows typed text when its report doesn't concern the word
  being typed.
- Sublime Text and JetBrains XToolkit can type Vietnamese again: apps that never identify
  themselves now get real Backspace injection instead of raw mark keys.
- A typed accent no longer flashes and vanishes ("mark then revert"): commits after Backspace
  injection wait for the injected keys to arrive, in order.
- Moving the caret inside an open preedit no longer scrambles the word into the wrong position.
- Picking an item in the Charset / Input Method / Display menus always applies the picked item —
  previously the last item in the group won regardless of choice ("always VIQR", "always preedit").
- The engine no longer breaks typing after switching windows quickly (stale capability state was
  carried across focus changes).
- Backspace inside a word now follows the unikey convention: it undoes the last typed key and the
  word recomposes from the remaining keys — previously backspacing through a composed word could
  delete nothing (`toool`+Backspace kept "tool") or corrupt the marks when retyping (`mừi`
  instead of `mười`); results are now deterministic.
- The last word typed in preedit mode no longer vanishes when the config changes mid-word (GUI
  menu click, mode-cycle hotkey, or an external config rewrite): the word on screen is committed
  before the engine rebuilds instead of being silently dropped.
