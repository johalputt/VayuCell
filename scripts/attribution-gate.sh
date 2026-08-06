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
# Usage: scripts/attribution-gate.sh [base-ref]
#   base-ref  Only inspect commits after this ref. Defaults to the whole history
#             on a local run; CI passes the merge base so a pull request is
#             judged on its own commits.
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
# scripts/ is excluded from the word scan because this gate must name the strings
# it looks for. It is not excluded from the trailer or generated-by scan.
hits="$(git grep -InE "$BANNED_WORDS" -- ':!scripts/attribution-gate.sh' 2>/dev/null || true)"
if [ -n "$hits" ]; then
  fail "assistant names appear in tracked files:"
  printf '        %s\n' "$hits"
else
  pass "no assistant name appears in any tracked file"
fi

hits="$(git grep -InE "$GENERATED_BY" -- ':!scripts/attribution-gate.sh' 2>/dev/null || true)"
if [ -n "$hits" ]; then
  fail "a generated-by attribution appears in tracked files:"
  printf '        %s\n' "$hits"
else
  pass "no generated-by attribution in tracked files"
fi

# ── Commit messages ───────────────────────────────────────────────────────────
BASE="${1:-}"
if [ -n "$BASE" ] && git rev-parse --verify --quiet "$BASE" >/dev/null; then
  RANGE="$BASE..HEAD"
  scope="commits in $RANGE"
else
  RANGE=""
  scope="every commit in the history"
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
bot_authors="$(git log ${RANGE:+"$RANGE"} --format='%H %an <%ae>' 2>/dev/null \
  | grep -iE '<[^>]*(noreply|no-reply|bot@|\[bot\])' || true)"
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
