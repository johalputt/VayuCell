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
RELEASING="scripts/release-gate.sh --releasing"
ACTIONS=scripts/actions-gate.sh
CONST=scripts/constitution-gate.sh
ATTRIB=scripts/attribution-gate.sh
INSTALL=scripts/install-gate.sh

# The install gate's end-to-end case builds VayuCell from source, which takes
# minutes. What is under test here is whether its *static* checks fire, and each
# plant would otherwise pay for a full build to answer a question the build has
# no bearing on. CI's install job runs the end-to-end path unskipped.
export VAYUCELL_SKIP_INSTALL_RUN=1

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

  # Split on whitespace so a case can name a gate with arguments — the release
  # gate has a --releasing form whose rules differ, and it needs testing too.
  local -a gate_cmd
  read -ra gate_cmd <<< "$gate"

  local out
  out="$(cd "$sandbox" && bash "${gate_cmd[@]}" 2>&1)"
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
for gate in "$CHARTER" "$HARDWARE" "$DOCS" "$ATTRIB" "$RELEASE" "$CONST" "$INSTALL"; do
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

# Article III.1 is now SATISFIED: core/src/governor.rs exists, so a serving
# capability is permitted and the gate is right to pass. Planting one alone no
# longer violates anything. The rule still has a live direction — a serving
# capability with the governor removed — and that is what this now plants.
violation "$CHARTER" "III.1 the governor is deleted while a serving capability exists" \
  "III.1" \
  "printf 'const _PLANTED: \&str = \"class: Class::Serving\";\n' >> core/src/capability.rs && rm -f core/src/governor.rs"

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

violation "$CHARTER" "V.2 production source opens an outbound connection" \
  "V.2" \
  "printf 'fn dial() { let _ = std::net::TcpStream::connect(\"h:1\"); }\n' >> cli/src/device.rs"

# The same call inside a test module must NOT be caught. A check that flagged it
# would be reporting this repo's own pool tests as egress, and the usual repair
# for that is to loosen the check until it catches nothing — so the discrimination
# is planted as its own case rather than assumed from the one above.
violation "$CHARTER" "V.2 the scan reads a project source file that is not core/" \
  "V.2" \
  "printf 'fn dial2() { let _ = std::net::TcpStream::connect(\"h:2\"); }\n' >> cli/src/report.rs"

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

# The roadmap carries a status per phase and every partly-built one leans on a
# path: "the types are in core/src/durability.rs". A rename falsifies that
# sentence without making it look wrong — a claim that reads as verified and
# points at nothing, which is the defect this project spends most of its effort
# on. Planted as a rename rather than a deletion, because that is how it happens.
# "Sixteen modules in core/src" sat in the README while the crate had twenty.
# Planted as an added module rather than an edited sentence, because that is the
# direction it drifts: nobody rewrites the prose when they add a file.
violation "$DOCS" "a module is added and the README's count is not" \
  "lib.rs declares" \
  "printf 'pub mod planted;\n' >> core/src/lib.rs"

violation "$DOCS" "the roadmap names a source file that no longer exists" \
  "source path that does not exist" \
  "sed -i 's|\`core/src/durability.rs\`|\`core/src/replicator.rs\`|' PLAN.md"

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
  "sed -i '0,/^version = /s/^version = .*/version = \"9.9.9\"/' core/Cargo.toml"

violation "$RELEASE" "a release has no changelog section" \
  "no '## \[" \
  "sed -i '/^## \[[0-9]/d' CHANGELOG.md"

# The tag-time form. Between releases this content is expected and the gate says
# so; only --releasing makes it a defect, and both directions need proving.
violation "$RELEASING" "notes are left stranded under Unreleased at tag time" \
  "content under \[Unreleased\] at tag time" \
  "sed -i 's/^## \[Unreleased\]/## [Unreleased]\n\n- forgot to move this/' CHANGELOG.md"

violation "$RELEASE" "an editor adds a trailing newline to .release-version" \
  "trailing newline" \
  "printf 'v0.0.1\n' > .release-version"

violation "$RELEASE" "the version file is deleted" \
  "missing" \
  "rm -f .release-version"

# ── Constitution gate ─────────────────────────────────────────────────────────
violation "$CONST" "a rule claims [CI] while naming no enforcer" \
  "name no enforcer" \
  "printf '\\n### 99.1 A planted rule **[CI]**\\n\\nNothing enforces this.\\n' >> GOVERNANCE-CONSTITUTION.md"

violation "$CONST" "a rule cites an enforcer that has been deleted" \
  "does not exist" \
  "sed -i '0,/\\*\\*Enforced by:\\*\\* \`scripts\\/charter-gate.sh\`/s//**Enforced by:** \`scripts\\/deleted-gate.sh\`/' GOVERNANCE-CONSTITUTION.md"

