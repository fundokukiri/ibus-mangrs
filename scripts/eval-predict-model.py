#!/usr/bin/env python3
"""Offline evaluation of the MăngRS prediction model (word-prediction plan, Task 5).

Scores the builder's held-out files (`held-out/<corpus>.txt`, one normalized
sentence per line) with the semantics the Rust scorer must implement.

State B (next word after a landed word): per-candidate stupid backoff, factor
0.4 per level; denominators are the model's own context row totals (raw
unigram counts are polluted by the EN word list and by pruning, the kept row
mass is not): P(w) = unigram(w)/total; P(w|prev) = bigram(prev,w)/row total,
else 0.4*P(w); P(w|w1,w2) = trigram(w1,w2,w)/row total, else 0.4*P(w|w2), else
0.16*P(w). Every word with a backoff score is a candidate, so the measured
top-k is what a full stupid-backoff scorer returns; ties rank by surface.

State A (compound completion mid-word): for each position whose in-text
continuation is a model compound (longest of the 2-/3-syllable sequences at
the token), the rank of that compound inside compounds_for_prefix() for the
2- and 3-character prefixes of the token's toneless key, ordered by
(strength desc, surface asc).

State B is reported full and bigram-only (trigram tier disabled, so
P(w|w1,w2) := P(w|w2)); the 2-token delta between the columns is the trigram
contribution, while the 1-token rows carry no trigram term and are equal.
Tokenization mirrors the builder (Vietnamese letter runs filtered through the
model's VN rows, ASCII runs through its EN rows); held-out files carry no
language marker, so both tokenizations compete per sentence and the one with
more surviving tokens wins (ties -> VN).

Usage:
    python3 scripts/eval-predict-model.py --model model.bin --held-out-dir held-out/
    python3 scripts/eval-predict-model.py --selftest

A missing or corrupt model, or a held-out dir without .txt files, exits with
a one-line error, never a traceback. Stdlib only, no network, deterministic:
every ranking and listing is explicitly sorted.
"""

import argparse
import sys
import tempfile
import unicodedata
from bisect import bisect_left, insort
from pathlib import Path
from struct import iter_unpack

_MAGIC = b"MRSPRED1"
_VERSION = 1
_SECTIONS = ("VOCAB", "VOCAB_INDEX", "UNIGRAM", "BIGRAM", "BIGRAM_INDEX",
             "TRIGRAM", "TRIGRAM_INDEX", "COMPOUNDS", "COMPOUNDS_INDEX", "PROVENANCE")
_HEADER_LEN = 16 + 8 * len(_SECTIONS)
_FLAG_VN, _FLAG_EN = 1, 2
_ENTRY = 12  # bigram/trigram entry: u64 packed key + u32 count
_BACKOFF = 0.4

_VIET_PLAIN = "abcdefghijklmnopqrstuvwxyzăâđêôơư"
_VIET_MARKED = ("áàảãạắằẳẵặấầẩẫậéèẻẽẹếềểễệíìỉĩị"
                "óòỏõọốồổỗộớờởỡợúùủũụứừửữựýỳỷỹỵ")
_VIET_LETTERS = frozenset(_VIET_PLAIN + _VIET_MARKED)
_ASCII_LETTERS = frozenset("abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ")


class EvalError(Exception):
    """Clean CLI failure: bad model file, bad held-out input."""


def normalize(text):
    """NFC, lowercase, whitespace-collapsed — the builder's sentence key."""
    return " ".join(unicodedata.normalize("NFC", text).lower().split())


def _letter_runs(text, letters):
    """Maximal letter runs; every non-letter character ends the run."""
    runs, run = [], ""
    for ch in text + "\n":  # sentinel flushes the final run
        if ch in letters:
            run += ch
        elif run:
            runs.append(run)
            run = ""
    return runs


