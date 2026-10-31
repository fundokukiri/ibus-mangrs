# Gap analysis

Every user-facing typing option mapped to its implementation, generated from the
actual code (not from memory). Status meanings: **complete** works as advertised,
**partial** works with a documented limitation, **advisory-only** loaded but not
acted on, **dead config** stored and editable but never read.

All paths are relative to `src/`. Config file: `~/.config/ibus-mangrs/config.toml`.

## Display and mode

| Option (config key) | Default | Implemented in | Status |
|---|---|---|---|
| Vietnamese on/off (`enabled`) | `true` | `engine.rs` `set_enabled`; panel toggle and GUI switch in `server.rs`, `gui.rs` | complete |
| Display mode (`mode`), 8 values | `preedit` | `config.rs` parsing; resolution in `engine.rs` `effective_mode` (app rule, context fallbacks); preedit/nonpreedit in `engine.rs`; surrounding verify in `engine.rs` `verify_surrounding`; uinput modes in `engine.rs` + `uinput.rs` | complete |
| OFF (`off`) | — | `engine.rs` `process_char_off`: forwards keys, runs macros when `macro_in_off_mode`; auto-capitalize does NOT run in OFF (its doc comment says it does — drift) | complete |
| Default typing (`defaulttyping`) | — | `engine.rs`: every key forwarded, engine inert | complete |
| Hide underline (`hide_underline`) | `false` | `engine.rs` flag; consumed when building preedit in `server.rs` | complete |
| Cycle-mode hotkey (`cycle_mode_hotkey`) | unassigned | `server.rs` `try_cycle_hotkey` + `parse_hotkey`; cycles all 8 modes | complete |

## Input method and tones

| Option (config key) | Default | Implemented in | Status |
|---|---|---|---|
| Input method (`method`): Telex, VNI, VIQR, Microsoft | `telex` | `config.rs` → `bamboo_wrapper.rs` `set_method` (bamboo-core presets) | complete |
| Modern tone style (`modern_tone_style`) | `true` | `config.rs` `bamboo_config` → bamboo `ToneStyle` | complete |
| Free tone marking (`free_tone_marking`) | `true` | same path → bamboo `free_tone_marking` | complete |
| Auto-correct invalid syllables (`auto_correct`) | `true` | same path → bamboo `auto_correct` | complete |

## Output charset

| Option (config key) | Default | Implemented in | Status |
|---|---|---|---|
| Charset (`charset`): Unicode, NFD, TCVN3, VNI, VIQR, VISCII, VPS, CP1258, BKHCM, Vietware X, Vietware Full | `unicode` | `config.rs` mapping → `bamboo_wrapper.rs` `encode` (bamboo `encode_charset`), applied at word commit/rewrite in `engine.rs` | complete |

## Typing behavior

| Option (config key) | Default | Implemented in | Status |
|---|---|---|---|
| Auto-restore non-Vietnamese (`auto_restore`) | `true` | `engine.rs` `on_finish` / `on_finish_smooth`: invalid word that differs from raw keys is committed as raw; dictionary and undo state gate it | complete |
| URL/email/path bypass (`bypass_url`) | `true` | `engine.rs` `url_word` flag from the character before the word; suppresses marks, macros, auto-capitalize inside the word | complete |
| `w` → `ư` (`w2u`): disabled / nonstart / everywhere | `disabled` | `config.rs` → bamboo `W2uMode` | complete |
| Bracket shortcuts `[ ]` → `ơ ư` (`bracket`) | `disabled` | `config.rs` → bamboo `BracketMode`; bracket keys handled in `bamboo_wrapper.rs` | complete |
| Em dash `--` → `—` (`em_dash`) | `false` | `engine.rs` (delete-before + commit); disabled by design in the uinput modes | complete |
| Auto-capitalize after `. ! ?` and Enter (`auto_capitalize`) | `false` | `engine.rs` window tracking + uppercase-on-next-char; not in OFF or uinput modes | complete |
| Double space → period (`double_space_period`) | `false` | `engine.rs` (delete-before + commit); disabled in uinput modes | complete |
| Commit interval (`commit_interval_ms`) | `20` | stored in `config.rs`, loaded into `engine.rs` with a public getter — **no consumer in `server.rs`, `uinput.rs` or `paste.rs`** | dead config |
| Backspace interval (`backspace_interval_ms`) | `5` | `engine.rs` getter → `server.rs` → `uinput.rs` pacing between injected presses | complete |
| Post-rewrite delay (`post_delay_ms`) | `10` | `engine.rs` pushes `Action::PostDelay`; `server.rs` sleeps | complete |
| Unicode channel (`unicode_channel`): commit / clipboard | `commit` | `engine.rs` action choice; clipboard path in `paste.rs` (set clipboard, inject Ctrl+V, debounced restore) | complete |
| Debug log (`debug_log`) | `false` | `debug_log.rs`; enabled at startup and on config reload | complete |

## Macros, dictionary, spell check

| Option (config key) | Default | Implemented in | Status |
|---|---|---|---|
| Macros (`macro_enabled`) | `true` | `engine.rs` `expand_macro` at word end, all modes | complete |
| Macro capitalization (`capitalize_macro`) | `true` | `macros.rs` first-letter / all-caps modes | complete |
| Macros in OFF mode (`macro_in_off_mode`) | `false` | `engine.rs` `process_char_off` | complete |
| `$TIME` / `$DATE` formats (`time_format`, `date_format`) | `%H:%M`, `%d/%m/%Y` | `macros.rs` strftime substitution | complete |
| Custom dictionary (`dictionary_enabled`) | `false` | `macros.rs` `Dictionary`; suppresses auto-restore for its words (lookup-only by design) | complete |
| Spell-check word list (no config key — the panel toggle maps to `auto_correct`) | — | `spellcheck.rs` loads the bundled 38,350-word list; `engine.rs` `known_word()` is the API | advisory-only: `known_word()` has no callers yet; auto-correct itself is bamboo-core phonotactics, the word list plays no role in it |

## App rules, GUI, backup

| Option | Default | Implemented in | Status |
|---|---|---|---|
| Per-app mode rules (`app_rules`) | built-ins for chrome/electron-family → preedit, sublime_text → surrounding | `engine.rs` `client_rule_mode` matched against the client program name; GUI editor | complete |
| GTK4 settings window | — | `gui.rs` (all controls write `Config` and save; hot reload within ~1 s) | complete |
| Backup/restore of config + macros + dictionary as one JSON | — | `gui.rs` export/import (version 1) | complete |

## Known gaps

1. `commit_interval_ms` is editable in the GUI Typing tab but never read — the
   commit action is not paced (backspace interval and post delay are). Either
   wire it in or remove the field; leaving it misleads users.
2. The bundled spell-check word list is loaded but idle (lookup-only by design,
   `Engine::known_word()` is the future hook). README wording should not imply
   it backs auto-correction — it does not.
3. The OFF mode doc comment claims auto-capitalize runs in OFF; the
   implementation runs macros only.