# ── Actions gate ──────────────────────────────────────────────────────────────
# Skipped without network: the gate reports UNVERIFIED there rather than
# failing, so a planted violation would be scored MISSED against a gate that is
# behaving correctly.
if git ls-remote --tags --refs https://github.com/actions/checkout >/dev/null 2>&1; then
  # Unpinning is now the violation. The previous version of this case planted a
  # tag that does not exist, which stopped being plantable the moment every
  # reference became a SHA — a case that can no longer be planted is a case that
  # tests nothing, and the STALE check is what would have said so.
  violation "$ACTIONS" "a workflow action is unpinned from its commit SHA" \
    "is not pinned to a commit SHA" \
    "sed -i -E '0,/uses: actions\\/checkout@[0-9a-f]{40}/s||uses: actions/checkout@v7|' .github/workflows/ci.yml"

  violation "$ACTIONS" "a workflow installs a package without --require-hashes" \
    "without --require-hashes" \
    "sed -i 's|--require-hashes -r requirements/schema.txt|jsonschema|' .github/workflows/ci.yml"

  violation "$ACTIONS" "a ci.yml job is added but never made a required check" \
  "cannot fail the build" \
  "sed -i '/^      - install$/d' .github/workflows/ci.yml"

violation "$ACTIONS" "a workflow names a commit SHA that does not exist" \
    "could not be fetched" \
    "sed -i -E '0,/uses: actions\\/checkout@[0-9a-f]{40}/s|@[0-9a-f]{40}|@0000000000000000000000000000000000000000|' .github/workflows/ci.yml"
else
  echo "  --      actions gate cases skipped: no network"
fi

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

violation "$CHARTER" "the CLI crate gains a third-party dependency" \
  "V.5 a crate gained runtime dependencies" \
  "sed -i '/^\\[dependencies\\]/a serde = \"1\"' cli/Cargo.toml"

violation "$CHARTER" "the fuzz harness rejoins the workspace, making its dependency shippable" \
  "not excluded in the root Cargo.toml" \
  "sed -i 's|^exclude = \\[\"fuzz\"\\]|exclude = []|' Cargo.toml"

# ── Installer ─────────────────────────────────────────────────────────────────
# install.sh is the one file here that runs on a stranger's phone, so the gate
# guarding it is the one most worth proving fires. Each plant below is a way the
# installer could quietly become worse without a single test going red.

violation "$INSTALL" "the battery warning stops being shown before anything is written" \
  "before the first write to disk" \
  "sed -i '/swollen battery is a fire hazard/d' install.sh"

violation "$INSTALL" "a failure path names what broke but not what to do about it" \
  "do not say what to do next" \
  "printf '\ndie \"it broke\"\n' >> install.sh"

violation "$INSTALL" "the physical-inspection instruction is dropped from the installer" \
  "must name physical inspection" \
  "sed -i '/face-down on a flat table/d' install.sh"

violation "$INSTALL" "the installer starts asking for root" \
  "escalates privileges" \
  "printf '\nsudo pkg install rust\n' >> install.sh"

violation "$INSTALL" "the release stops building a target the installer downloads" \
  "the release does not build" \
  "sed -i '/^          - aarch64-linux-android\$/d' .github/workflows/release.yml"

violation "$INSTALL" "the release publishes something other than a runnable binary" \
  "will never find a build" \
  "sed -i '/-czf \"dist\\/vayucell-/d' .github/workflows/release.yml"

violation "$RELEASE" "a crate is left behind at the previous version" \
  "cli/Cargo.toml is 0.0.0" \
  "sed -i '0,/^version = /s/^version = .*/version = \"0.0.0\"/' cli/Cargo.toml"

violation "$RELEASE" "an internal dependency pins a version that no longer exists" \
  "pins vayucell-core at" \
  "sed -i 's|version = \"[^\"]*\" }|version = \"0.0.0\" }|' cli/Cargo.toml"

# ── Doctest count ─────────────────────────────────────────────────────────────
# Checked directly rather than through violation(), which copies the tree
# without target/ — a sandboxed cargo case would rebuild the crate from scratch
# for each assertion, and the thing under test here is arithmetic.
#
# Worth testing at all because this gate exists to catch a silent pass, and for
# most of its life it could not: with a floor of one, fifteen of sixteen proofs
# could stop being collected and it would still print ok. Both directions are
# exercised, because a check that only fires when the count is too LOW would go
# quiet again the moment somebody adds a proof without updating the number.
expect_doctest_failure() {
  local arg="$1" desc="$2" expect="$3" out rc
  out="$(cd "$REPO" && bash scripts/doctest-count.sh "$arg" 2>&1)"; rc=$?
  if [ "$rc" -eq 0 ]; then
    echo "  MISSED  $desc"
    echo "          the gate passed with the count deliberately wrong."
    FAILED=1
  elif ! printf '%s' "$out" | grep -q "$expect"; then
    echo "  WRONG   $desc"
    echo "          expected a failure mentioning: $expect"
    FAILED=1
  else
    echo "  caught  $desc"
  fi
}

expect_doctest_failure 999 "a compile-time proof stops being collected" \
  "no longer being collected"
expect_doctest_failure 1 "a proof is added without the count being raised" \
  "keeps meaning something"

echo
if [ "$FAILED" -ne 0 ]; then
  echo "GATE SELF-TEST FAILED — a gate does not catch what it claims to."
  exit 1
fi
echo "Gate self-test passed: every check above is actually enforcing something."
