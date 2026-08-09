#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Run the doctests and assert the EXPECTED number actually ran.
#
# The registry's and the CSP's strongest guarantees are compile_fail doctests.
# Rustdoc collects doctests only from PUBLIC items, so a proof moved onto a
# private one runs zero tests and still prints "test result: ok" — the exit code
# cannot tell those apart. The count can.
#
# The count is asserted EXACTLY, not as a floor. A floor of one only catches all
# of the proofs disappearing at once, which is the least likely way to lose them.
# They are lost one at a time: an item made private during a refactor, a proof
# folded into a doc comment that moved. With twenty proofs and a floor of one,
# nineteen could stop being collected and this script would still print "ok" —
# which is exactly the class of silent pass every other gate here exists to
# catch, sitting inside the gate meant to catch it.
#
# Adding or removing a proof therefore means editing this number, and that is
# the point: it becomes one line in a diff stating how many compile-time
# guarantees this crate ships.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

EXPECTED="${1:-28}"

out="$(cargo test --workspace --doc 2>&1)"
rc=$?
printf '%s\n' "$out"
[ $rc -eq 0 ] || exit $rc

count="$(printf '%s' "$out" | grep -oE 'running [0-9]+ tests?' \
  | grep -oE '[0-9]+' | head -1)"

if [ -z "$count" ]; then
  echo
  echo "FAIL: no doctest count could be read from the output at all."
  exit 1
fi

if [ "$count" -lt "$EXPECTED" ]; then
  echo
  echo "FAIL: expected $EXPECTED doctests, saw $count — $((EXPECTED - count)) are"
  echo "no longer being collected. Rustdoc silently ignores a doctest on a"
  echo "non-public item, so check whether a proof moved onto a private one."
  exit 1
fi

if [ "$count" -gt "$EXPECTED" ]; then
  echo
  echo "FAIL: expected $EXPECTED doctests, saw $count."
  echo "New proofs are welcome — raise the number in scripts/doctest-count.sh so"
  echo "the count keeps meaning something."
  exit 1
fi

echo "$count doctests ran, exactly as expected."
