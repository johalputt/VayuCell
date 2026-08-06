#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# The constitution may not claim enforcement it does not have.
#
# GOVERNANCE-CONSTITUTION.md marks every rule [CI], [REVIEW] or [NORM], and §0.3
# says plainly that [CI] means a gate fails the build. That is the load-bearing
# claim of the whole document: it is what lets a reader tell which rules are real
# and which are aspiration.
#
# When this gate was first written, 43 of the 50 [CI] rules named nothing at all.
# The count in Appendix A was correct and the claim behind it was unverifiable —
# a reader had no way to tell whether "a gate fails the build" was true of any
# particular rule, and neither did the maintainer.
#
# So every [CI] rule now cites the artefact that enforces it, and this gate
# checks that the artefact exists. A rule whose enforcer is deleted stops
# building, which is the only way the classification stays honest as the project
# moves.
#
# What this gate does NOT claim: that the cited file genuinely enforces the rule
# it is attached to. No script can read a sentence and confirm that. That link is
# human review, and it is listed as such here rather than implied to be covered.
#
# Usage: scripts/constitution-gate.sh
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

DOC=GOVERNANCE-CONSTITUTION.md

if [ ! -s "$DOC" ]; then
  echo "  FAIL  $DOC is missing or empty"
  exit 1
fi

echo "Constitution gate — every [CI] rule must name a real enforcer"
echo

python3 - "$DOC" <<'PY'
import pathlib
import re
import sys

doc = pathlib.Path(sys.argv[1])
text = doc.read_text()

# Split into rules: a heading ending in its enforcement marker, then its body.
parts = re.split(r"^(#{2,3} .*?\*\*\[(?:CI|REVIEW|NORM)\]\*\*)\s*$", text, flags=re.M)

rules = []
for i in range(1, len(parts), 2):
    head, body = parts[i], parts[i + 1]
    tag = re.search(r"\[(CI|REVIEW|NORM)\]", head).group(1)
    title = head.lstrip("#").strip().replace("**[%s]**" % tag, "").strip()
    rules.append((tag, title, body))

if not rules:
    print("  FAIL        no classified rules found; the heading format has changed")
    sys.exit(1)

ci = [r for r in rules if r[0] == "CI"]
bad = 0
missing_citation = []
missing_file = []

for _, title, body in ci:
    m = re.search(r"\*\*Enforced by:\*\*\s*`([^`]+)`", body)
    if not m:
        missing_citation.append(title)
        bad = 1
        continue
    target = pathlib.Path(m.group(1))
    if not target.exists():
        missing_file.append((title, str(target)))
        bad = 1

if missing_citation:
    print(f"  FAIL        {len(missing_citation)} [CI] rule(s) name no enforcer:")
    for t in missing_citation:
        print(f"                {t}")
    print("              A rule marked [CI] claims a gate fails the build for it.")
    print("              Name the gate, or reclassify the rule as [REVIEW].")

if missing_file:
    print(f"  FAIL        {len(missing_file)} [CI] rule(s) cite a file that does not exist:")
    for t, f in missing_file:
        print(f"                {t}  ->  {f}")
    print("              The enforcer was deleted or moved. The rule is now")
    print("              unenforced and the document is overstating itself.")

if not bad:
    enforcers = sorted({
        re.search(r"\*\*Enforced by:\*\*\s*`([^`]+)`", b).group(1) for _, _, b in ci
    })
    print(f"  ok          all {len(ci)} [CI] rules cite an enforcer that exists")
    print(f"  ok          {len(enforcers)} distinct enforcers:")
    for e in enforcers:
        print(f"                {e}")

# Not checkable by any script, and said out loud rather than left implied.
print()
print("  --          that each cited file genuinely enforces the rule attached to")
print("  --          it is human review; no script can read a sentence and confirm it")

sys.exit(bad)
PY
rc=$?

echo
if [ "$rc" -ne 0 ]; then
  echo "CONSTITUTION GATE FAILED."
  exit 1
fi
echo "Constitution gate passed."