class Model:
    """Reader for the pinned MRSPRED1 layout (mirrors src/predict.rs).

    VOCAB_INDEX offsets are relative to the first row (base vocab_start + 4),
    COMPOUNDS_INDEX byte offsets relative to the first compound entry (base
    compounds_start + 4), BIGRAM/TRIGRAM indexes fixed-width 12-byte entry
    indices. Entries stay in the raw bytes and are sliced per context through
    the offset indexes, so a full 30MB model never materializes in Python.
    """

    def __init__(self, data):
        self.byte_len = len(data)
        if data[:8] != _MAGIC:
            raise EvalError("bad magic (not an MRSPRED1 model)")
        if int.from_bytes(data[8:12], "little") != _VERSION or int.from_bytes(data[12:16], "little") != len(_SECTIONS):
            raise EvalError("unsupported version or section count")
        offs = [int.from_bytes(data[16 + 8 * i:24 + 8 * i], "little") for i in range(len(_SECTIONS))]
        ends = offs[1:] + [len(data)]
        for i, (start, end) in enumerate(zip(offs, ends)):
            if start < _HEADER_LEN or not _HEADER_LEN <= start <= end <= len(data):
                raise EvalError(f"section {_SECTIONS[i]} out of file bounds")
        sec = [end - start for start, end in zip(offs, ends)]
        vocab_off, idx_off, uni_off, bi_off, bidx_off, tri_off, tidx_off, comp_off, cidx_off, prov_off = offs
        rows = self.row_count = int.from_bytes(data[vocab_off:vocab_off + 4], "little")
        idx_size = (rows + 1) * 4
        if [sec[1], sec[2], sec[4], sec[6], sec[8]] != [idx_size, rows * 4, idx_size, idx_size, idx_size]:
            raise EvalError("index/unigram section size disagrees with row_count")

        starts = self._index(data, idx_off, rows)
        vocab_end, prev_key = vocab_off + sec[0], ""
        self.flags, self.keys, self.surfaces = [], [], []
        for i in range(rows):
            base, row_end = vocab_off + 4 + starts[i], vocab_off + 4 + starts[i + 1]
            if not vocab_off + 4 <= base < row_end <= vocab_end:
                raise EvalError("vocab row out of section bounds")
            klen = int.from_bytes(data[base + 1:base + 3], "little")
            so, slen = base + 3 + klen, int.from_bytes(data[base + 3 + klen:base + 5 + klen], "little")
            if so + 2 + slen > row_end:
                raise EvalError("vocab row overruns its index slot")
            try:
                key, surface = data[base + 3:so].decode("utf-8"), data[so + 2:so + 2 + slen].decode("utf-8")
            except UnicodeDecodeError:
                raise EvalError("vocab row holds invalid UTF-8") from None
            if key < prev_key:
                raise EvalError("vocab rows not sorted by (key, surface)")
            prev_key = key
            self.flags.append(data[base])
            self.keys.append(key)
            self.surfaces.append(surface)
        self._id_of = {s: i for i, s in enumerate(self.surfaces)}
        self.vn_ids = {s: i for i, (f, s) in enumerate(zip(self.flags, self.surfaces)) if f & _FLAG_VN}
        self.en_ids = {s: i for i, (f, s) in enumerate(zip(self.flags, self.surfaces)) if f & _FLAG_EN}
        self.unigrams = [int.from_bytes(data[uni_off + 4 * i:uni_off + 4 * (i + 1)], "little") for i in range(rows)]
        self.uni_total = sum(self.unigrams) or 1
        # Backoff-tier ranking: one multiplier times the count, so count order
        # with a surface tie-break is exactly score order.
        self._uni_order = sorted((i for i in range(rows) if self.unigrams[i]),
                                 key=lambda i: (-self.unigrams[i], self.surfaces[i]))
        self.bi_starts, self.tri_starts, self.comp_starts = (self._index(data, o, rows)
                                                             for o in (bidx_off, tidx_off, cidx_off))
        self.n_bigrams = int.from_bytes(data[bi_off:bi_off + 4], "little")
        self.n_trigrams = int.from_bytes(data[tri_off:tri_off + 4], "little")
        self.n_compounds = int.from_bytes(data[comp_off:comp_off + 4], "little")
        if (sec[3] != 4 + _ENTRY * self.n_bigrams or sec[5] != 4 + _ENTRY * self.n_trigrams
                or self.bi_starts[-1] != self.n_bigrams or self.tri_starts[-1] != self.n_trigrams
                or sec[7] < 4 or self.comp_starts[-1] != sec[7] - 4):
            raise EvalError("n-gram/compound section size or end index inconsistent")
        for starts in (self.bi_starts, self.tri_starts, self.comp_starts):
            if any(b < a for a, b in zip(starts, starts[1:])):
                raise EvalError("an offset index is not non-decreasing")
        plen = int.from_bytes(data[prov_off:prov_off + 4], "little")
        if sec[9] != 4 + plen:
            raise EvalError("provenance length disagrees with section size")
        self.provenance = data[prov_off + 4:prov_off + 4 + plen].decode("utf-8", "replace")
        self._data = data
        self._bi_base, self._tri_base, self._comp_base = bi_off + 4, tri_off + 4, comp_off + 4
        self._bi_cache, self._tri_cache, self._head_cache, self._prefix_cache = {}, {}, {}, {}

    @classmethod
    def open(cls, path):
        try:
            data = path.read_bytes()
        except OSError as exc:
            raise EvalError(f"cannot read model {path}: {exc.strerror or exc}") from None
        try:
            return cls(data)
        except EvalError as exc:
            raise EvalError(f"{path}: {exc}") from None

    @staticmethod
    def _index(data, off, rows):
        return [int.from_bytes(data[off + 4 * i:off + 4 * (i + 1)], "little") for i in range(rows + 1)]

    def id_of(self, surface):
        return self._id_of.get(surface)

    def _ngram_row(self, chunk):
        """(count-ordered entries, id->count map, row total) for one context."""
        pairs = [(k & 0xFFFF, c) for k, c in iter_unpack("<QI", chunk)]
        return (sorted(pairs, key=lambda p: (-p[1], self.surfaces[p[0]])),
                dict(pairs), sum(c for _w, c in pairs))

    def bigram_row(self, prev):
        cached = self._bi_cache.get(prev)
        if cached is None:
            lo, hi = self.bi_starts[prev], self.bi_starts[prev + 1]
            self._bi_cache[prev] = cached = self._ngram_row(
                self._data[self._bi_base + _ENTRY * lo:self._bi_base + _ENTRY * hi])
        return cached

    def trigram_row(self, w1, w2):
        cached = self._tri_cache.get((w1, w2))
        if cached is None:
            lo, hi, floor = self.tri_starts[w1], self.tri_starts[w1 + 1], (w1 << 32) | (w2 << 16)
            lo = self._entry_bound(lo, hi, floor)
            hi = self._entry_bound(lo, hi, floor + (1 << 16))
            self._tri_cache[(w1, w2)] = cached = self._ngram_row(
                self._data[self._tri_base + _ENTRY * lo:self._tri_base + _ENTRY * hi])
        return cached

    def _entry_bound(self, lo, hi, floor):
        while lo < hi:
            mid = (lo + hi) // 2
            if int.from_bytes(self._data[self._tri_base + _ENTRY * mid:self._tri_base + _ENTRY * mid + 8],
                              "little") < floor:
                lo = mid + 1
            else:
                hi = mid
        return lo

    def compounds_of_head(self, head):
        cached = self._head_cache.get(head)
        if cached is None:
            out = []
            pos, stop = self._comp_base + self.comp_starts[head], self._comp_base + self.comp_starts[head + 1]
            while pos < stop:
                n = self._data[pos]
                ids = [int.from_bytes(self._data[pos + 1 + 2 * j:pos + 3 + 2 * j], "little") for j in range(n)]
                strength = int.from_bytes(self._data[pos + 1 + 2 * n:pos + 5 + 2 * n], "little")
                pos += 5 + 2 * n
                if pos > stop or any(i >= self.row_count for i in ids):
                    raise EvalError("compound entry overruns its group or cites an unknown row")
                out.append((" ".join(self.surfaces[i] for i in ids), strength))
            self._head_cache[head] = cached = out
        return cached

    def compounds_for_prefix(self, prefix):
        """All compounds whose head key starts with prefix, (strength, surface) best first."""
        cached = self._prefix_cache.get(prefix)
        if cached is None:
            merged = []
            for head in range(bisect_left(self.keys, prefix), bisect_left(self.keys, prefix + "\uffff")):
                merged.extend(self.compounds_of_head(head))
            self._prefix_cache[prefix] = cached = sorted(merged, key=lambda c: (-c[1], c[0]))
        return cached


