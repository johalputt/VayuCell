#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Gate self-test: prove the gates actually fire.
#
# The charter and hardware gates are the machines that enforce CHARTER.md and the
# device database. A gate that has only ever been observed passing is a gate
# nobody has verified — it is indistinguishable from one whose patterns match
# nothing at all. This script plants each violation in a scratch copy of the
# repository and requires the matching gate to catch it, citing the right rule.
#
# The need is not hypothetical, and both failure directions were hit while
# writing these gates. The first III.1 check matched the bare variant
# `Class::Serving` and so flagged capability.rs for *defining* the enum: it
# failed loudly, which looked like working, and was wrong. The first hardware
# honesty check read `battery.charge_limit.verified`, a field the schema does not
# have, and printed ok forever while checking nothing. The second kind has no
# symptom. This script is how it gets caught.
#
# Usage: scripts/gate-selftest.sh
set -uo pipefail

# Without -e a failed cd would leave the gate running against whatever
# directory it was invoked from, where empty file lists make several checks
# pass trivially. That is a false green, so it exits instead.
cd "$(dirname "$0")/.." || exit 1
REPO="$PWD"

FAILED=0

CHARTER=scripts/charter-gate.sh
HARDWARE=scripts/hardware-gate.sh
DOCS=scripts/docs-gate.sh
RELEASE=scripts/release-gate.sh
ATTRIB=scripts/attribution-gate.sh

WORK="$(mktemp -d)"
trap 'rm -rf "$WORK"' EXIT

# violation <gate-script> <description> <expected-substring-of-failure> <setup…>
# Runs the setup in a pristine copy and requires the named gate to fail, citing
# the expected article or rule.
violation() {
  local gate="$1" desc="$2" expect="$3"; shift 3
  local sandbox="$WORK/case"

  rm -rf "$sandbox"
  mkdir -p "$sandbox"
  # Copy tracked content only; .git and target/ are large and irrelevant.
  tar -C "$REPO" --exclude=.git --exclude=target -cf - . | tar -C "$sandbox" -xf -

  # Setup is a single shell string per case, so "$*" is the intended form.
  # A fingerprint of the sandbox before and after the plant. A setup command
  # whose pattern has gone stale — a sed matching a number that has since
  # changed — edits nothing, the gate then passes for the honest reason, and the
  # case is scored MISSED against a gate that is working fine. Requiring the
  # plant to actually change something turns that into a loud, correct error.
  local before after
  before="$(cd "$sandbox" && find . -type f -exec sha256sum {} + | sort | sha256sum)"

  ( cd "$sandbox" && eval "$*" ) || { echo "  ERROR   setup failed for: $desc"; FAILED=1; return; }

  after="$(cd "$sandbox" && find . -type f -exec sha256sum {} + | sort | sha256sum)"
  if [ "$before" = "$after" ]; then
    echo "  STALE   $desc"
    echo "          the setup command changed nothing, so this case tests nothing."
    FAILED=1
    return
  fi

  local out
  out="$(cd "$sandbox" && bash "$gate" 2>&1)"
  local rc=$?

  if [ "$rc" -eq 0 ]; then
    echo "  MISSED  $desc"
    echo "          the gate passed while the violation was present."
    FAILED=1
  elif ! printf '%s' "$out" | grep -q "$expect"; then
    echo "  WRONG   $desc"
    echo "          the gate failed, but not for the expected reason."
    echo "          expected a failure mentioning: $expect"
    printf '%s\n' "$out" | grep '  FAIL' | sed 's/^/          got: /'
    FAILED=1
  else
    echo "  caught  $desc"
  fi
}

echo "Gate self-test — each violation planted, the matching gate must catch it"
echo

# Both baselines must be green, or every "caught" below could be the baseline
# failing rather than the planted violation.
for gate in "$CHARTER" "$HARDWARE" "$DOCS" "$ATTRIB" "$RELEASE"; do
  if ! bash "$gate" >/dev/null 2>&1; then
    echo "refusing to run: $gate is already failing on a clean tree, so a caught"
    echo "violation would not be evidence that the gate caught anything."
    exit 1
  fi
done

# The hardware gate reports UNVERIFIED rather than failing when the JSON Schema
# library is absent, so without it the schema cases below would be recorded as
# missed. Require the validator here for the same reason CI does.
if ! python3 -c "import jsonschema" 2>/dev/null; then
  echo "refusing to run: python3 -m pip install jsonschema"
  echo "without it the schema-validation cases cannot be self-tested."
  exit 1
