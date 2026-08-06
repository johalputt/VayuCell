#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Line coverage over PRODUCTION code, with the same flags CI uses.
#
# Test files are excluded deliberately. Counting them inflates the figure with
# the coverage of the tests themselves — here it is several points — and a
# number that flatters itself is not worth having.
#
# This is a floor that catches whole modules landing untested, not a target.
# A percentage says how much code ran, not whether anything was checked while it
# ran; scripts/mutation-gate.sh is the check that answers that.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

FLOOR="${VAYUCELL_COVERAGE_FLOOR:-80}"

if ! command -v cargo-llvm-cov >/dev/null 2>&1; then
  echo "cargo-llvm-cov is not installed:"
  echo "  cargo install cargo-llvm-cov --locked"
  echo "  rustup component add llvm-tools-preview"
  # Not a pass. A missing tool means the check did not run, and a check that did
  # not run is never reported as one that passed.
  exit 1
fi

exec cargo llvm-cov --workspace --all-features \
  --ignore-filename-regex '_test\.rs$' \
  --fail-under-lines "$FLOOR" \
  "${@:---summary-only}"