def _rank(model, target, a_ordered, a_map, a_total, b_ordered, b_map, b_total, b_factor, u_factor):
    """Rank (1..5) of target under stupid backoff over A + backoff tiers, else None.

    The pool seeds with A's own top-5 (anything outside it can never reach
    the overall top-5) and walks tier 1 (b_factor * count / row total) and
    tier 2 (unigram walk) in descending score order, stopping once no
    remaining candidate can displace the current fifth best — stream scores
    only descend, so the result equals ranking the full candidate union.
    """
    pool = [(-c / a_total, model.surfaces[w], w) for w, c in a_ordered[:5]] if a_total else []
    uni, u_scale = model._uni_order, u_factor / model.uni_total
    i = j = 0
    nb = len(b_ordered)
    while True:
        nxt1 = None
        if i < nb:
            w, c = b_ordered[i]
            nxt1 = (-b_factor * c / b_total, model.surfaces[w], w)
        while j < len(uni) and (uni[j] in a_map or uni[j] in b_map):
            j += 1
        nxt2 = (-u_scale * model.unigrams[uni[j]], model.surfaces[uni[j]], uni[j]) if j < len(uni) else None
        if nxt1 is None and nxt2 is None:
            break
        nxt = nxt1 if nxt2 is None or (nxt1 is not None and nxt1 <= nxt2) else nxt2
        if len(pool) >= 5 and nxt >= pool[4]:
            break
        if nxt is nxt2:
            j += 1
            insort(pool, nxt)
        else:
            i += 1
            if nxt[2] not in a_map:  # an A member keeps its A score, never its backoff score
                insort(pool, nxt)
        if len(pool) > 5:
            pool.pop()
    for rank, entry in enumerate(pool, 1):
        if entry[2] == target:
            return rank
    return None


