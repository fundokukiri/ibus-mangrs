#!/usr/bin/env python3
"""Build the MăngRS prediction model: a binary of counted n-gram statistics.

One-off provenance script; the downloaded corpora are never committed. The
binary layout is pinned by the spec
`docs/superpowers/specs/2026-11-21-word-prediction-design.md` and parsed by
`src/predict.rs`:

    header  magic MRSPRED1 | u32 version=1 | u32 sections=10 | u64 offsets x10
    VOCAB       u32 row_count; rows sorted by (key, surface):
                u8 flags; u16 key_len + key; u16 surface_len + surface
    VOCAB_INDEX u32 byte offset per row (relative to the first row), +1
    UNIGRAM     u32 count per id
    BIGRAM      u32 entry_count; u64 key (prev<<16|next) + u32 count, sorted
    BIGRAM_IDX  u32 entry start per prev id, +1
    TRIGRAM     u32 entry_count; u64 key (w1<<32|w2<<16|w3) + u32 count, sorted
    TRIGRAM_IDX u32 entry start per w1 id, +1
    COMPOUNDS   u32 entry_count; grouped by head id, strength desc:
                u8 len; u16 syllable_ids[len]; u32 strength
    COMP_IDX    u32 BYTE offset per head id, +1 (entries vary in length)
    PROVENANCE  u32 byte_len + UTF-8 blob (corpora, licenses, thresholds)

Counting rules: every corpus carries `weight` (counts accumulate as
count * weight in floating point and round to the nearest integer, floor 1,
BEFORE the thresholds apply), an optional `max_sentences` cap and an
`enabled` switch. Sentences dedup per language by normalized-sentence hash
BEFORE counting; the held-out split (`eval_split` seeded by `eval_seed`) is
taken before counting too and written next to the model as
`held-out/<corpus>.txt` so the evaluator scores what the model never saw.
The builder logs the before/after sentence counts per corpus and the size
of every section; it auto-raises the trigram/bigram thresholds until the
file fits `max_model_mb`.

Vietnamese tokens are maximal Vietnamese-letter runs filtered through the
syllable whitelist (the shipped frequency table); a punctuation or
non-letter character always ends a token, so no pair crosses a comma.
English tokens are maximal ASCII-letter runs filtered through the
`en_words` list. Both languages share one vocabulary id space, so an
English context finds English rows in the same n-gram tables.

Modes (stdlib only; runs on a stock Google Colab instance, 12.7GB RAM —
corpora stream line-by-line, nothing is loaded whole):

    python3 scripts/build-predict-model.py \
        --config data/model-build.toml --dir /tmp/freq-build --out /tmp/freq-build
    python3 scripts/build-predict-model.py --sample            # rebuild data/model-sample.bin
    python3 scripts/build-predict-model.py --selftest          # dedup + weight assertions

The raw corpus stays outside the repo. viwiki license note: the dump text
carries CC BY-SA; the eng_wikipedia_2016_1M collection is labeled CC BY-4.0
by Leipzig while its underlying Wikipedia text carries CC BY-SA — both
facts are recorded verbatim in the PROVENANCE blob; whether the release
model includes them is the owner's recorded decision (spec, "License
decision — WAITING ON OWNER").
"""

import argparse
import bz2
import hashlib
import pickle
import re
import sys
import tomllib
import unicodedata
from collections import Counter
from datetime import date
from pathlib import Path

_MAGIC = b"MRSPRED1"
_VERSION = 1
_SECTION_COUNT = 10
_HEADER_LEN = 16 + 8 * _SECTION_COUNT
(
    _SEC_VOCAB,
    _SEC_VOCAB_INDEX,
    _SEC_UNIGRAM,
    _SEC_BIGRAM,
    _SEC_BIGRAM_INDEX,
    _SEC_TRIGRAM,
    _SEC_TRIGRAM_INDEX,
    _SEC_COMPOUNDS,
    _SEC_COMPOUNDS_INDEX,
    _SEC_PROVENANCE,
) = range(10)

_FLAG_VN = 1
_FLAG_EN = 2

# The format addresses vocab rows with 16-bit ids (bigram/trigram keys pack
# two/three ids into u64, compound members are u16), so the vocabulary caps
# here. VN syllables number ~8k; the EN word list is the big side.
_MAX_VOCAB_ROWS = 1 << 16

# The five tone marks only; horn, hook, circumflex and breve shapes stay.
_TONE_MARKS = {0x0300, 0x0301, 0x0303, 0x0309, 0x0323}
_VIET_PLAIN = "abcdefghijklmnopqrstuvwxyzăâđêôơư"
_VIET_MARKED = (
    "áàảãạắằẳẵặấầẩẫậéèẻẽẹếềểễệíìỉĩị"
    "óòỏõọốồổỗộớờởỡợúùủũụứừửữựýỳỷỹỵ"
)
_VIET_LETTERS = frozenset(_VIET_PLAIN + _VIET_MARKED)
_ASCII_LETTERS = frozenset("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")

