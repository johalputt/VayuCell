#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Attribution gate.
#
# This project's history records who is accountable for a decision. Assistant
# tooling may be used freely; what it may not do is appear in the permanent
# record as an author, a co-author, or a signature in a comment. A commit trailer
# is forever, and a reader years from now needs a person to ask.
#
# Scope: tracked files and commit messages. Both are pushed artefacts.
#
# Usage: scripts/attribution-gate.sh [base-ref] [head-ref]
#   base-ref  Only inspect commits after this ref. Defaults to the whole history
#             on a local run; CI passes the merge base so a pull request is
#             judged on its own commits.
#   head-ref  Where to stop. Defaults to HEAD.
#
# The head-ref exists because of a false positive this gate produced the first
# time the project used a pull request. On a `pull_request` event, actions/checkout
# leaves HEAD at a MERGE COMMIT that GitHub synthesises for the run — and that
# commit is authored with the `@users.noreply.github.com` address, which this gate
# rejects. It is not a bot hiding its identity and it never enters the history;
# it is an artefact of the checkout. Ending the range at the pull request's own
# head commit judges the commits somebody actually wrote.
set -uo pipefail

# Without -e a failed cd would leave the gate running against whatever
# directory it was invoked from, where empty file lists make several checks
# pass trivially. That is a false green, so it exits instead.
cd "$(dirname "$0")/.." || exit 1

FAILED=0
pass() { printf '  ok    %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1"; FAILED=1; }

# Assembled at runtime so this file does not itself contain the literal strings
# it bans — otherwise the gate would flag its own source and every run would be
# a false positive.
BANNED_TRAILER='Co-Authored-By:[[:space:]]*(Claude|GPT|Copilot|Gemini|Codex|Devin)'
BANNED_WORDS="$(printf '%s\n' 'claude' 'anthropic' 'chatgpt' 'openai' 'copilot' 'gemini' \
  | paste -sd'|' -)"
GENERATED_BY='(Generated|Authored|Written)[[:space:]]+(with|by)[[:space:]]+(an?[[:space:]]+)?(AI|LLM|assistant|Claude|GPT|Copilot)'

echo "Attribution gate"
echo

# ── Tracked file contents ─────────────────────────────────────────────────────
# Two files are excluded, and only two, each for a reason that would otherwise
# make every run a false positive:
#
#   attribution-gate.sh  must name the strings it bans in order to look for them.
#   gate-selftest.sh     plants those exact strings as violations, on purpose, to
#                        prove this gate catches them.
#
# Nothing else is exempt. The exclusion is spelled out per file rather than as a
# blanket ':!scripts/*', because a directory-wide exemption would quietly cover
# every script written afterwards.
EXCLUDE=(':!scripts/attribution-gate.sh' ':!scripts/gate-selftest.sh')

hits="$(git grep -InE "$BANNED_WORDS" -- "${EXCLUDE[@]}" 2>/dev/null || true)"
if [ -n "$hits" ]; then
  fail "assistant names appear in tracked files:"
  printf '        %s\n' "$hits"
else
  pass "no assistant name appears in any tracked file"
fi

hits="$(git grep -InE "$GENERATED_BY" -- "${EXCLUDE[@]}" 2>/dev/null || true)"
if [ -n "$hits" ]; then
  fail "a generated-by attribution appears in tracked files:"
  printf '        %s\n' "$hits"
else
  pass "no generated-by attribution in tracked files"
fi

# ── Commit messages ───────────────────────────────────────────────────────────
BASE="${1:-}"
TIP="${2:-}"

# With no explicit tip, step off GitHub's synthetic pull-request merge commit.
#
# On a `pull_request` event actions/checkout leaves HEAD at a merge commit that
# GitHub creates for the run — authored with an `@users.noreply.github.com`
# address, which the bot-author check below rejects. It is not a bot hiding its
# identity and it never enters the history: it is an artefact of the checkout,
# discarded when the run ends.
#
# Detected structurally rather than by matching the address, because the address
# is exactly the thing worth rejecting everywhere else. The signature is: this is
# a pull_request event AND HEAD has two parents. `HEAD^2` is then the pull
# request's own head — the commits somebody actually wrote.
#
# This has to work with no arguments, because the gate self-test runs every gate
# bare on a clean tree before planting anything, and a gate that fails there
# stops the whole self-test rather than just itself.
if [ -z "$TIP" ]; then
  TIP="HEAD"
  if [ "${GITHUB_EVENT_NAME:-}" = "pull_request" ] \
     && [ "$(git rev-list --no-walk --parents -1 HEAD 2>/dev/null | wc -w)" -ge 3 ] \
     && git rev-parse --verify --quiet 'HEAD^2' >/dev/null; then
    TIP="HEAD^2"
    echo "  --    HEAD is a synthesised pull-request merge; judging $TIP instead"
  fi
fi
git rev-parse --verify --quiet "$TIP" >/dev/null || TIP="HEAD"
if [ -n "$BASE" ] && git rev-parse --verify --quiet "$BASE" >/dev/null; then
  RANGE="$BASE..$TIP"
  scope="commits in $RANGE"
else
  # No base: walk the whole history, but from TIP rather than HEAD. Leaving this
  # empty was the second half of the same bug — stepping off the synthesised
  # merge commit achieves nothing if the log is then walked from HEAD anyway.
  RANGE="$TIP"
  scope="every commit reachable from $TIP"
fi

# Each line is prefixed with its commit hash, so a match names the commit to fix
# rather than just the offending line. Command substitution cannot carry NUL, so
# the record separator is a printable one.
bad_msgs="$(git log ${RANGE:+"$RANGE"} --format='%H %B' 2>/dev/null \
  | grep -iEn "$BANNED_TRAILER|$GENERATED_BY|$BANNED_WORDS" || true)"
if [ -n "$bad_msgs" ]; then
  fail "assistant attribution in $scope:"
  printf '%s\n' "$bad_msgs" | sed 's/^/        /'
else
  pass "no assistant attribution in $scope"
fi

# ── Author identity ───────────────────────────────────────────────────────────
# Every commit must carry a real person's address. A noreply or bot address in
# the author field is the same defect as a trailer: nobody to ask.
#
# Dependabot is the one exemption, and it is narrow. The rule exists so that every
# DECISION has an accountable person; a dependency bump carries no decision until
# somebody reviews and merges it, and that human act is the accountability. The
# exemption covers authorship only — a Dependabot commit is still scanned for
# assistant attribution like any other.
bot_authors="$(git log ${RANGE:+"$RANGE"} --format='%H %an <%ae>' 2>/dev/null \
  | grep -iE '<[^>]*(noreply|no-reply|bot@|\[bot\])' \
  | grep -v 'dependabot\[bot\]' || true)"
if [ -n "$bot_authors" ]; then
  fail "commits authored by a bot or noreply address:"
  printf '%s\n' "$bot_authors" | sed 's/^/        /'
else
  pass "every commit is authored by a reachable person"
fi

echo
if [ "$FAILED" -ne 0 ]; then
  echo "ATTRIBUTION GATE FAILED."
  echo "Assistant tooling is welcome; it does not go in the permanent record."
  exit 1
fi
echo "Attribution gate passed."
