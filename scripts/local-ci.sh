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
  "msrv|1|scripts/msrv-gate.sh"
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
  "install|1|scripts/install-gate.sh"
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

# Said on every green run, because a green run here is the moment somebody
# concludes CI will be green too.
#
# CI installs `stable` on the day it runs. This machine has whatever it has, and
# clippy gains lints between releases — `byte_char_slices` landed after 1.94 and
# failed CI on a line that had been passing locally for weeks. The lint was
# right and the local gate was not wrong; it simply could not see it.
#
# So the limit is stated rather than left to be discovered. This is the same
# rule the code follows: a check that could not be made must not read as one
# that was.
echo "Note: clippy here is $(cargo clippy --version 2>/dev/null | cut -d' ' -f2), and CI"
echo "      installs whatever \`stable\` is on the day. A lint added since this"
echo "      toolchain can fail there having passed here. Green is not parity."
echo "      The MSRV gate covers the other direction — code this compiler accepts"
echo "      and the declared rust-version does not."
exit 0