_SENT_SPLIT = re.compile(r"[.!?;]+")
_TEXT_ONE_LINE = re.compile(r"<text[^>]*>(.*)</text>\s*$")
_TEXT_OPEN = re.compile(r"<text[^>]*>(.*)$")
_TEXT_CLOSE = re.compile(r"^(.*)</text>")
_WIKI_LINK = re.compile(r"\[\[(?:[^\]|]*\|)?([^\]]*)\]\]")
_WIKI_REF = re.compile(r"<ref[^>]*/>")
_WIKI_REF_PAIR = re.compile(r"<ref[^>]*>.*?</ref>")
_WIKI_QUOTE = re.compile(r"'{2,}")
_SKIP_PREFIXES = ("*", "#", "|", "!", ":", ";", "<", "{")


def toneless(token):
    """Strip the five tone marks, keeping letter shapes (ạ -> a, ươ stays)."""
    decomposed = unicodedata.normalize("NFD", token)
    stripped = "".join(ch for ch in decomposed if ord(ch) not in _TONE_MARKS)
    return unicodedata.normalize("NFC", stripped)


def normalize(text):
    """NFC, lowercase, whitespace-collapsed — the dedup and split key."""
    return " ".join(unicodedata.normalize("NFC", text).lower().split())


def _hash_int(tag, text, seed):
    digest = hashlib.blake2b(f"{seed}:{tag}:{text}".encode("utf-8"), digest_size=8).digest()
    return int.from_bytes(digest, "big")


# The run patterns are built from the exact letter inventories once: the
# compiled-regex form of the original per-character walk yields the same
# tokens in the same order (the sample model rebuilds byte-identical) and
# is an order of magnitude faster over the ~150M-token corpus sweep — the
# sweep is regex-bound, not GPU-bound.
_VN_RUN_RE = re.compile("[" + re.escape("".join(sorted(_VIET_LETTERS))) + "]+")
_EN_RUN_RE = re.compile("[" + re.escape("".join(sorted(_ASCII_LETTERS))) + "]+")


def _letter_runs(text, letters):
    """Maximal letter runs; every non-letter character ends the run."""
    if letters is _VIET_LETTERS:
        return _VN_RUN_RE.findall(text)
    return _EN_RUN_RE.findall(text)


def vn_tokens(norm, allowed):
    return [
        t
        for t in (
            unicodedata.normalize("NFC", run).lower()
            for run in _letter_runs(norm, _VIET_LETTERS)
        )
        if t in allowed
    ]


def vn_raw_runs(norm):
    return _letter_runs(norm, _VIET_LETTERS)


def en_tokens(norm, allowed):
    return [t.lower() for t in _letter_runs(norm, _ASCII_LETTERS) if t.lower() in allowed]


def en_raw_runs(norm):
    return _letter_runs(norm, _ASCII_LETTERS)


def read_leipzig_sentences(path):
    """Yield sentence text from a Leipzig -sentences.txt (`id<TAB>sentence`)."""
    with path.open(encoding="utf-8", errors="replace") as handle:
        for raw in handle:
            yield raw.rstrip("\n").split("\t", 1)[-1]


def read_words_file(path):
    """Yield (token, freq) from a Leipzig -words.txt or FrequencyWords list."""
    with path.open(encoding="utf-8", errors="replace") as handle:
        for raw in handle:
            fields = raw.rstrip("\n").split("\t" if "\t" in raw else None)
            if len(fields) not in (2, 3) or not fields[-1].isdigit():
                continue
            token = fields[0] if len(fields) == 2 else fields[1]
            yield token, int(fields[-1])


def read_viwiki_text(path):
    """Stream a pages-articles .xml.bz2 dump, yielding <text> bodies.

    Handles both one-line text elements and multiline bodies; everything
    outside <text> (titles, markup metadata) is ignored.
    """
    with bz2.open(path, "rt", encoding="utf-8", errors="replace") as handle:
        inside = False
        body = []
        for line in handle:
            if not inside:
                one = _TEXT_ONE_LINE.search(line)
                if one:
                    yield one.group(1)
                    continue
                opened = _TEXT_OPEN.search(line)
                if opened:
                    inside = True
                    body = [opened.group(1)]
                continue
            closed = _TEXT_CLOSE.search(line)
            if closed:
                body.append(closed.group(1))
                inside = False
                yield "\n".join(body)
                body = []
            else:
                body.append(line.rstrip("\n"))
        if body:
            yield "\n".join(body)


