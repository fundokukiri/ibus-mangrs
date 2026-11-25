# Features and usage — MăngRS

Installation and troubleshooting: [README-en.md](README-en.md). Tiếng Việt: [FEATURES-vi.md](FEATURES-vi.md).

## Display modes

Five modes, one contract each (config value in parentheses; the factory default first):

| Mode (config value) | How it works |
| ------------------- | ------------ |
| **Underline** (`preedit`, default) | the word shows inline with underline, commits at word end; a click or focus change never loses the half-typed word |
| **Surrounding text** (`surrounding`) | every key commits at once; the marked tail is deleted and rewritten via `DeleteSurroundingText`, verified against the app's own text report and repaired on drift |
| **Flow** (`flow`) | raw keys are forwarded as you type; every composition-changing key applies immediately as an inline mini-burst (per-key marks); the finished word commits at word end |
| **Forward Backspace** (`forwardbackspace`) | every key commits at once; the rewrite deletes by forwarding Backspace key events through IBus; never falls back to preedit |
| **OFF** (`off`) | Vietnamese processing off; macros and auto-capitalize still work |

Switch modes from the input-source menu, the settings window's _Display mode_ row, a cycle
hotkey, or per app with **App Rules**.

All five modes are always offered: input-source menu, mode row, App Rules and the cycle
hotkey. An old config file that still carries the `visible_modes` key loads normally and the
key is ignored.

