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
#   scripts/release-gate.sh              check the current version is consistent
#   scripts/release-gate.sh --releasing  additionally require [Unreleased] to be
#                                        empty; only true at tag time
#   scripts/release-gate.sh --next       print the next patch version and stop
#
# On --releasing: the first version of this gate required [Unreleased] to be
# empty on EVERY push, which is not a stricter rule — it is a wrong one. Keep a
# Changelog exists so notes accumulate there between releases, and a gate that
# forbids the accumulating makes the section pointless. The requirement is real
# at the moment a tag is cut and meaningless before it, so it is now asked for
# only then, and release.yml is what asks.
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

RELEASING=0
for arg in "$@"; do
  [ "$arg" = "--releasing" ] && RELEASING=1
done

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

# Every other crate, and every internal pin between them. The gate checked
# core/Cargo.toml alone, so a bump left cli/Cargo.toml at the old number and
# left its `vayucell-core = { version = "…" }` pin naming a version that no
# longer existed. That is a release which fails at dependency resolution — after
# the tag is pushed and public, in the one job whose whole purpose is to be
# trustworthy. Manifests are found rather than listed, so a crate added later is
# not exempt from the rule by never having been named.
while IFS= read -r m; do
  mver="$(grep -m1 '^version = ' "$m" | cut -d'"' -f2)"
  [ -n "$mver" ] || continue
  if [ "v$mver" != "$rv" ]; then
    fail "$m is $mver but .release-version is $rv"
  else
    pass "$m agrees at $mver"
  fi
done < <(find . -mindepth 2 -maxdepth 2 -name Cargo.toml -not -path './fuzz/*' -not -path './target/*' | sort)

bad_pin=""
while IFS= read -r pin; do
  [ "$pin" = "${rv#v}" ] || bad_pin="$bad_pin $pin"
done < <(grep -rhoE 'vayucell-core = \{[^}]*version = "[^"]+"' ./*/Cargo.toml 2>/dev/null \
           | grep -oE 'version = "[^"]+"' | cut -d'"' -f2)
if [ -n "$bad_pin" ]; then
  fail "an internal dependency pins vayucell-core at:$bad_pin, not ${rv#v}"
else
  pass "every internal pin on vayucell-core names ${rv#v}"
fi

if [ ! -f CHANGELOG.md ]; then
  fail "CHANGELOG.md is missing"
elif [ -n "$rv" ] && ! grep -q "^## \[${rv#v}\]" CHANGELOG.md; then
  fail "CHANGELOG.md has no '## [${rv#v}]' section for this release"
else
  pass "CHANGELOG.md documents ${rv#v}"
fi

# An unreleased section with content in it is normal between releases and is a
# defect at tag time, when it means somebody wrote notes and forgot to move them
# under the version being shipped.
unreleased_lines="$(awk '/^## \[Unreleased\]/{f=1;next} /^## \[/{f=0} f && NF' CHANGELOG.md 2>/dev/null | wc -l)"
if [ "${RELEASING:-0}" = "1" ]; then
  if [ "$unreleased_lines" -gt 0 ]; then
    fail "CHANGELOG.md still has content under [Unreleased] at tag time"
  else
    pass "nothing is stranded under [Unreleased]"
  fi
elif [ "$unreleased_lines" -gt 0 ]; then
  printf '  --    %s\n' "[Unreleased] holds $unreleased_lines line(s); that is expected between releases"
else
  pass "[Unreleased] is empty"
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