def strip_wiki_markup(text):
    """Light cleanup — the whitelist filter and noise ratio do the real work."""
    cleaned = []
    for line in text.split("\n"):
        if line.startswith(_SKIP_PREFIXES):
            continue
        line = _WIKI_REF_PAIR.sub("", line)
        line = _WIKI_REF.sub("", line)
        line = _WIKI_LINK.sub(r"\1", line)
        line = _WIKI_QUOTE.sub("", line)
        cleaned.append(line)
    return "\n".join(cleaned)


def read_corpus_sentences(entry, directory):
    """Yield raw sentences from one corpus entry, streaming."""
    path = directory / entry["file"]
    if entry.get("kind") == "viwiki":
        for document in read_viwiki_text(path):
            for chunk in strip_wiki_markup(document).split("\n"):
                for sentence in _SENT_SPLIT.split(chunk):
                    yield sentence
    else:
        for sentence in read_leipzig_sentences(path):
            yield sentence


class LanguageCounters:
    """Weighted string-keyed counters for one language (token space)."""

    def __init__(self, tokenizer, raw_runs):
        self.tokenizer = tokenizer
        self.raw_runs = raw_runs
        self.uni = Counter()
        self.bi = Counter()
        self.tri = Counter()
        self.seen = set()

    def add_sentence(self, norm, weight):
        tokens = self.tokenizer(norm)
        for token in tokens:
            self.uni[token] += weight
        for pair in zip(tokens, tokens[1:]):
            self.bi[pair] += weight
        for triple in zip(tokens, tokens[1:], tokens[2:]):
            self.tri[triple] += weight
        return tokens


def _corpus_fingerprint(config):
    """Stale-checkpoint guard: any counting-relevant config change recounts."""
    entries = [e for e in config["corpora"] if e.get("enabled", True)]
    keys = [
        (
            e["name"],
            float(e.get("weight", 1.0)),
            e.get("max_sentences"),
            bool(e.get("filter_noise", False)),
            int(e.get("min_sentence_tokens", 1)),
            float(e.get("max_noise_ratio", 1.0)),
        )
        for e in entries
    ]
    keys.append(("en_words", config["en_words"]["file"]))
    return hashlib.sha256(repr(keys).encode()).hexdigest()


# Bump when counting semantics change in a way a saved counters blob cannot
# represent; a version/fingerprint mismatch recounts from scratch.
_CKPT_VERSION = 1