fi
export VAYUCELL_REQUIRE_SCHEMA_VALIDATOR=1

violation "$CHARTER" "III.1 a serving capability registered before the governor exists" \
  "III.1" \
  "printf 'const _PLANTED: \&str = \"class: Class::Serving\";\n' >> core/src/capability.rs"

violation "$CHARTER" "III.3 verify becomes optional, so a control with no read-back compiles" \
  "III.3" \
  "sed -i 's/pub verify: VerifyFn,/pub verify: Option<VerifyFn>,/' core/src/capability.rs"

violation "$CHARTER" "III.3 the compile_fail proofs are deleted" \
  "III.3" \
  "sed -i 's/compile_fail/ignore/g' core/src/capability.rs"

violation "$CHARTER" "IV.2 absent and unverified collapse into one answer" \
  "IV.2" \
  "sed -i 's/^    Unverified,/    Whatever,/' core/src/capability.rs"

violation "$CHARTER" "IV.1 a generic success variant appears that would absorb 'not checked'" \
  "IV.1" \
  "sed -i 's/^    Absent,/    Absent,\n    Ok,/' core/src/capability.rs"

violation "$CHARTER" "IV.3 tier detection gains a default tier instead of Unknown" \
  "IV.3" \
  "sed -i 's/^    Unknown,/    AssumeT0,/' core/src/tier.rs"

violation "$CHARTER" "V.2 telemetry appears in production source" \
  "V.2" \
  "printf 'fn send_telemetry() {}\n' >> core/src/lib.rs"

violation "$CHARTER" "V.3 a remote control path appears in production source" \
  "V.3" \
  "printf 'const _K: &str = \"kill_switch\";\n' >> core/src/lib.rs"

violation "$CHARTER" "V.5 the core takes a third-party runtime dependency" \
  "V.5" \
  "sed -i 's/^\[dependencies\]/[dependencies]\nserde = \"1\"/' core/Cargo.toml"

violation "$CHARTER" "V.5 production source reaches a host the project operates" \
  "V.5" \
  "printf 'const _U: &str = \"https://api.vayucell.example\";\n' >> core/src/lib.rs"

violation "$CHARTER" "VI a source file loses its SPDX header" \
  "VI" \
  "sed -i '1d' core/src/tier.rs"

violation "$CHARTER" "VI the code licence stops being Apache-2.0" \
  "VI" \
  "printf 'Some other licence\n' > LICENSE"

violation "$CHARTER" "VII a contributor licence agreement appears" \
  "VII" \
  "printf '# CLA\n' > CLA.md"

violation "$CHARTER" "IX Article III is edited without recording the amendment" \
  "IX Article III has changed" \
  "sed -i 's/^1\. \*\*No capability that serves traffic.*/1. Capabilities may ship in any order./' CHARTER.md"

violation "$CHARTER" "IX Article V is edited without recording the amendment" \
  "IX Article V has changed" \
  "sed -i 's/^1\. \*\*No token, no treasury.*/1. A modest fee is permitted./' CHARTER.md"

violation "$CHARTER" "IX the recorded digests are simply deleted" \
  "IX" \
  "rm -f .charter-digests"

# ── Hardware database gate ────────────────────────────────────────────────────
DEV=hardware/devices/example-t2-pixel-class.json

violation "$HARDWARE" "a device profile stops being valid JSON" \
  "not valid JSON" \
  "printf '{oops' > $DEV"

violation "$HARDWARE" "a profile gains a field the schema does not allow" \
  "Additional properties are not allowed" \
  "python3 -c \"import json;d=json.load(open('$DEV'));d['marketing_score']=10;json.dump(d,open('$DEV','w'))\""

violation "$HARDWARE" "a profile drops a field the schema requires" \
  "'reported_at' is a required property" \
  "python3 -c \"import json;d=json.load(open('$DEV'));del d['reported_at'];json.dump(d,open('$DEV','w'))\""

violation "$HARDWARE" "a charge ceiling is verified to hold but names no node" \
  "node_path is empty" \
  "python3 -c \"import json;d=json.load(open('$DEV'));d['battery']['charge_control'].pop('node_path');json.dump(d,open('$DEV','w'))\""

violation "$HARDWARE" "a ceiling is verified to hold on a device with no mechanism" \
  "while available is not true" \
  "python3 -c \"import json;d=json.load(open('$DEV'));c=d['battery']['charge_control'];c['available']=False;c['mechanism']='none';json.dump(d,open('$DEV','w'))\""