def rank_next_1tok(model, prev, target):
    """Rank of target as the next word after prev (bigram -> unigram backoff)."""
    ordered, amap, total = model.bigram_row(prev)
    return _rank(model, target, ordered, amap, total, (), {}, 1, 0.0, _BACKOFF)


def rank_next_2tok(model, w1, w2, target, use_trigram=True):
    """Rank of target as the next word after the pair (w1, w2). With the
    trigram tier disabled this is exactly the 1-token scoring on w2."""
    if not use_trigram:
        return rank_next_1tok(model, w2, target)
    ordered, amap, total = model.trigram_row(w1, w2)
    b_ordered, b_map, b_total = model.bigram_row(w2)
    return _rank(model, target, ordered, amap, total, b_ordered, b_map, b_total, _BACKOFF, _BACKOFF * _BACKOFF)


def rank_compound(model, head_key, prefix_len, surface):
    """Rank of `surface` among compounds whose head key starts with the
    prefix, ordered by (strength desc, surface asc); None if absent."""
    for rank, (name, _s) in enumerate(model.compounds_for_prefix(head_key[:prefix_len]), 1):
        if name == surface:
            return rank
    return None


def sentence_ids(model, norm):
    """Builder-faithful token ids: maximal letter runs filtered through the
    model's own rows; the language with more surviving tokens wins (ties -> VN)."""
    vn = [model.vn_ids[r] for r in _letter_runs(norm, _VIET_LETTERS) if r in model.vn_ids]
    en = [model.en_ids[r] for r in _letter_runs(norm, _ASCII_LETTERS) if r in model.en_ids]
    return vn if len(vn) >= len(en) else en


class Acc:
    """Hit counter for one metric row; a rank above 5 counts as a miss."""

    __slots__ = ("n", "hits")

    def __init__(self):
        self.n = 0
        self.hits = [0, 0, 0]  # top-1/3/5

    def add(self, rank):
        self.n += 1
        if rank is not None:
            self.hits = [h + (rank <= k) for h, k in zip(self.hits, (1, 3, 5))]

    def rates(self):
        return tuple(100.0 * h / self.n if self.n else 0.0 for h in self.hits)


def _add_compound(model, accs, ids, i):
    """One state-A target: the longest model compound starting at ids[i]."""
    comps = {name for name, _s in model.compounds_of_head(ids[i])}
    two = model.surfaces[ids[i]] + " " + model.surfaces[ids[i + 1]]
    target = two
    if i + 2 < len(ids) and two + " " + model.surfaces[ids[i + 2]] in comps:
        target = two + " " + model.surfaces[ids[i + 2]]
    elif two not in comps:
        return
    key = model.keys[ids[i]]
    if len(key) >= 2:  # the strip stays silent below two typed characters
        accs["comp2"].add(rank_compound(model, key, 2, target))
    if len(key) >= 3:
        accs["comp3"].add(rank_compound(model, key, 3, target))


