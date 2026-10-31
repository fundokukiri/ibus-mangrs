# Usage

Everything is reachable from the input-source menu (the `vi` icon in the top bar), the settings
window, or the config file — all three stay in sync and changes apply within ~1 second.

## Display modes

Eight modes decide how text reaches the application. Switch from the menu (*Display*), the
settings window, an assignable cycle hotkey, or per application with App Rules.

| Mode | How it works | Best for |
|---|---|---|
| Preedit (default) | word shows inline with underline, commits at word end | widest compatibility |
| Non-preedit | every key commits immediately, the word tail is rewritten on marks | Firefox, GTK apps |
| Surrounding text | non-preedit + verifies against the app's own text report | Sublime Text |
| Smooth (uinput) | raw keys forwarded while typing, latin tail erased with real Backspaces at word end | terminals, quirky apps (X11) |
| Super Smooth (uinput) | same, without the safety check before rewriting | fast typing (X11) |
| Select (uinput) | Shift+← selects the tail, Delete erases it, the word commits | apps where Backspaces misbehave (X11) |
| OFF | Vietnamese off, macros still work | mixed Vietnamese/English sessions |
| Default Typing | engine fully inert, every key forwarded | apps that resent any processing |

The uinput modes need a writable `/dev/uinput` (see [installation.md](installation.md)) and fall
back to preedit wherever real-key injection cannot reach the application — on GNOME Wayland they
resolve to preedit; they are intended for X11 sessions.

The cycle hotkey (GUI *General* tab, e.g. `Control+Alt+v`) walks through all eight in order.

## Input methods and charsets

- *Input Method*: Telex (`tieengs` → `tiếng`), VNI (`tie6ng1`), VIQR, Microsoft layout.
- *Charset*: Unicode (default) plus 10 legacy encodings — NFD, TCVN3, VNI, VIQR, VISCII, VPS,
  CP1258, BKHCM, Vietware X, Vietware Full.

## Typing behavior

- **Modern tone placement** (`hòa`) vs traditional (`hoà`), and **free tone marking**
  (`hoangf` → `hoàng`, the mark may be typed anywhere in the word).
- **Spell check** backs the auto-correction of invalid syllables with a bundled 38 350-word
  Vietnamese word list; a file at `~/.config/ibus-mangrs/vn-words.txt` overrides it.
- **Auto non-Vietnamese restore**: words that are not Vietnamese syllables revert to the raw
  keystrokes — English text and identifiers type through untouched.
- **URL/email/path bypass**: a word typed directly after `/ : @ . _ ~ = # & % ?` stays raw, so
  web addresses, emails and file paths are never marked.
- **Backspace** follows the unikey convention: it undoes the last typed key and the word
  recomposes from what remains. Repeating a mark key (`teeen`, `toool`) undoes that mark.
- **Vietnamese abbreviations** (toggle, on by default): `ddc` → `đc`, `ddk` → `đk`,
  `ddt` → `đt`, `ddg` → `đg` — the đ survives on shorthand that is not a syllable.
- **Idle auto-commit** (preedit mode): a word left pending for 800 ms commits in place, so a
  later mouse click cannot lose it. A tone key pressed after the wait re-enters the word and
  still applies the mark (`tung` + wait + `f` → `tùng`). Configurable, 0 = off.
- **Click actions for a pending word** (Chế độ gõ tab, "Hành vi khi click"): **Bamboo** —
  the default above, a click that still catches a pending word drops it; **Keep word** — the
  click hides the word but keeps it, clicking back to the same spot restores it with
  mark-undo intact (the idle commit is off in this mode); **Relocate** — the word is always
  delivered to the position where it was typed, and the caret ends up right after it.
- Extras (all optional): `w` → `ư`, `[`/`]` → `ơ`/`ư`, `--` → `—`, auto-capitalize after
  `. ! ?` and Enter, double space → period.

## Macros

`~/.config/ibus-mangrs/macros.toml`, one `key=expansion` per line:

```toml
gs=giờ sáng
dc=địa chỉ: $TIME
```

`$TIME` and `$DATE` expand per *Time Format* (`%H:%M`) and *Date Format* (`%d/%m/%Y`).
Capitalization: a partially capitalized key capitalizes the expansion, an ALL-CAPS key expands
to ALL CAPS. Shorthand keys that already form a valid Vietnamese word (like `cam`) never expand
while you are typing that word normally.

## Custom dictionary

`~/.config/ibus-mangrs/dictionary.txt`, one word per line. Words in this file are never
auto-restored to raw keystrokes. Enable it with *Dùng từ điển của tôi* in the menu or the GUI.

## App Rules

Per-application display modes (GUI *Applications* tab), one rule per line:

```
sublime_text=surrounding
org.gnome.TextEditor=preedit
```

Case-insensitive substring match on the process name. The GUI offers a dropdown of apps the
engine has already seen. Built-in defaults: Chromium-family and Electron apps → preedit, Sublime
Text → surrounding text.

## Configuration files

```
~/.config/ibus-mangrs/config.toml     # all settings, hot-reloaded within ~1 s
~/.config/ibus-mangrs/macros.toml     # shorthand table
~/.config/ibus-mangrs/dictionary.txt  # never-restored words
~/.config/ibus-mangrs/vn-words.txt    # optional override of the bundled word list
```

Backup/restore (one JSON containing config, macros and dictionary) lives in the GUI *Backup* tab.