violation "$HARDWARE" "storage is recorded with no durability class chosen" \
  "durability_class is omitted" \
  "python3 -c \"import json;d=json.load(open('$DEV'));del d['storage']['durability_class'];json.dump(d,open('$DEV','w'))\""

violation "$HARDWARE" "the schema validator is absent where it is required" \
  "validator is required" \
  "sed -i 's/^import jsonschema$/raise ImportError/' /dev/null 2>/dev/null; sed -i 's|python3 -c \"import jsonschema\" 2>/dev/null|false|' scripts/hardware-gate.sh"

# ── Documentation gate ────────────────────────────────────────────────────────
violation "$DOCS" "a required document is deleted" \
  "required documents missing" \
  "rm -f SECURITY.md"

violation "$DOCS" "a required document is emptied rather than deleted" \
  "required documents missing" \
  ": > GOVERNANCE.md"

violation "$DOCS" "a link into the founding documents goes dead" \
  "broken relative link" \
  "printf '\n[the charter](CHARTER-moved.md)\n' >> README.md"

violation "$DOCS" "an ADR title and its filename name different decisions" \
  "title says ADR-" \
  "sed -i '1s/ADR-0003/ADR-0031/' docs/adr/ADR-0003-sovereign-ingress.md"

violation "$DOCS" "an ADR is deleted, leaving a gap in the decision log" \
  "not contiguous" \
  "rm -f docs/adr/ADR-0003-sovereign-ingress.md README.md PLAN.md docs/CI.md"

violation "$DOCS" "the constitution understates how many rules a machine enforces" \
  "Appendix A claims" \
  "sed -i -E 's/^\\| \\*\\*\\[CI\\]\\*\\* \\| [0-9]+ \\|/| **[CI]** | 12 |/' GOVERNANCE-CONSTITUTION.md"

violation "$DOCS" "a rule is added without updating the enforcement table" \
  "Appendix A" \
  "printf '\\n### 99.1 A new rule **[CI]**\\n\\nPlanted by the gate self-test.\\n' >> GOVERNANCE-CONSTITUTION.md"

# ── Release gate ──────────────────────────────────────────────────────────────
violation "$RELEASE" "the crate version drifts from .release-version" \
  "but .release-version is" \
  "sed -i 's/^version = \"0.0.1\"/version = \"0.0.2\"/' core/Cargo.toml"

violation "$RELEASE" "a release has no changelog section" \
  "no '## \[" \
  "sed -i 's/^## \[0.0.1\].*/## [0.0.9] - later/' CHANGELOG.md"

violation "$RELEASE" "notes are left stranded under Unreleased at release time" \
  "content under \[Unreleased\]" \
  "sed -i 's/^## \[Unreleased\]/## [Unreleased]\n\n- forgot to move this/' CHANGELOG.md"

violation "$RELEASE" "an editor adds a trailing newline to .release-version" \
  "trailing newline" \
  "printf 'v0.0.1\n' > .release-version"

violation "$RELEASE" "the version file is deleted" \
  "missing" \
  "rm -f .release-version"

# ── Attribution gate ──────────────────────────────────────────────────────────
# These need a repository with history, so the sandbox gets its own.
init_repo='git init -q . && git add -A && git -c user.name=T -c user.email=t@example.com commit -qm "base"'

violation "$ATTRIB" "an assistant name appears in a tracked source file" \
  "assistant names appear in tracked files" \
  "printf '// written by copilot\n' >> core/src/lib.rs && $init_repo"

violation "$ATTRIB" "a generated-by line appears in the documentation" \
  "generated-by attribution" \
  "printf '\nGenerated with an AI assistant.\n' >> README.md && $init_repo"

violation "$ATTRIB" "an assistant co-author trailer reaches a commit message" \
  "assistant attribution in" \
  "$init_repo && printf 'x\n' >> NOTICE && git add -A && git -c user.name=T -c user.email=t@example.com commit -qm \"tidy up

Co-Authored-By: Gemini <noreply@example.com>\""

violation "$ATTRIB" "a commit is authored by a bot address" \
  "authored by a bot or noreply" \
  "$init_repo && printf 'x\n' >> NOTICE && git add -A && git -c user.name=B -c user.email='b[bot]@users.noreply.example.com' commit -qm tidy"

echo
if [ "$FAILED" -ne 0 ]; then
  echo "GATE SELF-TEST FAILED — a gate does not catch what it claims to."
  exit 1
fi
echo "Gate self-test passed: every check above is actually enforcing something."
