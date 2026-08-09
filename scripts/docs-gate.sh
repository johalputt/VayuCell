#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Documentation gate.
#
# The founding documents are load-bearing here in a way they are not on most
# projects: CHARTER.md is what the code may not violate, and the ADRs are where
# the safety reasoning lives. A dead link into that set is not cosmetic — it is a
# reader who cannot reach the argument they were sent to check.
#
# Usage: scripts/docs-gate.sh
set -uo pipefail

# Without -e a failed cd would leave the gate running against whatever
# directory it was invoked from, where empty file lists make several checks
# pass trivially. That is a false green, so it exits instead.
cd "$(dirname "$0")/.." || exit 1

FAILED=0
pass() { printf '  ok    %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1"; FAILED=1; }

echo "Documentation gate"
echo

# ── Required documents ────────────────────────────────────────────────────────
REQUIRED=(
  README.md CHARTER.md GOVERNANCE-CONSTITUTION.md PLAN.md
  GOVERNANCE.md CONTRIBUTING.md
  SECURITY.md TRADEMARK.md NOTICE LICENSE LICENSE-CHARTER
  CHANGELOG.md .release-version
  docs/CI.md docs/BRAND.md
  hardware/schema.json
)
missing=""
for f in "${REQUIRED[@]}"; do
  [ -s "$f" ] || missing="$missing $f"
done
if [ -n "$missing" ]; then
  fail "required documents missing or empty:"
  printf '        %s\n' $missing
else
  pass "all ${#REQUIRED[@]} required documents present and non-empty"
fi

# ── ADR numbering, naming and titles agree ────────────────────────────────────
python3 - <<'PY'
import re, sys, pathlib

adrs = sorted(pathlib.Path("docs/adr").glob("ADR-*.md"))
bad = 0

if not adrs:
    print("  FAIL  no ADRs found in docs/adr/")
    sys.exit(1)

numbers = []
for p in adrs:
    m = re.match(r"ADR-(\d{4})-[a-z0-9-]+\.md$", p.name)
    if not m:
        print(f"  FAIL  filename is not ADR-NNNN-kebab-slug.md: {p.name}")
        bad = 1
        continue
    n = int(m.group(1))
    numbers.append(n)

    first = p.read_text().splitlines()[0]
    # The title must name the same number as the filename. A mismatch sends a
    # reader following a cross-reference to the wrong decision.
    tm = re.match(r"# ADR-(\d{4}) — .+", first)
    if not tm:
        print(f"  FAIL  {p.name}: first line is not '# ADR-NNNN — Title', got {first!r}")
        bad = 1
    elif int(tm.group(1)) != n:
        print(f"  FAIL  {p.name}: title says ADR-{tm.group(1)} but the filename says ADR-{m.group(1)}")
        bad = 1

# Contiguity from 0001. A gap means an ADR was deleted rather than superseded,
# and a superseded decision must stay readable — that is the whole point of
# keeping a decision log instead of a wiki.
expected = list(range(1, len(numbers) + 1))
if sorted(numbers) != expected:
    print(f"  FAIL  ADR numbering is not contiguous from 0001: found {sorted(numbers)}")
    bad = 1

if not bad:
    print(f"  ok    {len(adrs)} ADRs: filenames, titles and numbering agree")
sys.exit(bad)
PY
[ $? -ne 0 ] && FAILED=1

# ── Every relative link in every Markdown file resolves ───────────────────────
python3 - <<'PY'
import re, sys, pathlib
from urllib.parse import unquote

LINK = re.compile(r"\[[^\]]*\]\(([^)]+)\)")
broken = []
checked = 0

for md in sorted(pathlib.Path(".").rglob("*.md")):
    if any(part in (".git", "target", "node_modules") for part in md.parts):
        continue
    for raw in LINK.findall(md.read_text()):
        link = raw.strip()
        # External links and anchors are out of scope: reaching the network from
        # a gate makes the build depend on someone else's uptime.
        if link.startswith(("http://", "https://", "mailto:", "#")):
            continue
        target = unquote(link.split("#", 1)[0])
        if not target:
            continue
        checked += 1
        resolved = (md.parent / target).resolve()
        if not resolved.exists():
            broken.append(f"{md}: {link}")

if broken:
    print(f"  FAIL  {len(broken)} broken relative link(s):")
    for b in broken:
        print(f"          {b}")
    sys.exit(1)
print(f"  ok    {checked} relative links across the documentation all resolve")
PY
[ $? -ne 0 ] && FAILED=1

# ── No orphan ADRs ────────────────────────────────────────────────────────────
# An ADR nobody links to is an ADR nobody reads. The index in README.md and
# PLAN.md is the entry point, so every ADR must be reachable from the prose.
python3 - <<'PY'
import pathlib, sys

adrs = {p.stem.split("-")[1] for p in pathlib.Path("docs/adr").glob("ADR-*.md")}
prose = ""
for f in ("README.md", "PLAN.md", "CHARTER.md", "docs/CI.md"):
    p = pathlib.Path(f)
    if p.exists():
        prose += p.read_text()
for p in pathlib.Path("docs/adr").glob("ADR-*.md"):
    prose += p.read_text()

orphans = sorted(n for n in adrs if f"ADR-{n}" not in prose.replace(f"# ADR-{n}", "", 1))
# An ADR naming only itself does not count as referenced; the replace above drops
# each ADR's own title line before looking.
truly = []
for n in adrs:
    others = ""
    for p in sorted(pathlib.Path(".").rglob("*.md")):
        if any(x in (".git", "target") for x in p.parts):
            continue
        if p.name.startswith(f"ADR-{n}-"):
            continue
        others += p.read_text()
    if f"ADR-{n}" not in others:
        truly.append(n)

if truly:
    print(f"  FAIL  ADR(s) nothing links to: {', '.join('ADR-' + n for n in sorted(truly))}")
    sys.exit(1)
print(f"  ok    every ADR is referenced from outside itself")
PY
[ $? -ne 0 ] && FAILED=1

# ── The constitution may not lie about how much of itself is enforced ─────────
# GOVERNANCE-CONSTITUTION.md classifies every rule as [CI], [REVIEW] or [NORM],
# and Appendix A totals them. Those totals were wrong in the document's first
# draft — off by six — and nothing would ever have noticed. A governance document
# that overstates how much of itself a machine actually enforces is committing
# the error Article 4 forbids in a device report, against the reader of the
# governance instead. So the table is a gate.
python3 - <<'INNER'
import pathlib, re, sys
from collections import Counter

p = pathlib.Path("GOVERNANCE-CONSTITUTION.md")
if not p.exists():
    print("  FAIL  GOVERNANCE-CONSTITUTION.md is missing")
    sys.exit(1)
s = p.read_text()

# A rule is a heading that ends in its enforcement marker. Markers inside tables
# and prose are explanation, not classification, and must not be counted.
actual = Counter(re.findall(r"^#{2,3} .*?\*\*\[(CI|REVIEW|NORM)\]\*\*\s*$", s, re.M))

claimed = {}
for tag in ("CI", "REVIEW", "NORM"):
    m = re.search(rf"\|\s*\*\*\[{tag}\]\*\*\s*\|\s*(\d+)\s*\|", s)
    if not m:
        print(f"  FAIL  Appendix A has no row for [{tag}]")
        sys.exit(1)
    claimed[tag] = int(m.group(1))

m = re.search(r"\|\s*\*\*Total\*\*\s*\|\s*(\d+)\s*\|", s)
if not m:
    print("  FAIL  Appendix A has no total row")
    sys.exit(1)
claimed_total = int(m.group(1))

bad = 0
for tag in ("CI", "REVIEW", "NORM"):
    if actual[tag] != claimed[tag]:
        print(f"  FAIL  Appendix A claims {claimed[tag]} [{tag}] rules; the document has {actual[tag]}")
        bad = 1
if claimed_total != sum(actual.values()):
    print(f"  FAIL  Appendix A totals {claimed_total}; the document has {sum(actual.values())}")
    bad = 1

if not bad:
    total = sum(actual.values())
    print(f"  ok    the constitution's {total} rules match Appendix A "
          f"({actual['CI']} CI, {actual['REVIEW']} review, {actual['NORM']} norm)")
sys.exit(bad)
INNER
[ $? -ne 0 ] && FAILED=1

# ── Every source path a document names must exist ─────────────────────────────
#
# The roadmap and the ADRs point at files to say *where a decision is
# implemented*: "the supervisor loop is in `core/src/runtime.rs`", "the types are
# in `core/src/durability.rs`". That is the sentence a reader follows to check
# whether a claim is real, and it is the sentence a rename quietly falsifies.
#
# A path that no longer exists does not make a document wrong in a visible way.
# It makes it wrong in the way this project spends most of its effort on: a claim
# that reads as verified and points at nothing. The roadmap in particular now
# carries a status per phase, and every ◐ leans on a path.
#
# Only paths inside backticks that look like project source are checked, so
# ordinary prose about a directory is not dragged in.
missing=""
while IFS= read -r path; do
  [ -e "$path" ] || missing="$missing$path"$'\n'
done < <(
  grep -rhoE '`(core|cli|scripts|docs)/[a-zA-Z0-9_./-]+\.(rs|sh|md|toml|json)`' \
    --include='*.md' . 2>/dev/null | tr -d '`' | sort -u
)
if [ -n "$missing" ]; then
  fail "a document names a source path that does not exist:"
  # Quoted, so a path prints as one line. Unquoted, the shell splits it into
  # words and the operator is shown fragments of the thing they have to fix.
  printf '        %s' "$missing"
  # And which document named it, since the point is to go and correct the claim.
  while IFS= read -r path; do
    [ -n "$path" ] && grep -rln --include='*.md' -F "$path" . | sed 's/^/          named in /'
  done <<< "$missing"
else
  pass "every source path named in a document exists"
fi

# ── The README's module count is the number of modules ────────────────────────
#
# "Sixteen modules in core/src" sat in the README while the crate had twenty. It
# is a small number in a sentence nobody re-reads, and that is exactly why it
# drifts: adding a module is the moment nobody thinks about prose.
#
# This project already pins its other counts mechanically — the constitution's
# rules against Appendix A, the doctests exactly in both directions — and this
# one had no check at all. Found while adding the twentieth module and writing
# the wrong number in its place.
declared="$(grep -oE '^(One|Two|Three|Four|Five|Six|Seven|Eight|Nine|Ten|Eleven|Twelve|Thirteen|Fourteen|Fifteen|Sixteen|Seventeen|Eighteen|Nineteen|Twenty|Twenty-one|Twenty-two|Twenty-three|Twenty-four|Twenty-five)( modules in)' README.md | head -1 | sed 's/ modules in//')"
actual="$(grep -c '^pub mod ' core/src/lib.rs)"
words="One Two Three Four Five Six Seven Eight Nine Ten Eleven Twelve Thirteen Fourteen Fifteen Sixteen Seventeen Eighteen Nineteen Twenty Twenty-one Twenty-two Twenty-three Twenty-four Twenty-five"
expected="$(echo "$words" | cut -d' ' -f"$actual")"
if [ -z "$declared" ]; then
  fail "the README no longer states how many modules core/src has"
elif [ "$declared" != "$expected" ]; then
  fail "README says '$declared modules in core/src'; lib.rs declares $actual ($expected)"
else
  pass "the README's module count matches lib.rs ($actual)"
fi

echo
if [ "$FAILED" -ne 0 ]; then
  echo "DOCUMENTATION GATE FAILED."
  exit 1
fi
echo "Documentation gate passed."
