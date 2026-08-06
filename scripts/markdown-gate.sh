#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Markdown lint, at a version pinned in ONE place.
#
# This script exists because of a real failure. CI ran the linter through
# `DavidAnson/markdownlint-cli2-action@v24`, which bundles markdownlint v0.41;
# the local check ran `markdownlint-cli2@0.18.1`, which bundles v0.38. v0.41
# added MD060, so the local gate printed "0 errors" and the push failed on a rule
# the laptop had never heard of.
#
# That is worse than an ordinary red build. A gate whose local form is weaker
# than its CI form actively misleads the person running it, and the whole point
# of putting every check in scripts/ was that the two are the same check.
#
# So the version lives here, once, and CI runs this script instead of the action.
# There is now no second place for it to drift.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

# Pinned exactly. Bumping it is a deliberate edit that shows up in a diff, and
# the new rules it brings are dealt with in that same change rather than
# arriving unannounced in somebody else's push.
VERSION="0.19.0"

if ! command -v npx >/dev/null 2>&1; then
  echo "  UNVERIFIED  markdown lint did not run: npx is not installed"
  # A check that did not run is never reported as one that passed.
  [ "${VAYUCELL_REQUIRE_MARKDOWNLINT:-0}" = "1" ] && exit 1
  exit 0
fi

echo "markdownlint-cli2@$VERSION"
exec npx --yes "markdownlint-cli2@$VERSION" "$@"
