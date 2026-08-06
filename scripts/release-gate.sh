#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Release preflight: the version must say the same thing in every place it is
# written down, and the release notes must exist before the tag does.
#
# A release where .release-version, Cargo.toml and CHANGELOG.md disagree is a
# release whose artefacts cannot be matched back to their source. That is the
# same class of defect as an unverifiable safety claim, applied to distribution.
#
# Usage:
#   scripts/release-gate.sh            check the current version is consistent
#   scripts/release-gate.sh --next     print the next patch version and stop
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

FAILED=0
pass() { printf '  ok    %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1"; FAILED=1; }

read_release_version() {
  [ -f .release-version ] || return 1
  tr -d '\n' < .release-version
}

crate_version() {
  grep -m1 '^version = ' core/Cargo.toml | cut -d'"' -f2
}

if [ "${1:-}" = "--next" ]; then
  cur="$(read_release_version || echo v0.0.0)"
  IFS=. read -r maj min pat <<< "${cur#v}"
  # Patch only. A minor or major bump is a decision, not something a script
  # should make on anyone's behalf.
  echo "v${maj}.${min}.$((pat + 1))"
  exit 0
fi

echo "Release gate"
echo

if ! rv="$(read_release_version)"; then
  fail ".release-version is missing"
  rv=""
else
  case "$rv" in
    v[0-9]*.[0-9]*.[0-9]*) pass ".release-version is $rv" ;;
    *) fail ".release-version is not vMAJOR.MINOR.PATCH: '$rv'" ;;
  esac
  # No trailing newline: the file is compared byte-for-byte by the tag workflow,
  # and an editor that adds one would retrigger a release that already shipped.
  if [ "$(tail -c1 .release-version | wc -l)" -ne 0 ]; then
    fail ".release-version has a trailing newline; it must not"
  else
    pass ".release-version has no trailing newline"
  fi
fi

cv="$(crate_version)"
if [ -z "$cv" ]; then
  fail "core/Cargo.toml declares no version"
elif [ "v$cv" != "$rv" ]; then
  fail "core/Cargo.toml is $cv but .release-version is $rv"
else
  pass "core/Cargo.toml agrees at $cv"
fi

if [ ! -f CHANGELOG.md ]; then
  fail "CHANGELOG.md is missing"
elif [ -n "$rv" ] && ! grep -q "^## \[${rv#v}\]" CHANGELOG.md; then
  fail "CHANGELOG.md has no '## [${rv#v}]' section for this release"
else
  pass "CHANGELOG.md documents ${rv#v}"
fi

# An unreleased section that still has content at tag time means somebody wrote
# notes and forgot to move them under the version being shipped.
if [ -f CHANGELOG.md ] && awk '/^## \[Unreleased\]/{f=1;next} /^## \[/{f=0} f && NF && !/^$/' CHANGELOG.md | grep -q .; then
  fail "CHANGELOG.md still has content under [Unreleased]"
else
  pass "nothing is stranded under [Unreleased]"
fi

if [ -n "$rv" ] && git rev-parse -q --verify "refs/tags/$rv" >/dev/null; then
  fail "tag $rv already exists; releases are never re-tagged"
else
  pass "tag is free"
fi

echo
if [ "$FAILED" -ne 0 ]; then
  echo "RELEASE GATE FAILED."
  exit 1
fi
echo "Release gate passed."
