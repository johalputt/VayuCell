#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Charter gate: the constitution, enforced by a machine.
#
# CHARTER.md is the document this project may not violate. A constitution that
# only a reviewer checks is a constitution that erodes on the busy week. Every
# article below that CAN be checked mechanically IS checked mechanically, and the
# ones that cannot are named here as uncheckable rather than quietly dropped —
# the same rule Article IV applies to device capabilities, applied to ourselves.
#
# Usage: scripts/charter-gate.sh
set -uo pipefail

# Without -e a failed cd would leave the gate running against whatever
# directory it was invoked from, where empty file lists make several checks
# pass trivially. That is a false green, so it exits instead.
cd "$(dirname "$0")/.." || exit 1

FAILED=0
pass() { printf '  ok    %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1"; FAILED=1; }
note() { printf '  --    %s\n' "$1"; }

# Source files, excluding tests, where the production rules must hold.
prod_sources() { find core/src -name '*.rs' ! -name '*_test.rs' -print; }
all_sources() { find core/src -name '*.rs' -print; }

echo "Charter gate — CHARTER.md enforced mechanically"
echo

# ── Article III — Safety of persons comes first ───────────────────────────────
echo "Article III — safety of persons"

# III.1: no capability that serves traffic may ship before the battery governor.
# The mechanical form: if any production source REGISTERS a Serving-class
# capability, the governor module must exist. Until it does, shipping one is a
# constitutional violation and not merely a sequencing preference.
#
# The pattern is the struct field `class: Class::Serving`, not the bare variant.
# Matching the variant alone flags capability.rs for *defining* Class::Serving
# and for naming it in a Display impl — a gate that fires on the type's own
# definition is noise, and a gate that cries wolf gets switched off.
serving_hits="$(prod_sources | xargs grep -l 'class:\s*Class::Serving' 2>/dev/null || true)"
if [ -n "$serving_hits" ] && [ ! -f core/src/governor.rs ]; then
  fail "III.1 a Serving-class capability exists before core/src/governor.rs:"
  printf '        %s\n' $serving_hits
elif [ -n "$serving_hits" ]; then
  pass "III.1 serving capabilities exist and the governor is present"
else
  pass "III.1 no serving capability has shipped yet"
fi

# III.3: never imply a safety property that is not read back from the hardware.
# The registry enforces this in the type system: `verify` is not an Option, so a
# capability with no read-back does not compile. If that field ever becomes
# optional the guarantee is gone, silently.
if grep -q 'pub verify: VerifyFn,' core/src/capability.rs; then
  pass "III.3 verify is mandatory in the type, not merely by convention"
else
  fail "III.3 Capability::verify is no longer a non-optional VerifyFn — a control with no read-back would compile"
fi

# The compile_fail proofs must actually be collected by rustdoc, which only
# happens on a public item. A proof on a private item runs zero tests and
# reports success.
if grep -q 'compile_fail' core/src/capability.rs; then
  pass "III.3 compile_fail proofs are present on the public module"
else
  fail "III.3 the compile_fail proofs for the registry have disappeared"
fi

# ── Article IV — Honest reporting ─────────────────────────────────────────────
echo
echo "Article IV — honest reporting"

# IV.2 and IV.3: absent, unverified and present are three distinct answers.
# A result type that offers a generic success variant invites "not checked" to
# be recorded as "fine".
if grep -q 'Unverified,' core/src/capability.rs && grep -q 'Absent,' core/src/capability.rs; then
  pass "IV.2/IV.3 Result_ distinguishes Absent from Unverified"
else
  fail "IV.2/IV.3 Result_ no longer distinguishes absent from unverified"
fi

for banned in 'Ok,' 'Pass,' 'Clean,' 'Fine,' 'Good,'; do
  if grep -qE "^\s+${banned}" core/src/capability.rs; then
    fail "IV.1 Result_ gained a generic success variant (${banned%,}) that would absorb 'not checked'"
  fi
done
pass "IV.1 no generic success variant absorbs unchecked results"

# IV.5: a control that cannot be read back may not be reported. Same field as
# III.3, checked above.

# The tier layer must have no "assume the lowest tier" fallback, which is the
# same defect: a device nothing recognised reported as a device we understand.
if grep -q 'Unknown,' core/src/tier.rs; then
  pass "IV.3 tier detection has an Unknown verdict rather than a default tier"
else
  fail "IV.3 tier detection lost its Unknown verdict"
fi

# ── Article V — What VayuCell will never contain ──────────────────────────────
echo
echo "Article V — what this project will never contain"

# V.1 no token, treasury, fee, mandatory account. V.2 no identifying telemetry.
# V.3 no unseverable remote control. These are checked as forbidden concepts in
# production source. The words are allowed to appear in prose that FORBIDS them,
# which is why only code is scanned.
declare -a FORBIDDEN=(
  'treasury:V.1 no treasury'
  'airdrop:V.1 no token distribution'
  'subscription:V.1 no fee'
  'billing:V.1 no fee'
  'license_key:V.1 no mandatory account'
  'licence_key:V.1 no mandatory account'
  'telemetry:V.2 no telemetry'
  'phone_home:V.2 no call-home'
  'beacon_url:V.2 no beacon'
  'device_fingerprint:V.2 no device identification'
  'remote_command:V.3 no remote control path'
  'remote_wipe:V.3 no remote control path'
  'kill_switch:V.3 no remote control path'
)
#
# Comment lines are stripped before the scan. This is not a loophole, it is the
# rule stated above being made true: ADR-0006's module documentation explains
# that a violation-report collector "would be exactly the telemetry Article V.2
# forbids", and flagging that sentence as telemetry is the gate misreading prose
# that FORBIDS the thing as the thing itself. A gate that punishes documenting a
# constraint teaches contributors to stop documenting constraints.
#
# ONLY whole-line comments are stripped, and the anchor matters. Cutting at any
# '//' also truncates 'https://…' — which silently erased the very URL the V.5
# home-call check exists to find, and the gate went green with the violation
# present. The gate self-test is what caught it.
#
# A trailing comment on a line of code is therefore still scanned. That is the
# right side to err on: a line with code on it is where a real identifier hides.
strip_comments() { sed -E 's:^[[:space:]]*//.*$::' "$1"; }

v_clean=1
for entry in "${FORBIDDEN[@]}"; do
  pattern="${entry%%:*}"
  why="${entry#*:}"
  hits=""
  while IFS= read -r f; do
    if strip_comments "$f" | grep -q "$pattern"; then
      hits="$hits $f"
    fi
  done < <(prod_sources)
  if [ -n "$hits" ]; then
    fail "$why — '$pattern' appears in:"
    printf '        %s\n' $hits
    v_clean=0
  fi
done
[ "$v_clean" = "1" ] && pass "V.1/V.2/V.3 no forbidden concept appears in production source"

# V.5: no dependency on a service the project controls. The strongest available
# mechanical form is that the core has no third-party runtime dependencies at
# all, which ADR-0005 §5.1 already requires. A vendored crate could later carry a
# network call nobody reviewed; zero dependencies means zero to review.
deps="$(sed -n '/^\[dependencies\]/,/^\[/p' core/Cargo.toml | grep -vE '^\[|^\s*#|^\s*$' || true)"
if [ -z "$deps" ]; then
  pass "V.5 the core has no third-party runtime dependencies"
else
  fail "V.5 the core gained runtime dependencies without an ADR admitting them:"
  printf '        %s\n' "$deps"
fi

# V.5 again: nothing in production source may reach out to a host this project
# operates. An installed cell whose owner never contacts the project again must
# keep working.
homecall=""
while IFS= read -r f; do
  if strip_comments "$f" | grep -qE 'https?://[a-z0-9.-]*(vayucell|johal\.in|vayupress)'; then
    homecall="$homecall $f"
  fi
done < <(prod_sources)
if [ -n "$homecall" ]; then
  fail "V.5 production source references a project-operated host:"
  printf '        %s\n' $homecall
else
  pass "V.5 no production source reaches a project-operated host"
fi

# ── Article VI — Licensing ────────────────────────────────────────────────────
echo
echo "Article VI — licensing"

grep -q 'Apache License' LICENSE \
  && pass "VI code licence is Apache-2.0" \
  || fail "VI LICENSE is not the Apache License"

grep -qi 'CC0' LICENSE-CHARTER \
  && pass "VI charter licence is CC0" \
  || fail "VI LICENSE-CHARTER is not CC0"

missing_spdx=""
while IFS= read -r f; do
  head -n 3 "$f" | grep -q 'SPDX-License-Identifier: Apache-2.0' || missing_spdx="$missing_spdx $f"
done < <(all_sources)
while IFS= read -r f; do
  head -n 3 "$f" | grep -q 'SPDX-License-Identifier: Apache-2.0' || missing_spdx="$missing_spdx $f"
done < <(find scripts -name '*.sh' -print)

if [ -n "$missing_spdx" ]; then
  fail "VI files without an SPDX header:"
  printf '        %s\n' $missing_spdx
else
  pass "VI every source and script file carries an SPDX header"
fi

# ── Article VII — Governance, and resistance to capture ───────────────────────
echo
echo "Article VII — governance"

if [ -f CLA.md ] || [ -f cla.md ] || grep -rqi 'contributor license agreement' CONTRIBUTING.md 2>/dev/null; then
  fail "VII a contributor licence agreement appeared; the charter requires DCO instead"
else
  pass "VII no contributor licence agreement"
fi

grep -qi 'developer certificate of origin\|DCO\|Signed-off-by' CONTRIBUTING.md \
  && pass "VII CONTRIBUTING documents the DCO" \
  || fail "VII CONTRIBUTING does not document the DCO"

# ── Article IX — Amendment ────────────────────────────────────────────────────
echo
echo "Article IX — Articles III and V may not be weakened"

# Article IX puts Articles III and V beyond ordinary amendment. A gate cannot
# judge whether an edit weakens them — that is a human question — but it CAN
# refuse to let them change unnoticed. The recorded digest must be updated in the
# same commit that edits the article, which makes the change visible in review
# instead of arriving inside an unrelated diff.
extract_article() {
  awk -v want="## Article $1 " '
    $0 ~ "^## Article " { inside = (index($0, want) == 1) }
    inside { print }
  ' CHARTER.md
}

if [ ! -f .charter-digests ]; then
  fail "IX .charter-digests is missing; regenerate with scripts/charter-gate.sh --record"
else
  for art in III V; do
    actual="$(extract_article "$art" | sha256sum | cut -d' ' -f1)"
    expected="$(grep "^ARTICLE_${art}=" .charter-digests | cut -d= -f2)"
    if [ -z "$expected" ]; then
      fail "IX no recorded digest for Article $art"
    elif [ "$actual" != "$expected" ]; then
      fail "IX Article $art has changed."
      note "recorded $expected"
      note "actual   $actual"
      note "Article IX places this article beyond ordinary amendment. If the change"
      note "is genuinely intended, run scripts/charter-gate.sh --record and explain"
      note "the amendment in the commit message, so review sees it deliberately."
    else
      pass "IX Article $art is unchanged"
    fi
  done
fi

if [ "${1:-}" = "--record" ]; then
  {
    echo "# SHA-256 of the charter articles Article IX places beyond ordinary"
    echo "# amendment. Regenerated only by scripts/charter-gate.sh --record."
    for art in III V; do
      echo "ARTICLE_${art}=$(extract_article "$art" | sha256sum | cut -d' ' -f1)"
    done
  } > .charter-digests
  echo
  echo "Recorded charter digests to .charter-digests"
  exit 0
fi

# ── What this gate cannot check ───────────────────────────────────────────────
echo
echo "Not mechanically checkable — these remain human review"
note "III.2 that the plain-language warning is genuinely understandable"
note "III.4 that physical inspection is named as the definitive check in the UI"
note "IV.4 that permanent failing rows read as permanent to an actual operator"
note "V.4 that no capability exists only in a hosted edition"

echo
if [ "$FAILED" -ne 0 ]; then
  echo "CHARTER GATE FAILED — see the FAIL lines above."
  exit 1
fi
echo "Charter gate passed."