- **Flow** needs one working injection backend (the kernel's `/dev/uinput` device by default,
  or the compositor's virtual-keyboard protocol on Hyprland/wlroots); without one,
  **Forward Backspace** serves the words and the engine re-requests a backend after a cooldown.
- **Forward Backspace** needs nothing beyond IBus: its deletes travel as forwarded key events,
  so it works on the GNOME Wayland relay (where injection is banned) and in XIM clients.
- A config or rule still carrying a removed value migrates automatically on load:
  `nonpreedit` → `surrounding`, `turbo`/`select`/`smoothforward` → `flow`, `forwardselect`
  and the old `forwardkeyevent` spelling → `forwardbackspace`, `defaulttyping` → `off`,
  `auto` → `preedit`. Old files keep loading; nothing is rewritten behind the user's back.

## Input methods and charsets

**9 input methods** — Telex (`tieengs` → `tiếng`), Telex 2, Telex W (`asz` → `a`),
Telex + VNI, Telex + VNI + VIQR, VNI (`tie6ng1`), VNI (French keyboard), VIQR and the
Microsoft layout (`ti3ng8` → `tiếng`).

**11 output charsets** — Unicode (default), VNI, VIQR, TCVN3, VISCII, VPS, CP1258, BKHCM,
Vietware X, Vietware Full, NFD. Only text sent to the app is encoded; legacy encodings change
the character count, so the whole word is rewritten with a correct delete count.

## Typing options

- Modern tone placement (`hòa`) or traditional (`hoà`).
- Free tone marking (`hoangf` → `hoàng`) — a mark placed on the "wrong" key still applies.
- Auto-correction of invalid syllables (they revert to raw keys; a still-open syllable like
  `đuo` keeps composing).
- Auto-restore of foreign words (`windowf` stays raw) — your custom dictionary is never restored.
- Raw-key bypass for URLs/emails/paths (`example.tieeng` stays `example.tieeng`); only a real
  URL (`https://…`, `www.`, `word://`) arms the bypass, query and fragment included. Extra
  keep-raw characters on the _Experimental_ page.
- `w` → `ư`, `[` `]` → `ơ` `ư`, `--` → `—`, auto-capitalize after `. ! ?` and Enter,
  double space → period.

## Backspace and tone removal

The _Xóa dấu bằng Backspace_ dropdown (_Dấu & tự động_) decides how Backspace treats a
tone-marked word:

| Mode | Behavior | Example |
| --- | --- | --- |
| **Tone first** (default) | one Backspace drops the whole tone | `hạ` → `ha` → `h` |
| **Whole screen character** | deletes the screen's last character | `việt` → `việ` → `vi` |
| **Unikey walk** | undoes the last raw key | `hạ` `hã` `hả` `hà` `há` `ha` |

In **Flow**, erasing into a finished word is a plain delete — **one Backspace deletes exactly
one character**, nothing injected. Deleting the space after `chào ` brings the word back
unchanged, **tone included**, for further editing; the next Backspace drops the tone per the
dropdown. Erasing a mistyped word in front of a finished one no longer eats the finished
word's characters (`chịu t` + two Backspaces keeps `chịu`).

## Per-key marks in Flow

Always on (the switch is gone): `ee` shows `ê` the moment it lands instead of holding raw keys
until the word ends; the pacing field sits on the Tuning tab (default 20 ms). A click or a
focus change never loses the half-typed word — the preedit commits what it shows, including a
shorthand fix like `ddc` → `đc`.

## Macros and dictionary

- **Macros** (`macros.toml`): one `key=expansion` per line, with `$TIME` / `$DATE` expansion
  and smart capitalization. A shorthand keyed like a real Vietnamese word only expands with
  _Gõ tắt nở cả trên từ hợp lệ_ (Experimental page).
- **Custom dictionary** (`dictionary.txt`): one word per line, never auto-restored.

## Emoji

- Typing `:name:` rewrites to the emoji (enable on the _Emoji_ page, off by default) — 1,940
  built-in emoji searchable in Vietnamese and English; only the closing `:` decides, so words
  typed after `:` keep their tones.
- The emoji picker at the caret on an assignable hotkey: recent picks first, type to search,
  digits pick, Enter commits the glyph.
- Your own table: `name = "glyph"` pairs in `~/.config/ibus-mangrs/emoji.toml`.

## App Rules (per-app display mode)

A rule forces one display mode for one application and overrides the global mode; the first
match wins, so put specific rules above general ones. Matching is a case-insensitive substring
of the app's name (`term` also matches `xterm`); the special pattern `none` targets apps that
never report a name. Set rules on the GUI _Per-app rules_ page, through the app-rule picker
hotkey, or by hand in `~/.config/ibus-mangrs/config.toml` (hot-reloaded within ~1 s):

```toml
[[app_rules]]
pattern = "chrome"
mode = "preedit"

[[app_rules]]
pattern = "sublime_text"
mode = "surrounding"
```

Mode values: `preedit` (default), `surrounding`, `flow`, `forwardbackspace`, `off` — old values
keep loading and migrate. A rule naming a hidden mode is kept in the file but ignored until
that mode is shown again.

No rules ship with the engine. It does carry a built-in safeguard for known delete-dropping
clients (Chrome, Chromium, Edge, Brave, Vivaldi and Electron apps such as VS Code, Slack,
Discord): if a rewriting mode is active there, the engine types them in preedit instead of
duplicating text. Give such an app a different mode by adding your own rule.

Chrome and Firefox on GNOME Wayland are reported to IBus only as an anonymous relay: matching
them by name needs the companion GNOME Shell extension
(`data/extension/README.md`), then a logout/login.

## Shortcuts

All four built-in hotkey slots (toggle Vietnamese, cycle display mode, app-rule picker, emoji
picker) ship unassigned — set them on the _Shortcuts_ page. Beyond them, **14 functions** are
assignable: input method switching, charset cycling, opening the settings window on a chosen
page, and the text switches (`--`, double space, auto-capitalize, ...). `Super+Space` is
GNOME's input-source switcher, not a MăngRS shortcut.

## Panel menu and settings window

- **The panel menu is editable** (_Menu panel_ page): folders (one level), functions, the three
  radio groups (charset / method / display), separators, reordering, renaming and a reset to
  the shipped menu; the design is saved in `config.toml` and reaches the panel within a second.
- **The GTK4 + libadwaita settings window** (12 pages, Vietnamese or English — a row on _Other
  settings_ switches the language): every change hot-reloads within about a second without an
  IBus restart, and all settings back up into one JSON file.