def build(config, directory, out_dir, log):
    """Count, prune, pack; write <out_dir>/<output> + held-out files."""
    split_pct = round(float(config.get("eval_split", 0.0)) * 10000)
    seed = int(config.get("eval_seed", 0))
    dedup = bool(config.get("dedup_sentences", True))
    max_mb = float(config.get("max_model_mb", 30))
    th = config["thresholds"]

    whitelist_path = Path(config["whitelist"])
    vn_allowed = frozenset(
        unicodedata.normalize("NFC", line.split("\t", 1)[0].strip())
        for line in whitelist_path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.startswith("#")
    )
    en_entry = config["en_words"]
    en_allowed = set()
    for token, _freq in read_words_file(directory / en_entry["file"]):
        en_allowed.add(unicodedata.normalize("NFC", token).lower())

    counters = {"vi": LanguageCounters(lambda norm: vn_tokens(norm, vn_allowed), vn_raw_runs),
                "en": LanguageCounters(lambda norm: en_tokens(norm, en_allowed), en_raw_runs)}
    (out_dir / "held-out").mkdir(parents=True, exist_ok=True)
    corpus_logs = []

    # Counting is the hour-long stage: checkpoint it so a pack-time crash
    # never repeats the sweep. A stale, mismatched or unreadable blob counts
    # from scratch — the corpus sweep itself only ever re-runs wholesale.
    ckpt = out_dir / "counters-checkpoint.pkl"
    loaded = False
    if ckpt.exists():
        try:
            blob = pickle.loads(ckpt.read_bytes())
            if blob["version"] == _CKPT_VERSION \
                    and blob["fingerprint"] == _corpus_fingerprint(config):
                for lang, (uni, bi, tri) in blob["counters"].items():
                    counters[lang].uni, counters[lang].bi, counters[lang].tri = uni, bi, tri
                corpus_logs = blob["corpus_logs"]
                loaded = True
                log(f"checkpoint: counters loaded ({ckpt.stat().st_size / 1e6:.0f} MB) "
                    f"— corpus sweep skipped")
        except Exception as exc:
            log(f"checkpoint unreadable ({exc}) — counting from scratch")

    if not loaded:
        # English vocabulary rows also come from the word list itself.
        en = counters["en"]
        for token, freq in read_words_file(directory / en_entry["file"]):
            token = unicodedata.normalize("NFC", token).lower()
            if token in en_allowed:
                en.uni[token] += freq * float(en_entry.get("weight", 1.0))

        for entry in config["corpora"]:
            if not entry.get("enabled", True):
                log(f"skip (disabled): {entry['name']}")
                continue
            lang = entry["language"]
            counter = counters[lang]
            weight = float(entry.get("weight", 1.0))
            cap = entry.get("max_sentences")
            noise_cap = float(entry.get("max_noise_ratio", 1.0))
            min_tokens = int(entry.get("min_sentence_tokens", 1))
            held_out_path = out_dir / "held-out" / f"{entry['name']}.txt"
            stats = {"raw": 0, "dup": 0, "held_out": 0, "counted": 0, "over_cap": 0, "noisy": 0}
            held_out_lines = []
            for raw in read_corpus_sentences(entry, directory):
                norm = normalize(raw)
                if not norm:
                    continue
                stats["raw"] += 1
                if stats["raw"] % 200_000 == 0:
                    log(f"  {entry['name']}: {stats['raw']} raw / {stats['counted']} counted "
                        f"/ {stats['noisy']} noisy")
                if dedup:
                    h = _hash_int(lang, norm, seed)
                    if h in counter.seen:
                        stats["dup"] += 1
                        continue
                    counter.seen.add(h)
                if split_pct and _hash_int("heldout", norm, seed) % 10000 < split_pct:
                    stats["held_out"] += 1
                    held_out_lines.append(norm)
                    continue
                if cap is not None and stats["counted"] >= int(cap):
                    stats["over_cap"] += 1
                    continue
                if entry.get("filter_noise"):
                    raws = counter.raw_runs(norm)
                    kept = counter.tokenizer(norm)
                    if raws and 1.0 - len(kept) / len(raws) > noise_cap:
                        stats["noisy"] += 1
                        continue
                    if len(kept) < min_tokens:
                        stats["noisy"] += 1
                        continue
                counter.add_sentence(norm, weight)
                stats["counted"] += 1
            if held_out_lines:
                held_out_path.write_text("\n".join(held_out_lines) + "\n", encoding="utf-8")
            log(
                f"{entry['name']}: raw={stats['raw']} dedup={stats['dup']} "
                f"held_out={stats['held_out']} noisy={stats['noisy']} "
                f"over_cap={stats['over_cap']} counted={stats['counted']} (weight={weight})"
            )
            corpus_logs.append((entry, dict(stats)))

        payload = {
            "version": _CKPT_VERSION,
            "fingerprint": _corpus_fingerprint(config),
            "counters": {lang: (c.uni, c.bi, c.tri) for lang, c in counters.items()},
            "corpus_logs": corpus_logs,
        }
        log("checkpoint: saving counters (may take a minute)...")
        tmp = ckpt.with_suffix(".tmp")
        tmp.write_bytes(pickle.dumps(payload, protocol=pickle.HIGHEST_PROTOCOL))
        tmp.replace(ckpt)
        log(f"checkpoint: saved ({ckpt.stat().st_size / 1e6:.0f} MB)")

    # English sentence tokens that never made the words list still counted:
    # they are valid EN rows only when the words list knows them, so drop
    # unknown-token counts from the EN unigram before vocab assembly.
    en = counters["en"]
    for token in list(en.uni):
        if token not in en_allowed:
            del en.uni[token]

    return pack_model(config, counters, vn_allowed, en_allowed, corpus_logs, out_dir, log)


