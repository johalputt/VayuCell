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

for entry in "${REFS[@]}"; do
  repo_path="${entry%@*}"
  ref="${entry##*@}"
  # Only the first two segments are the repository; the rest is a path to an
  # action inside it, which ls-remote neither knows nor needs.
  owner="$(echo "$repo_path" | cut -d/ -f1)"
  name="$(echo "$repo_path" | cut -d/ -f2)"

  if git ls-remote --exit-code "https://github.com/$owner/$name" \
      "refs/tags/$ref" "refs/heads/$ref" >/dev/null 2>&1; then
    pass "$entry"
  else
    latest="$(git ls-remote --tags --refs "https://github.com/$owner/$name" 2>/dev/null \
      | awk -F/ '{print $NF}' | grep -E '^v?[0-9]+(\.[0-9]+)*$' | sort -V | tail -1)"
    fail "$entry does not resolve${latest:+ (latest tag: $latest)}"
  fi
done

echo
if [ "$FAILED" -ne 0 ]; then
  echo "ACTIONS GATE FAILED — a workflow references something that does not exist."
  echo "It would fail on the first run, in CI, at the worst possible moment."
  exit 1
fi
echo "Actions gate passed: all ${#REFS[@]} workflow references resolve."
