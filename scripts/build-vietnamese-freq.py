#!/usr/bin/env python3
"""Build data/vietnamese-freq.dict from a Vietnamese word-frequency list.

One-off provenance script for the suggestion-strip data table; the downloaded
corpus itself is never committed. It reads a token/frequency list — the
Leipzig Corpora Collection `-words.txt` layout (`rank<TAB>token<TAB>freq`) or
a hermitd/FrequencyWords layout (`token<WHITESPACE>freq`) — normalizes each
token to NFC, lowercases, keeps purely-alphabetic Vietnamese tokens, dedupes
keeping the max frequency, and emits `syllable<TAB>freq` rows sorted by
toneless form with an NFC tie-break.

The task brief names the token class `[a-zđà-ỹăâêôơưĐ]`, but the U+00E0-U+1EF9
range inside it admits every script from Latin-1 to Latin Extended Additional
(Cyrillic, Greek, Hebrew...). The filter below therefore uses the exact
Vietnamese letter inventory — the same set the data test in src/suggest.rs
accepts, so script output and test validation cannot drift apart.

Toneless form: NFD-decompose, drop the five tone marks (sắc U+0301, huyền
U+0300, hỏi U+0309, ngã U+0303, nặng U+0323), NFC-recompose. Horn, hook,
circumflex and breve shapes survive (ô stays ô, ươ stays ươ).

Re-run (from the repo root; the raw corpus stays outside the repo). The
whitelist is ibus-bamboo's syllable inventory, a build input only — the
invocation and the labels below are what the shipped data header carries:
    curl -o /tmp/freq-build/vie_mixed_2014_1M.tar.gz \
        https://downloads.wortschatz-leipzig.de/corpora/vie_mixed_2014_1M.tar.gz
    tar xzf /tmp/freq-build/vie_mixed_2014_1M.tar.gz -C /tmp/freq-build
    python3 scripts/build-vietnamese-freq.py \
        /tmp/freq-build/vie_mixed_2014_1M/vie_mixed_2014_1M-words.txt \
        --source "Leipzig Corpora Collection, vie_mixed_2014_1M (mixed, 2014, 1M sentences), downloads.wortschatz-leipzig.de" \
        --license "CC BY-4.0" \
        --syllables /usr/share/ibus-bamboo/data/vietnamese.cm.dict

Standard library only.
"""

import argparse
import sys
import unicodedata
from datetime import date
from pathlib import Path

# The five tone marks only; horn, hook, circumflex and breve shapes stay.
_TONE_MARKS = {0x0300, 0x0301, 0x0303, 0x0309, 0x0323}

# Plain letters plus the 60 tone-marked vowel forms: every character an
# emitted syllable may contain (lowercase NFC).
_VIET_PLAIN = "abcdefghijklmnopqrstuvwxyzăâđêôơư"
_VIET_MARKED = (
    "áàảãạắằẳẵặấầẩẫậéèẻẽẹếềểễệíìỉĩị"
    "óòỏõọốồổỗộớờởỡợúùủũụứừửữựýỳỷỹỵ"
)
_VIET_LETTERS = frozenset(_VIET_PLAIN + _VIET_MARKED)

OUTPUT = Path("data/vietnamese-freq.dict")


def toneless(token: str) -> str:
    """Strip the five tone marks, keeping letter shapes (ơ stays ơ, ạ -> a)."""
    decomposed = unicodedata.normalize("NFD", token)
    stripped = "".join(ch for ch in decomposed if ord(ch) not in _TONE_MARKS)
    return unicodedata.normalize("NFC", stripped)


def read_tokens(path: Path):
    """Yield (token, freq) pairs from a word-frequency list.

    Accepts both observed layouts: three tab-separated fields with a leading
    rank (Leipzig `-words.txt`) and two whitespace-separated fields
    (FrequencyWords). Frequency is always the last field, so a numeric token
    like `1` is dropped with the rank field instead of being taken as a word.
    """
    with path.open(encoding="utf-8") as handle:
        for raw in handle:
            fields = raw.rstrip("\n").split("\t" if "\t" in raw else None)
            if len(fields) not in (2, 3) or not fields[-1].isdigit():
                continue
            token = fields[0] if len(fields) == 2 else fields[1]
            yield token, int(fields[-1])


def build(path: Path, source: str, license_label: str, whitelist: Path | None):
    """Filter, merge and sort the frequency list; return the file text."""
    allowed = None
    if whitelist is not None:
        allowed = {
            unicodedata.normalize("NFC", line.strip())
            for line in whitelist.read_text(encoding="utf-8").splitlines()
            if line.strip()
        }

    freqs: dict[str, int] = {}
    for raw_token, freq in read_tokens(path):
        if freq < 1:
            continue
        # NFC first, then lowercase; the letter-set filter below rejects the
        # rare lowercase results that decompose, so rows stay NFC.
        token = unicodedata.normalize("NFC", raw_token).lower()
        if not token or not set(token) <= _VIET_LETTERS:
            continue
        if allowed is not None and token not in allowed:
            continue
        freqs[token] = max(freq, freqs.get(token, 0))

    rows = sorted(freqs, key=lambda token: (toneless(token), token))
    header_lines = [
        "# Vietnamese syllable frequency table for MăngRS.",
        f"# Source: {source}",
        f"# License: {license_label}",
    ]
    if whitelist is not None:
        header_lines.append(f"# Whitelist: {whitelist.name} syllables only")
    header_lines += [
        f"# Built: {date.today().isoformat()} with scripts/build-vietnamese-freq.py",
        f"# Rows: {len(rows)}",
        "# Format: NFC `syllable<TAB>freq`, sorted by toneless form, NFC tie-break.",
    ]
    header = "\n".join(header_lines)
    body = "\n".join(f"{token}\t{freqs[token]}" for token in rows)
    return header + "\n" + body + "\n"


def main(argv=None) -> int:
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("input", type=Path, help="word-frequency list to read")
    parser.add_argument("--source", required=True, help="provenance label for the header")
    parser.add_argument("--license", required=True, help="license label for the header")
    parser.add_argument(
        "--syllables",
        type=Path,
        default=None,
        help="optional whitelist file, one syllable per line; keep only its members",
    )
    args = parser.parse_args(argv)

    text = build(args.input, args.source, args.license, args.syllables)
    OUTPUT.write_text(text, encoding="utf-8")
    rows = sum(1 for line in text.splitlines() if line and not line.startswith("#"))
    print(f"wrote {OUTPUT} with {rows} rows from {args.input}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