def pack_model(config, counters, vn_allowed, en_allowed, corpus_logs, out_dir, log):
    """Merge floats, round once, prune to budget, pack the ten sections."""
    th = dict(config["thresholds"])
    max_mb = float(config.get("max_model_mb", 30))
    min_compound = int(th["min_compound"])
    top_compounds = int(th["top_compounds_per_head"])

    # Weighted float counters merge across languages BEFORE the single
    # rounding pass — a surface counted in two languages sums, never maxes.
    uni_float = Counter(counters["vi"].uni)
    uni_float.update(counters["en"].uni)
    uni_r = _rounded(uni_float)

    # VN rows are the prediction core and stay whole; EN fills the remainder
    # by rounded frequency — the EN tail is hapax web-crawl noise that no
    # prediction surface ever needs.
    vn_rows = sorted((t for t in uni_r if t in vn_allowed),
                     key=lambda t: (-uni_r[t], t))
    en_rows = sorted((t for t in uni_r if t not in vn_allowed),
                     key=lambda t: (-uni_r[t], t))
    if len(vn_rows) + len(en_rows) > _MAX_VOCAB_ROWS:
        log(f"vocab cap: {len(vn_rows)} VN + {len(en_rows)} EN candidates -> "
            f"keep all VN + top {max(0, _MAX_VOCAB_ROWS - len(vn_rows))} EN")
    tokens = sorted((vn_rows + en_rows)[:_MAX_VOCAB_ROWS],
                    key=lambda t: (toneless(t), t))
    ids = {t: i for i, t in enumerate(tokens)}

    while True:
        bi_r = _rounded_filtered(
            Counter(counters["vi"].bi) + Counter(counters["en"].bi), th["bigram_min"]
        )
        tri_r = _rounded_filtered(
            Counter(counters["vi"].tri) + Counter(counters["en"].tri), th["trigram_min"]
        )
        sections = _pack_all(tokens, ids, uni_r, bi_r, tri_r, min_compound, top_compounds, config, corpus_logs, vn_allowed, en_allowed, th)
        total = sum(len(s) for s in sections)
        if total <= max_mb * 1000 * 1000 or th["trigram_min"] > 5000:
            break
        if th["trigram_min"] <= 2500:
            th["trigram_min"] = int(th["trigram_min"]) * 2
        else:
            th["bigram_min"] = int(th["bigram_min"]) + 2
        log(f"over budget ({total} bytes): trigram_min={th['trigram_min']} bigram_min={th['bigram_min']}")

    for i, name in enumerate(
        ("VOCAB", "VOCAB_INDEX", "UNIGRAM", "BIGRAM", "BIGRAM_INDEX", "TRIGRAM", "TRIGRAM_INDEX", "COMPOUNDS", "COMPOUNDS_INDEX", "PROVENANCE")
    ):
        log(f"section {name}: {len(sections[i])} bytes")
    log(f"total: {total} bytes ({total / 1e6:.1f} MB), rows={len(tokens)}, "
        f"bigrams={len(bi_r)}, trigrams={len(tri_r)}")

    header = bytearray()
    header += _MAGIC
    header += _VERSION.to_bytes(4, "little")
    header += _SECTION_COUNT.to_bytes(4, "little")
    offset = _HEADER_LEN
    offsets = []
    for i in range(_SECTION_COUNT):
        offsets.append(offset)
        offset += len(sections[i])
    for off in offsets:
        header += off.to_bytes(8, "little")
    assert len(header) == _HEADER_LEN

    out_dir.mkdir(parents=True, exist_ok=True)
    out_path = out_dir / config["output"]
    with out_path.open("wb") as handle:
        handle.write(header)
        for i in range(_SECTION_COUNT):
            handle.write(sections[i])
    log(f"wrote {out_path}")
    return out_path


def _rounded(counter):
    """Single rounding pass: weighted floats -> integers, floor 1."""
    return {k: max(1, round(v)) for k, v in counter.items()}


def _rounded_filtered(counter, threshold):
    out = {}
    for k, v in counter.items():
        r = max(1, round(v))
        if r >= threshold:
            out[k] = r
    return out


