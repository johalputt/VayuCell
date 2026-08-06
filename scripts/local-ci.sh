#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Everything CI runs, in the order CI runs it, on your machine.
#
# The point is that a contributor should never learn about a failure from a red
# check five minutes after pushing. Every gate in .github/workflows/ci.yml has
# its logic in scripts/, precisely so this file can exist.
#
# Usage:
#   scripts/local-ci.sh            every gate
#   scripts/local-ci.sh --fast     skip the slow ones (mutation, coverage, self-test)
#   scripts/local-ci.sh --list     print what would run, and stop
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

FAST=0
LIST=0
for a in "$@"; do
  case "$a" in
    --fast) FAST=1 ;;
    --list) LIST=1 ;;
    -h|--help) sed -n '3,12p' "$0"; exit 0 ;;
    *) echo "unknown option: $a"; exit 2 ;;
  esac
done

# name | slow? | command
STEPS=(
  "fmt|0|cargo fmt --all -- --check"
  "clippy|0|cargo clippy --workspace --all-targets --all-features -- -D warnings"
  "build|0|cargo build --workspace --all-features"
  "test|0|cargo test --workspace --all-features"
  "doctests|0|scripts/doctest-count.sh"
  "doc|0|cargo doc --workspace --all-features --no-deps"
  "charter|0|scripts/charter-gate.sh"
  "attribution|0|scripts/attribution-gate.sh"
  "docs|0|scripts/docs-gate.sh"
  "constitution|0|scripts/constitution-gate.sh"
  "hardware|0|scripts/hardware-gate.sh"
  "release|0|scripts/release-gate.sh"
  "markdown|0|scripts/markdown-gate.sh"
  "actions|1|scripts/actions-gate.sh"
  "shellcheck|0|shellcheck --severity=warning --shell=bash scripts/*.sh"
  "gate-selftest|1|scripts/gate-selftest.sh"
  "mutation|1|scripts/mutation-gate.sh"
  "coverage|1|scripts/coverage.sh"
)

if [ "$LIST" = "1" ]; then
  for s in "${STEPS[@]}"; do
    IFS='|' read -r name slow cmd <<< "$s"
    [ "$FAST" = "1" ] && [ "$slow" = "1" ] && continue
    printf '  %-14s %s\n' "$name" "$cmd"
  done
  exit 0
fi

export RUSTDOCFLAGS="${RUSTDOCFLAGS:--D warnings}"

FAILED=()
START_ALL=$SECONDS

for s in "${STEPS[@]}"; do
  IFS='|' read -r name slow cmd <<< "$s"
  if [ "$FAST" = "1" ] && [ "$slow" = "1" ]; then
    printf '  %-14s skipped (--fast)\n' "$name"
    continue
  fi
  start=$SECONDS
  # Output is captured and only shown on failure. A gate that prints nothing
  # when it passes is a gate you will actually run before every push.
  if out="$(eval "$cmd" 2>&1)"; then
    printf '  %-14s ok      %ss\n' "$name" "$((SECONDS - start))"
  else
    printf '  %-14s FAIL    %ss\n' "$name" "$((SECONDS - start))"
    printf '%s\n' "$out" | sed 's/^/      /'
    FAILED+=("$name")
  fi
done

echo
if [ ${#FAILED[@]} -ne 0 ]; then
  echo "FAILED after $((SECONDS - START_ALL))s: ${FAILED[*]}"
  exit 1
fi
echo "All gates passed in $((SECONDS - START_ALL))s."
[ "$FAST" = "1" ] && echo "Note: --fast skipped the mutation gate, the gate self-test and coverage."
exit 0