def eval_sentences(model, name, lines):
    """Score one held-out file: state B at every position with a 1- and
    2-token context, state A at every position with a compound continuation."""
    accs = {k: Acc() for k in ("ctx1", "ctx2_full", "ctx2_bi", "comp2", "comp3")}
    sentences = 0
    for raw in lines:
        norm = normalize(raw)
        ids = sentence_ids(model, norm) if norm else []
        if not ids:
            continue
        sentences += 1
        for i in range(1, len(ids)):
            accs["ctx1"].add(rank_next_1tok(model, ids[i - 1], ids[i]))
        for i in range(2, len(ids)):
            accs["ctx2_full"].add(rank_next_2tok(model, ids[i - 2], ids[i - 1], ids[i]))
            accs["ctx2_bi"].add(rank_next_2tok(model, ids[i - 2], ids[i - 1], ids[i], use_trigram=False))
        for i in range(len(ids) - 1):
            _add_compound(model, accs, ids, i)
    return {"name": name, "sentences": sentences, **accs}


def evaluate(model, files):
    """files: ordered (name, text) pairs. Returns (per-file stats, overall)."""
    per_file = [eval_sentences(model, name, text.splitlines()) for name, text in files]
    overall = {"name": "overall", "sentences": sum(st["sentences"] for st in per_file)}
    for tag in ("ctx1", "ctx2_full", "ctx2_bi", "comp2", "comp3"):
        acc = Acc()
        for st in per_file:
            acc.n += st[tag].n
            acc.hits = [a + b for a, b in zip(acc.hits, st[tag].hits)]
        overall[tag] = acc
    return per_file, overall


def read_held_out(directory):
    if not directory.is_dir():
        raise EvalError(f"held-out dir not found: {directory}")
    files = sorted(directory.glob("*.txt"))
    if not files:
        raise EvalError(f"no .txt files in {directory}")
    out = []
    for path in files:
        try:
            out.append((path.name, path.read_text(encoding="utf-8", errors="replace")))
        except OSError as exc:
            raise EvalError(f"cannot read {path}: {exc.strerror or exc}") from None
    return out


_ACC_TAGS = (("state_b.ctx1.full", "ctx1"), ("state_b.ctx1.bigram_only", "ctx1"),
             ("state_b.ctx2.full", "ctx2_full"), ("state_b.ctx2.bigram_only", "ctx2_bi"),
             ("state_a.prefix2", "comp2"), ("state_a.prefix3", "comp3"))


def _row(label, acc, acc_bi=None):
    r = acc.rates()
    line = f"  {label:<24} n={acc.n:>7}  "
    if acc_bi is None:
        return line + f"top-1 {r[0]:6.2f}  top-3 {r[1]:6.2f}  top-5 {r[2]:6.2f}"
    b = acc_bi.rates()
    return line + (f"full  top-1 {r[0]:6.2f}  top-3 {r[1]:6.2f}  top-5 {r[2]:6.2f}  |  "
                   f"bigram-only  top-1 {b[0]:6.2f}  top-3 {b[1]:6.2f}  top-5 {b[2]:6.2f}")


def format_block(title, st):
    # The 1-token row never involves the trigram tier: full == bigram-only.
    return [f"== {title} ==",
            _row("state B  1-token context", st["ctx1"], st["ctx1"]),
            _row("state B  2-token context", st["ctx2_full"], st["ctx2_bi"]),
            _row("state A  prefix 2 chars", st["comp2"]),
            _row("state A  prefix 3 chars", st["comp3"])]


def summary_lines(per_file, overall, model):
    lines = ["summary.begin", f"model.rows={model.row_count}",
             f"model.bigram_entries={model.n_bigrams}", f"model.trigram_entries={model.n_trigrams}",
             f"model.compound_entries={model.n_compounds}", f"model.bytes={model.byte_len}"]
    for prefix, st in [("overall", overall)] + [(Path(st["name"]).stem, st) for st in per_file]:
        lines.append(f"{prefix}.sentences={st['sentences']}")
        for tag, key in _ACC_TAGS:
            acc, r = st[key], st[key].rates()
            lines += [f"{prefix}.{tag}.n={acc.n}", f"{prefix}.{tag}.top1={r[0]:.2f}",
                      f"{prefix}.{tag}.top3={r[1]:.2f}", f"{prefix}.{tag}.top5={r[2]:.2f}"]
    lines.append("summary.end")
    return lines