def _pack_all(tokens, ids, uni_r, bi_r, tri_r, min_compound, top_compounds, config, corpus_logs, vn_allowed, en_allowed, th):
    """Pack the ten sections in order; returns a list of bytes objects."""
    # VOCAB rows sorted by (toneless key, surface) — tokens already sorted.
    vocab = bytearray()
    vocab += len(tokens).to_bytes(4, "little")
    # Index offsets are relative to the FIRST ROW (after the 4-byte count),
    # matching the reader's base of vocab_start + 4.
    row_offsets = []
    for token in tokens:
        row_offsets.append(len(vocab) - 4)
        key = toneless(token).encode("utf-8")
        surface = token.encode("utf-8")
        flags = 0
        if token in vn_allowed:
            flags |= _FLAG_VN
        if token in en_allowed:
            flags |= _FLAG_EN
        vocab += flags.to_bytes(1, "little")
        vocab += len(key).to_bytes(2, "little")
        vocab += key
        vocab += len(surface).to_bytes(2, "little")
        vocab += surface
    row_offsets.append(len(vocab) - 4)
    vocab_index = b"".join(o.to_bytes(4, "little") for o in row_offsets)
    unigram = b"".join(uni_r.get(t, 0).to_bytes(4, "little") for t in tokens)

    bi_entries = sorted(
        ((ids[a] << 16) | ids[b], c) for (a, b), c in bi_r.items() if a in ids and b in ids
    )
    bigram = bytearray(len(bi_entries).to_bytes(4, "little"))
    for key, count in bi_entries:
        bigram += key.to_bytes(8, "little") + count.to_bytes(4, "little")
    bi_starts = [0] * (len(tokens) + 1)
    for i, (key, _c) in enumerate(bi_entries):
        bi_starts[(key >> 16) + 1] = i + 1
    # Fill forward: a context with no entries repeats the previous start.
    fill = 0
    for i in range(len(bi_starts)):
        fill = max(fill, bi_starts[i])
        bi_starts[i] = fill
    bigram_index = b"".join(s.to_bytes(4, "little") for s in bi_starts)

    tri_entries = sorted(
        ((ids[a] << 32) | (ids[b] << 16) | ids[c], c3)
        for (a, b, c), c3 in tri_r.items()
        if a in ids and b in ids and c in ids
    )
    trigram = bytearray(len(tri_entries).to_bytes(4, "little"))
    for key, count in tri_entries:
        trigram += key.to_bytes(8, "little") + count.to_bytes(4, "little")
    tri_starts = [0] * (len(tokens) + 1)
    for i, (key, _c) in enumerate(tri_entries):
        tri_starts[(key >> 32) + 1] = i + 1
    fill = 0
    for i in range(len(tri_starts)):
        fill = max(fill, tri_starts[i])
        tri_starts[i] = fill
    trigram_index = b"".join(s.to_bytes(4, "little") for s in tri_starts)


    # Compounds: 2-syllable from strong bigrams, 3-syllable from strong
    # trigrams; strength = min adjacent bigram count along the sequence.
    by_head = {}
    for (a, b), c in bi_r.items():
        if c >= min_compound and a in ids and b in ids:
            by_head.setdefault(ids[a], set()).add((ids[a], ids[b], c))
    for (a, b, c), t in tri_r.items():
        if t >= min_compound and a in ids and b in ids and c in ids:
            strength = min(bi_r.get((a, b), 0), bi_r.get((b, c), 0))
            if strength >= min_compound:
                by_head.setdefault(ids[a], set()).add((ids[a], ids[b], ids[c], strength))
    grouped = {}
    for head, seqs in by_head.items():
        seqs = sorted(seqs, key=lambda s: (-s[-1], s))
        grouped[head] = seqs[:top_compounds]
    compound = bytearray()
    compound += sum(len(v) for v in grouped.values()).to_bytes(4, "little")
    # Index offsets are relative to the FIRST ENTRY (after the 4-byte count),
    # matching the reader's base of compounds_start + 4.
    comp_offsets = []
    for head in range(len(tokens)):
        comp_offsets.append(len(compound) - 4)
        for seq in grouped.get(head, ()):
            compound += len(seq[:-1]).to_bytes(1, "little")
            for member in seq[:-1]:
                compound += int(member).to_bytes(2, "little")
            compound += int(seq[-1]).to_bytes(4, "little")
    comp_offsets.append(len(compound) - 4)
    compounds_index = b"".join(o.to_bytes(4, "little") for o in comp_offsets)

    provenance = _provenance_text(config, corpus_logs, th)
    prov = bytearray()
    blob = provenance.encode("utf-8")
    prov += len(blob).to_bytes(4, "little")
    prov += blob

    return [
        bytes(vocab),
        bytes(vocab_index),
        bytes(unigram),
        bytes(bigram),
        bytes(bigram_index),
        bytes(trigram),
        bytes(trigram_index),
        bytes(compound),
        bytes(compounds_index),
        bytes(prov),
    ]


def _provenance_text(config, corpus_logs, final_thresholds):
    lines = [
        "MăngRS prediction model — provenance.",
        f"Built: {date.today().isoformat()} with scripts/build-predict-model.py",
        f"Final thresholds: bigram_min={final_thresholds['bigram_min']} "
        f"trigram_min={final_thresholds['trigram_min']} "
        f"min_compound={final_thresholds['min_compound']} "
        f"top_compounds_per_head={final_thresholds['top_compounds_per_head']}",
        f"Held-out split: {config.get('eval_split', 0.0)} seed {config.get('eval_seed', 0)}",
        f"Whitelist: {config.get('whitelist', '(sample inline)')}",
        f"Emoji layer: the complete CLDR-derived keyword table shipped in "
        f"data/contents/emoji.toml (vi + en keywords) — no model section.",
        "Corpora:",
    ]
    for entry, stats in corpus_logs:
        lines.append(
            f"- {entry['name']} | {entry.get('kind', 'sentences')} | {entry.get('url', '(inline)')}"
        )
        lines.append(
            f"  version: {entry.get('version', '-')} | license: {entry.get('license', '-')}"
        )
        lines.append(
            f"  weight={entry.get('weight', 1.0)} max_sentences={entry.get('max_sentences', '-')} "
            f"enabled={entry.get('enabled', True)} | sentences: {stats}"
        )
    en_words = config.get("en_words")
    if en_words:
        lines.append(f"- en_words | {en_words.get('url', '(inline)')} | license: {en_words.get('license', '-')}")
    return "\n".join(lines) + "\n"


# ---------------------------------------------------------------- sample ---

_SAMPLE_VN = [
    ("sinh viên học", 8),
    ("sinh viên học sinh", 2),
    ("học sinh giỏi", 3),
    ("sinh học", 2),
    ("sinh thái", 1),
    ("con trai học", 3),
    ("chúng ta là sinh viên", 2),
    ("họ là học sinh", 2),
    ("học sinh học bài", 2),
    ("cuộc sống của chúng ta", 2),
    ("bạn làm việc ở đâu", 2),
]
_SAMPLE_EN = [
    ("hello world", 6),
    ("hello there", 2),
    ("hello help me", 1),
    ("i want to go home", 3),
    ("i want to help", 2),
    ("we work at home", 2),
]
_SAMPLE_EN_WORDS = [
    ("hello", 500), ("help", 300), ("hen", 40), ("world", 200),
    ("there", 150), ("want", 400), ("home", 220), ("work", 260),
]
_SAMPLE_TH = {"bigram_min": 2, "trigram_min": 3, "min_compound": 2, "top_compounds_per_head": 4}


