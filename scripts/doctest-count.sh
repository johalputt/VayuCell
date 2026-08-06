#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Run the doctests and assert a non-zero count actually ran.
#
# The registry's and the CSP's strongest guarantees are compile_fail doctests.
# Rustdoc collects doctests only from PUBLIC items, so a proof moved onto a
# private one runs zero tests and still prints "test result: ok" — the exit code
# cannot tell those apart. The count can.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

MIN="${1:-1}"

out="$(cargo test --workspace --doc 2>&1)"
rc=$?
printf '%s\n' "$out"
[ $rc -eq 0 ] || exit $rc

count="$(printf '%s' "$out" | grep -oE 'running [0-9]+ tests?' \
  | grep -oE '[0-9]+' | head -1)"

if [ -z "$count" ] || [ "$count" -lt "$MIN" ]; then
  echo
  echo "FAIL: expected at least $MIN doctest(s), saw ${count:-none}."
  echo "The compile_fail proofs are not being collected — check they are on a"
  echo "public item, because rustdoc silently ignores them anywhere else."
  exit 1
fi
echo "$count doctests ran (minimum $MIN)."