def report(model_path, per_file, overall, model):
    lines = [f"model: {model_path} (rows={model.row_count}, bigrams={model.n_bigrams}, "
             f"trigrams={model.n_trigrams}, compounds={model.n_compounds}, {model.byte_len} bytes)"]
    for st in per_file:
        lines.append("")
        lines += format_block(f"{st['name']}: {st['sentences']} sentence(s)", st)
    lines.append("")
    lines += format_block("overall", overall)
    lines.append("")
    lines += summary_lines(per_file, overall, model)
    return "\n".join(lines)


def _default_model():
    return Path(__file__).resolve().parent.parent / "data" / "model-sample.bin"


def _fail(msg):
    print(f"error: {msg}", file=sys.stderr)
    return 2


def selftest(model_path=None):
    """The whole pipeline against the committed sample model: anchors pin the
    scoring semantics, two runs must produce identical reports."""
    with tempfile.TemporaryDirectory(prefix="mangrs-eval-selftest-") as tmp:
        garbage = Path(tmp) / "garbage.bin"
        garbage.write_bytes(b"\x00" * 100)
        try:
            Model.open(garbage)
        except EvalError:
            print("selftest: garbage model rejected with a clean error")
        else:
            return _fail("selftest: garbage model parsed without error")

    path = model_path or _default_model()
    if not path.exists():
        return _fail(f"selftest: sample model not found at {path} "
                     f"(rebuild it with scripts/build-predict-model.py --sample)")
    try:
        model = Model.open(path)
    except EvalError as exc:
        return _fail(f"selftest: sample model does not parse: {exc}")
    ids = {s: model.id_of(s) for s in ("sinh", "viên", "học", "hello", "world")}
    if any(v is None for v in ids.values()):
        return _fail("selftest: sample model lacks expected fixture rows")
    for label, ok in (
        ("next(sinh) ranks viên first", rank_next_1tok(model, ids["sinh"], ids["viên"]) == 1),
        ("next(sinh, viên) ranks học first", rank_next_2tok(model, ids["sinh"], ids["viên"], ids["học"]) == 1),
        ("next(hello) ranks world first", rank_next_1tok(model, ids["hello"], ids["world"]) == 1),
        ("prefixes si/sinh rank sinh vien first",
         rank_compound(model, "sinh", 2, "sinh viên") == 1 and rank_compound(model, "sinh", 3, "sinh viên") == 1),
    ):
        if not ok:
            return _fail(f"selftest: anchor failed: {label}")
    print("selftest: anchor rankings hold on the sample model")

    heldout = "\n".join(("sinh viên học", "sinh viên học sinh", "học sinh giỏi",
                         "hello world", "hello help me", "chúng ta là sinh viên")) + "\n"
    run1 = evaluate(model, [("inline-fixture.txt", heldout)])
    run2 = evaluate(model, [("inline-fixture.txt", heldout)])
    text1 = report("inline-fixture.txt", *run1, model)
    if text1 != report("inline-fixture.txt", *run2, model):
        return _fail("selftest: two runs produced different reports")
    for tag in ("ctx1", "ctx2_full", "comp2", "comp3"):
        if run1[1][tag].n == 0:
            return _fail(f"selftest: no positions scored for {tag}")
    print(text1)
    print("selftest: metrics compute and are identical across two runs")
    print("PASS")
    return 0


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--model", type=Path, help="model binary (default: the committed data/model-sample.bin)")
    parser.add_argument("--held-out-dir", type=Path, help="directory of held-out/<corpus>.txt files")
    parser.add_argument("--selftest", action="store_true", help="run against the sample model with inline fixtures")
    args = parser.parse_args(argv)

    if args.selftest:
        return selftest(args.model)
    if args.held_out_dir is None:
        return _fail("--held-out-dir is required (or run with --selftest)")
    model_path = args.model or _default_model()
    try:
        model = Model.open(model_path)
        per_file, overall = evaluate(model, read_held_out(args.held_out_dir))
    except EvalError as exc:
        return _fail(str(exc))
    print(report(model_path, per_file, overall, model))
    return 0


if __name__ == "__main__":
    sys.exit(main())