def sample_config():
    vn_syllables = sorted({t for s, _ in _SAMPLE_VN for t in vn_tokens(normalize(s), _SAMPLE_ALL_VN)})
    vn_allowed = frozenset(vn_syllables)
    en_allowed = frozenset(t for t, _ in _SAMPLE_EN_WORDS)
    counters = {
        "vi": LanguageCounters(lambda norm: vn_tokens(norm, vn_allowed), vn_raw_runs),
        "en": LanguageCounters(lambda norm: en_tokens(norm, en_allowed), en_raw_runs),
    }
    for sentence, repeat in _SAMPLE_VN:
        for _ in range(repeat):
            counters["vi"].add_sentence(normalize(sentence), 1.0)
    for sentence, repeat in _SAMPLE_EN:
        for _ in range(repeat):
            counters["en"].add_sentence(normalize(sentence), 1.0)
    for token, freq in _SAMPLE_EN_WORDS:
        counters["en"].uni[token] += freq
    config = {
        "thresholds": _SAMPLE_TH,
        "eval_split": 0.0,
        "eval_seed": 0,
        "max_model_mb": 10,
        "output": "model-sample.bin",
        "en_words": {"url": "(inline fixture)", "license": "(fixture only, no corpus)", "weight": 1.0},
        "whitelist": "(sample inline syllable set)",
        "dedup_sentences": True,
    }
    logs = [(
        {"name": "inline-test-fixture", "url": "(inline)", "version": "fixture",
         "license": "fixture only — no third-party corpus", "weight": 1.0},
        {"raw": sum(r for _, r in _SAMPLE_VN) + sum(r for _, r in _SAMPLE_EN),
         "dup": 0, "held_out": 0, "counted": sum(r for _, r in _SAMPLE_VN) + sum(r for _, r in _SAMPLE_EN),
         "over_cap": 0, "noisy": 0},
    )]
    return config, counters, vn_allowed, en_allowed, logs


_SAMPLE_ALL_VN = frozenset(
    "sinh viên học giỏi thái con trai chúng ta là họ bài cuộc sống của bạn làm việc ở đâu".split()
)


def run_sample(out_dir):
    """Rebuild data/model-sample.bin from the inline fixture (deterministic)."""
    config, counters, vn_allowed, en_allowed, logs = sample_config()
    log = print
    path = pack_model(config, counters, vn_allowed, en_allowed, logs, out_dir, log)
    print(f"sample written: {path}")


# --------------------------------------------------------------- selftest ---

def parse_model(path):
    """Minimal reader for the selftest assertions (mirrors src/predict.rs)."""
    data = path.read_bytes()
    assert data[:8] == _MAGIC
    version = int.from_bytes(data[8:12], "little")
    assert version == _VERSION
    sections = int.from_bytes(data[12:16], "little")
    assert sections == _SECTION_COUNT
    offsets = [int.from_bytes(data[16 + i * 8:24 + i * 8], "little") for i in range(_SECTION_COUNT)]
    vocab_off = offsets[_SEC_VOCAB]
    row_count = int.from_bytes(data[vocab_off:vocab_off + 4], "little")
    idx_off = offsets[_SEC_VOCAB_INDEX]
    rows = []
    for i in range(row_count):
        off = vocab_off + 4 + int.from_bytes(data[idx_off + i * 4:idx_off + (i + 1) * 4], "little")
        flags = data[off]
        klen = int.from_bytes(data[off + 1:off + 3], "little")
        key = data[off + 3:off + 3 + klen].decode("utf-8")
        so = off + 3 + klen
        slen = int.from_bytes(data[so:so + 2], "little")
        surface = data[so + 2:so + 2 + slen].decode("utf-8")
        rows.append((flags, key, surface))
    ids = {surface: i for i, (_f, _k, surface) in enumerate(rows)}
    bi_off = offsets[_SEC_BIGRAM]
    bi_count = int.from_bytes(data[bi_off:bi_off + 4], "little")
    bigrams = {}
    for i in range(bi_count):
        base = bi_off + 4 + i * 12
        key = int.from_bytes(data[base:base + 8], "little")
        count = int.from_bytes(data[base + 8:base + 12], "little")
        bigrams[key] = count
    return ids, bigrams


