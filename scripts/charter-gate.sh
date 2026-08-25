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
#
# EVERY crate, not just core/. This scanned core/src alone, so Article V's
# forbidden concepts and the V.5 home-call check never looked at cli/src — the
# only crate that opens a socket. The rule "nothing may reach a host this
# project operates" was being enforced exclusively on the crate that cannot
# reach anything.
#
# The V.5 *dependency* check in this same file already carries the note that a
# gate naming one manifest by hand "goes on passing while a dependency lands in
# the crate beside it". That half learned the lesson; this half had not.
#
# Crates are found rather than listed, for the same reason.
prod_sources() {
  find . -path ./target -prune -o -path ./fuzz -prune -o \
    -name '*.rs' ! -name '*_test.rs' -print
}
all_sources() {
  find . -path ./target -prune -o -path ./fuzz -prune -o -name '*.rs' -print
}

# Production code is everything outside a `#[cfg(test)]` item.
#
# `*_test.rs` is this repo's convention and it is not the only one: cli/src has
# inline `#[cfg(test)] mod` blocks, and a scan that missed them would report the
# test module's own TcpStream::connect as production egress. Braces are matched
# rather than cutting at the first marker, because cutting there would silently
# skip any production code that followed one — a false pass, which is what every
# gate here exists to prevent.
strip_test_items() {
  python3 - "$1" <<'PYEOF'
import sys
src = open(sys.argv[1], encoding="utf-8", errors="replace").read()
out, i = [], 0
while True:
    at = src.find("#[cfg(test)]", i)
    if at == -1:
        out.append(src[i:])
        break
    out.append(src[i:at])
    brace = src.find("{", at)
    if brace == -1:
        break
    depth, j = 0, brace
    while j < len(src):
        if src[j] == "{":
            depth += 1
        elif src[j] == "}":
            depth -= 1
            if depth == 0:
                break
        j += 1
    i = j + 1
sys.stdout.write("".join(out))
PYEOF
}

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
strip_comments_stdin() { sed -E 's:^[[:space:]]*//.*$::'; }

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
# EVERY crate is checked, not just core/. The binary was added after this gate
# was written, and a gate that names one manifest by hand goes on passing while a
# dependency lands in the crate beside it — which is precisely the silent pass
# every gate here exists to catch. A path dependency on another crate in this
# workspace is not third-party and is allowed; anything else is not.
# The fuzz harness is the ONE exemption, and it is verified rather than trusted.
# It carries libfuzzer-sys, and it is allowed to only because nothing it touches
# ships: it is excluded from the workspace, so no `cargo build` of the binary
# can reach it. An exemption that merely asserted that would be a hole with a
# comment over it, so the exclusion is checked here — and if the fuzz crate ever
# rejoins the workspace, this fails before its dependency does.
if [ -d fuzz ]; then
  if grep -qE '^\s*exclude\s*=\s*\[[^]]*"fuzz"' Cargo.toml; then
    pass "V.5 the fuzz harness is excluded from the workspace, so its dependency never ships"
  else
    fail "V.5 fuzz/ exists but is not excluded in the root Cargo.toml, so its third-party dependency is reachable from a build of the binary"
  fi
fi

deps=""
while IFS= read -r manifest; do
  found="$(sed -n '/^\[dependencies\]/,/^\[/p' "$manifest" \
    | grep -vE '^\[|^\s*#|^\s*$' \
    | grep -v 'path *= *"' || true)"
  [ -n "$found" ] && deps="$deps$manifest: $found"$'\n'
done < <(find . -name Cargo.toml -not -path './target/*' -not -path './.git/*' -not -path './fuzz/*' | sort)

if [ -z "$deps" ]; then
  pass "V.5 no crate has a third-party runtime dependency"
else
  fail "V.5 a crate gained runtime dependencies without an ADR admitting them:"
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

# V.2, mechanically: nothing in production source may open an OUTBOUND
# connection at all.
#
# The check above looks for a project-operated hostname, which only catches a
# call-home somebody was honest enough to write a URL for. This is the general
# form, and it is the mechanism behind a sentence `vayucell report` prints to
# every operator: *nothing in this binary dials out.*
#
# That sentence used to read "this program has no network code", which is false
# — the binary runs three HTTP listeners, and an operator who has run
# `vayucell site` can check it and find it wrong. The true claim is narrower and
# stronger: it binds, and it never connects. A claim that reassuring needs
# something enforcing it, or the next person to add an update check makes it
# false without noticing.
#
# `bind` is deliberately not forbidden. Listening is what the surfaces do.
#
# The scan itself runs through python3, which strips the test items first. A
# machine where that interpreter is missing or broken — a Store stub on PATH,
# a stripped container — made every file strip to NOTHING, the grep match
# nothing, and this check pass while reading no source at all. It passed here
# for exactly that reason while failing in CI, which is the quietest way a
# gate can rot: green everywhere it runs, true nowhere. So the interpreter is
# proven usable before any verdict downstream of it is allowed to mean
# anything.
if ! python3 -c 'pass' >/dev/null 2>&1; then
  fail "V.2 python3 is unusable on PATH, so the outbound-connection scan would read nothing and pass"
else
  # The companion, and the one carve-out. `sync/` is `vayucell-sync`: a command
  # for the machine that HOLDS the files, whose entire purpose is dialling the
  # cell you name. The cell itself — core/ and cli/ — remains scanned in full,
  # because "it binds, and it never connects" is a claim about what runs on the
  # phone, not about every binary this repository builds. The carve-out is
  # guarded rather than assumed: if sync/ vanished while this exclusion stayed,
  # an unwatched directory would exist only in this comment.
  if [ ! -d sync/src ]; then
    fail "V.2 the scan excludes sync/, which no longer exists — remove the exclusion or restore the crate"
  fi
  egress=""
  while IFS= read -r f; do
    case "$f" in
      ./sync/*) continue ;;
    esac
    if strip_test_items "$f" | strip_comments_stdin \
        | grep -qE '(TcpStream|UdpSocket)::(connect|bind|connect_timeout)|reqwest|ureq|\bcurl\b'; then
      egress="$egress $f"
    fi
  done < <(prod_sources)
  if [ -n "$egress" ]; then
    fail "V.2 production source opens an outbound connection; a cell must not dial out:"
    printf '        %s\n' $egress
  else
    pass "V.2 no production source outside the sync companion opens an outbound connection"
  fi
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
