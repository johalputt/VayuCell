#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Every `uses:` reference in a workflow must actually resolve.
#
# This gate exists because of a real failure, not a hypothetical one. Two
# workflows were written referencing `google/osv-scanner-action@v2` and
# `ossf/scorecard-action@v2` — neither of which is a tag either project
# publishes. The workflows were valid YAML, they parsed locally, they reviewed
# fine, and they failed in CI with "unable to find version v2" the first time
# they ran.
#
# That is the same defect class the rest of this repository guards against: a
# claim that looks checked and was not. A pinned version nobody verified is a
# pinned version, not a verified one.
#
# The check is a `git ls-remote` per distinct reference, which needs the network
# but not a token. Where the network is unavailable the gate reports UNVERIFIED
# and says so, rather than passing — Article IV binds this script too.
#
# Usage: scripts/actions-gate.sh
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

FAILED=0
pass() { printf '  ok          %s\n' "$1"; }
fail() { printf '  FAIL        %s\n' "$1"; FAILED=1; }
unver() { printf '  UNVERIFIED  %s\n' "$1"; }

echo "Actions gate — every workflow reference must resolve"
echo

# owner/repo[/subpath]@ref, deduplicated. Local (./…) and docker:// references
# are skipped: they are not fetched from a remote and have nothing to resolve.
mapfile -t REFS < <(
  grep -rhoE '^\s*(-\s*)?uses:\s*[A-Za-z0-9_.-]+/[A-Za-z0-9_./-]+@[A-Za-z0-9_.\-]+' \
    .github/workflows/ 2>/dev/null \
    | sed -E 's/^\s*(-\s*)?uses:\s*//' \
    | sort -u
)

if [ "${#REFS[@]}" -eq 0 ]; then
  fail "no workflow references found — the extraction pattern has gone stale"
  echo
  echo "ACTIONS GATE FAILED."
  exit 1
fi

if ! git ls-remote --tags --refs https://github.com/actions/checkout >/dev/null 2>&1; then
  unver "no network: workflow references could not be resolved"
  echo
  echo "The check did not run. It is not reported as passed."
  # In CI this must be a hard failure; on a laptop with no network it is not a
  # defect in the change being made.
  [ "${VAYUCELL_REQUIRE_NETWORK:-0}" = "1" ] && exit 1
  exit 0
fi

# Every reference must be pinned to a full commit SHA, and the SHA must exist.
#
# A tag is a moving target. `actions/checkout@v7` is whatever the owner of that
# tag decides it is tomorrow, and a tag can be repointed at any commit without
# anybody downstream seeing a diff — which is the supply-chain attack this
# project would otherwise have no defence against, on a binary people are asked
# to run unattended in the building where they sleep.
#
# So the check is now in two parts, and BOTH must hold: the reference is a
# 40-character SHA, and that SHA is reachable in the repository it names. The
# second part matters on its own — a SHA nobody can fetch fails the workflow on
# its first run, and a typo in a SHA looks exactly like a legitimate pin.
for entry in "${REFS[@]}"; do
  repo_path="${entry%@*}"
  ref="${entry##*@}"
  # Only the first two segments are the repository; the rest is a path to an
  # action inside it, which ls-remote neither knows nor needs.
  owner="$(echo "$repo_path" | cut -d/ -f1)"
  name="$(echo "$repo_path" | cut -d/ -f2)"

  if ! printf '%s' "$ref" | grep -qE '^[0-9a-f]{40}$'; then
    fail "$entry is not pinned to a commit SHA; a tag can be repointed at any commit without producing a diff here"
    continue
  fi

  # `ls-remote <sha>` does not work — a SHA is not a ref. Fetching the single
  # object is the only way to establish it exists without cloning the history.
  tmp="$(mktemp -d)"
  if git -C "$tmp" init -q 2>/dev/null \
     && git -C "$tmp" fetch -q --depth 1 "https://github.com/$owner/$name" "$ref" 2>/dev/null; then
    pass "$entry"
  else
    fail "$entry names a commit that could not be fetched from $owner/$name"
  fi
  rm -rf "$tmp"
done

# ── Package installs are pinned too ───────────────────────────────────────────
# Pinning the actions and then installing an unpinned package inside one of them
# closes the front door and leaves the side door open: `pip install jsonschema`
# resolves to whatever the index serves at that moment, in the job that decides
# whether a device profile is valid.
#
# So every pip install in a workflow must go through --require-hashes. That flag
# is the load-bearing part rather than the pinned version: it makes pip refuse
# when ANY package in the resolved set lacks a hash, so an unpinned transitive
# dependency fails the install instead of slipping through.
while IFS= read -r line; do
  file="${line%%:*}"
  rest="${line#*:}"
  case "$rest" in
    *--require-hashes*) pass "pip install in ${file##*/} is hash-pinned" ;;
    *) fail "${file##*/} installs a package without --require-hashes: ${rest## }" ;;
  esac
done < <(grep -rn 'pip install' .github/workflows/ 2>/dev/null || true)

# ── Every job in ci.yml is actually required ──────────────────────────────────
# The aggregating job carries a comment warning that "a job added above but
# forgotten here would otherwise be required in name and unenforced in fact".
# That is exactly what happened: the install job was added, left out of `needs`,
# failed on its first run, and CI reported all required checks green. The hazard
# was written down and nothing enforced it. This enforces it.
python3 - <<'PYCHECK' || FAILED=1
import sys, yaml

ci = yaml.safe_load(open(".github/workflows/ci.yml"))
jobs = ci.get("jobs", {})
# The aggregator is the job that depends on many others; found by shape rather
# than by name, so renaming it does not quietly disable this check.
agg = next((n for n, j in jobs.items()
            if isinstance(j.get("needs"), list) and len(j["needs"]) > 3), None)
if agg is None:
    print("  FAIL        ci.yml has no aggregating required-checks job")
    sys.exit(1)

required = set(jobs[agg]["needs"])
missing = sorted(set(jobs) - required - {agg})
for m in missing:
    print(f"  FAIL        ci.yml job '{m}' is missing from {agg}.needs, so it cannot fail the build")
if missing:
    sys.exit(1)
print(f"  ok          all {len(required)} ci.yml jobs are required by {agg}")
PYCHECK

echo
if [ "$FAILED" -ne 0 ]; then
  echo "ACTIONS GATE FAILED — a workflow reference is unpinned or unreachable."
  echo "It would fail on the first run, in CI, at the worst possible moment."
  exit 1
fi
echo "Actions gate passed: all ${#REFS[@]} workflow references are SHA-pinned and reachable."