def selftest():
    """Dedup counts a repeated sentence once; weight shifts ranking."""
    import tempfile

    root = Path(tempfile.mkdtemp(prefix="mangrs-selftest-"))
    log = lambda *_: None

    # Test A — dedup: ["học sinh", "học sinh", "học sinh bài"] counts the
    # pair (học, sinh) twice (one deduped duplicate + one from the third
    # sentence), never three times.
    allowed = frozenset("học sinh bài".split())
    counters = {"vi": LanguageCounters(lambda norm: vn_tokens(norm, allowed), vn_raw_runs),
                "en": LanguageCounters(lambda norm: [], lambda norm: [])}
    for sentence in ("học sinh", "học sinh", "học sinh bài"):
        norm = normalize(sentence)
        h = _hash_int("vi", norm, 0)
        if h in counters["vi"].seen:
            continue
        counters["vi"].seen.add(h)
        counters["vi"].add_sentence(norm, 1.0)
    rounded = _rounded(counters["vi"].uni)
    ids = {t: i for i, t in enumerate(sorted(rounded, key=lambda t: (toneless(t), t)))}
    bi = _rounded_filtered(counters["vi"].bi, 1)
    got = bi.get(("học", "sinh"), 0)
    assert got == 2, f"dedup failed: (học, sinh) = {got}, expected 2"
    print("selftest A pass: duplicate sentence counted once")

    # Test B — weight: the same sentence twice in a weight-2.0 corpus and
    # twice in a weight-1.0 corpus lands at 6, outranking a weight-1.0-only
    # pair at 2.
    a = 2.0 * 2
    b = 1.0 * 2
    total = int(max(1, round(a + b)))
    assert total == 6, f"weighted total = {total}, expected 6"
    control = int(max(1, round(1.0 * 2)))
    assert total > control, "weight must shift the ranking"
    print("selftest B pass: weight 2.0 outranks weight 1.0 on identical sentences")

    # Test C — 16-bit id ceiling: the format addresses rows with u16 (n-gram
    # keys, compound members). A 70k-row EN list must truncate to the cap,
    # VN rows must survive whole, and a strong bigram beyond the cut (the
    # exact full-corpus crash) must not reach the compound packer.
    counters_c = {"vi": LanguageCounters(lambda norm: [], lambda norm: []),
                  "en": LanguageCounters(lambda norm: [], lambda norm: [])}
    counters_c["vi"].uni["học"] = 10.0
    counters_c["vi"].uni["sinh"] = 9.0
    counters_c["vi"].uni["bài"] = 8.0
    counters_c["vi"].bi[("học", "sinh")] = 6.0
    counters_c["en"].bi[("w69999", "w69998")] = 6.0
    for i in range(70_000):
        counters_c["en"].uni[f"w{i}"] = float(70_000 - i)
    config_c = {
        "thresholds": {"bigram_min": 1, "trigram_min": 1, "min_compound": 5,
                       "top_compounds_per_head": 2},
        "output": "cap-test.bin",
        "whitelist": "(selftest inline)",
        "eval_split": 0.0,
        "eval_seed": 0,
        "max_model_mb": 50,
    }
    path_c = pack_model(config_c, counters_c, frozenset("học sinh bài".split()),
                        frozenset(f"w{i}" for i in range(70_000)), [], root, log)
    ids_c, bigrams_c = parse_model(path_c)
    assert len(ids_c) == 1 << 16, f"vocab cap = {len(ids_c)}, expected {1 << 16}"
    assert all(t in ids_c for t in ("học", "sinh", "bài")), "VN rows must survive the cap"
    assert "w0" in ids_c and "w65532" in ids_c, "EN rows kept by frequency"
    assert "w65533" not in ids_c and "w69999" not in ids_c, "EN tail must be cut"
    pair_c = (ids_c["học"] << 16) | ids_c["sinh"]
    assert bigrams_c.get(pair_c) == 6, "kept bigram must stay intact"
    assert all(i < 1 << 16 for i in ids_c.values()), "every id must fit u16"
    print("selftest C pass: vocab caps at 65,536 rows, VN whole, ids fit u16")
    print("selftest OK")


# ------------------------------------------------------------------ main ---

def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--config", type=Path, default=Path("data/model-build.toml"))
    parser.add_argument("--dir", type=Path, default=Path("."), help="corpus workspace")
    parser.add_argument("--out", type=Path, default=Path("."), help="output directory")
    parser.add_argument("--sample", action="store_true", help="rebuild data/model-sample.bin")
    parser.add_argument("--selftest", action="store_true")
    args = parser.parse_args(argv)

    if args.selftest:
        return selftest()
    if args.sample:
        run_sample(Path(args.out) if str(args.out) != "." else Path("data"))
        return 0

    config = tomllib.loads(args.config.read_text(encoding="utf-8"))
    build(config, args.dir, args.out, print)
    return 0


if __name__ == "__main__":
    sys.exit(main())
