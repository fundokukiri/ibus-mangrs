# Data files

## `ibus-mangrs.xml`

IBus component registration, installed to `/usr/share/ibus/component/` by
`scripts/install.sh`.

## `80-uinput.rules`

udev rule giving the `input` group access to `/dev/uinput` (the backspace
injector), installed to `/etc/udev/rules.d/` by `scripts/install.sh` and to
`/usr/lib/udev/rules.d/` by the PKGBUILD. See `docs/install.md`.

## `vn-words.txt` — spell-check word list

38 350 Vietnamese words, UTF-8 NFC, one word per line, `#` comments. Loaded by
`src/spellcheck.rs` when the Spell Check toggle is on. Lookup-only: the list
never changes what the engine commits — it backs word checks and future
suggestion features (owner decision, 31/10/2026).

- **Source**: "Lists of the most common Vietnamese words" by Ho Ngoc Duc, Free
  Vietnamese Dictionary Project, distributed as `Viet39K.txt` in
  <https://github.com/duyet/vietnamese-wordlist> (39 071 raw lines; deduplicated
  to 38 350 entries, otherwise unmodified).
- **License**: GPL — the list comes from the Free Vietnamese Dictionary
  Project, and its derivative dictionaries (Firefox, OpenOffice spell checkers)
  are distributed under the GNU GPL. The word list is shipped as a separate
  data file with this notice; the MăngRS code itself stays MIT. If this is
  unacceptable for a redistribution, deleting the file only disables the
  dictionary check (`WordList::load()` falls back to an empty list).
- **Installed to**: `/usr/share/ibus-mangrs/vn-words.txt`. A file at
  `~/.config/ibus-mangrs/vn-words.txt` overrides the system copy.
