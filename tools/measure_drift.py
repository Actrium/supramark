#!/usr/bin/env python3
"""For each (actual, expected) SVG pair, compute per-token max numeric
drift after applying the same canonicalization the test uses."""
import re
import os

ACTUALS_DIR = "/tmp/drift_actuals"
ROOT = "/ext/plantuml/plantuml-little"

def load_index():
    m = {}
    with open(f"{ROOT}/tests/reference/INDEX.tsv") as f:
        for line in f:
            line = line.rstrip("\n")
            if not line: continue
            fix, ref = line.split("\t")
            m[fix] = ref
    return m

NUM_RE = re.compile(r'-?\d+\.?\d*')
PI_RE = re.compile(r'<\?plantuml-src [^?]*\?>')
DATA_NV_RE = re.compile(r' data-(?:source-line|entity-[12])="[^"]*"')
SPACES_RE = re.compile(r' {2,}')
ARROW_RE = re.compile(r'<polygon fill="([^"]*)" points="[^"]*" style="[^"]*"/>')
RANDPX_RE = re.compile(r'<rect fill="#[0-9A-Fa-f]{6}" height="1" style="stroke:#[0-9A-Fa-f]{6};stroke-width:1;" width="1" x="0" y="0"/>')
PNG_RE = re.compile(r'data:image/png;base64,[A-Za-z0-9+/=]+')
SVG_RE = re.compile(r'data:image/svg\+xml;base64,[A-Za-z0-9+/=]+')
FILTER_ID_RE = re.compile(r'<(?:filter|linearGradient|radialGradient) [^>]*id="([^"]+)"')

ERROR_KEYWORDS = ("Syntax Error?", "Fatal crash error:", "Welcome to PlantUML",
                  "You should send a mail to plantuml@gmail.com")

def norm(s: str) -> str:
    s = PI_RE.sub("", s)
    s = DATA_NV_RE.sub("", s)
    s = SPACES_RE.sub(" ", s)
    mappings = {}
    counter = 0
    for m in FILTER_ID_RE.finditer(s):
        old = m.group(1)
        if old not in mappings:
            mappings[old] = f"__f{counter}__"
            counter += 1
    for old, new in mappings.items():
        s = s.replace(f'id="{old}"', f'id="{new}"')
        s = s.replace(f"url(#{old})", f"url(#{new})")
    ent_map = {}
    for m in re.finditer(r'id="(ent\w+)"', s):
        if m.group(1) not in ent_map:
            ent_map[m.group(1)] = f"__e{len(ent_map)}__"
    for old, new in ent_map.items():
        s = s.replace(f'id="{old}"', f'id="{new}"')
        s = s.replace(f'data-entity-1="{old}"', f'data-entity-1="{new}"')
        s = s.replace(f'data-entity-2="{old}"', f'data-entity-2="{new}"')
    lnk_map = {}
    for m in re.finditer(r'id="(lnk\w+)"', s):
        if m.group(1) not in lnk_map:
            lnk_map[m.group(1)] = f"__l{len(lnk_map)}__"
    for old, new in lnk_map.items():
        s = s.replace(f'id="{old}"', f'id="{new}"')
    s = ARROW_RE.sub(r'<polygon fill="\1"/>', s)
    if any(k in s for k in ERROR_KEYWORDS):
        s = RANDPX_RE.sub("", s)
    s = PNG_RE.sub("PNG_DATA", s)
    s = SVG_RE.sub("SVG_DATA", s)
    return s

def _num_at_or_before(s, pos):
    if pos >= len(s): return None
    start = pos
    while start > 0 and (s[start-1].isdigit() or s[start-1] in '.-'):
        start -= 1
    m = NUM_RE.match(s, start)
    if m and m.end() > pos:
        return (m.start(), m.end(), m.group())
    m2 = NUM_RE.match(s, pos)
    if m2:
        return (m2.start(), m2.end(), m2.group())
    return None

def per_fixture_max_drift(actual: str, expected: str):
    a = norm(actual); e = norm(expected)
    if a == e: return (0.0, 0, False)
    diffs = []
    i = j = 0
    structural = False
    skips = 0
    while i < len(a) and j < len(e):
        if a[i] == e[j]:
            i += 1; j += 1; continue
        ma = _num_at_or_before(a, i); me = _num_at_or_before(e, j)
        if ma and me:
            try:
                diff = abs(float(ma[2]) - float(me[2]))
                diffs.append(diff)
                i = max(ma[1], i + 1); j = max(me[1], j + 1)
                skips += 1
                if skips > 1500: structural = True; break
                continue
            except ValueError: pass
        structural = True; break
    if not diffs: return (0.0, 0, structural)
    return (max(diffs), len(diffs), structural)

def main():
    index = load_index()
    results = []
    for actual_file in sorted(os.listdir(ACTUALS_DIR)):
        fixture = actual_file.replace("__", "/")
        if not fixture.startswith("tests/"): continue
        rel_fix = fixture[len("tests/"):]
        direct = rel_fix.replace("fixtures/", "reference/").replace(".puml", ".svg")
        ref_path = f"{ROOT}/tests/{direct}"
        if not os.path.exists(ref_path):
            mapped = index.get(rel_fix)
            if not mapped: continue
            ref_path = f"{ROOT}/tests/{mapped}"
        if not os.path.exists(ref_path): continue
        try:
            with open(ref_path, "rb") as f: expected = f.read()
            if expected[:4] == b"\x89PNG": continue
            expected = expected.decode("utf-8", errors="replace")
        except Exception: continue
        with open(f"{ACTUALS_DIR}/{actual_file}") as f:
            actual = f.read()
        mx, cnt, struc = per_fixture_max_drift(actual, expected)
        if mx == 0.0 and cnt == 0: continue
        results.append((mx, cnt, struc, fixture))

    results.sort(reverse=True)
    buckets = {'<0.001 (FP rounding)': 0, '0.001-0.01': 0, '0.01-0.1': 0,
               '0.1-0.5': 0, '0.5-1': 0, '1-2.51 (fuzzy edge)': 0,
               '>2.51 (known_failure)': 0}
    print(f"{'max_drift':>10} {'#tokens':>8} {'struct':>6}  fixture")
    print("-" * 90)
    for mx, cnt, struc, fx in results:
        if struc:
            bucket = '>2.51 (known_failure)' if mx > 2.51 else '1-2.51 (fuzzy edge)'
        elif mx > 2.51: bucket = '>2.51 (known_failure)'
        elif mx >= 1: bucket = '1-2.51 (fuzzy edge)'
        elif mx >= 0.5: bucket = '0.5-1'
        elif mx >= 0.1: bucket = '0.1-0.5'
        elif mx >= 0.01: bucket = '0.01-0.1'
        elif mx >= 0.001: bucket = '0.001-0.01'
        else: bucket = '<0.001 (FP rounding)'
        buckets[bucket] += 1
        flag = "STRUC" if struc else ""
        print(f"{mx:10.4f} {cnt:8d} {flag:>6}  {fx}")
    print()
    print("=== Drift distribution ===")
    for k, v in buckets.items():
        if v > 0: print(f"  {k:25s} {v:4d} fixtures")
    print(f"\nTotal fixtures with any divergence: {len(results)}")
    print(f"Total reference fixtures with refs:  ~341")
    pct = len(results) * 100.0 / 341
    print(f"Divergence rate:                     {pct:.1f}%")

if __name__ == "__main__":
    main()
