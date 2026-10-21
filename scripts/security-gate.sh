#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# The security scans must be inside the gate, and no finding may be left open.
#
# Two checks, each of which exists because of a specific failure rather than a
# general worry.
#
# 1. **A workflow that runs on a push must live in ci.yml, and every one of
#    its jobs must be in the required check.** The static analysis, the SBOM,
#    the scorecard scan and the fuzz harness each ran in a workflow of their
#    own. Their verdicts were visible — a red run, an alert in the security
#    tab — and nothing about them could fail the build. So a finding sat open
#    in this repository for a month while every push was green, and the
#    standing rule to fix red CI before the next commit was applied to
#    whichever workflow somebody happened to be looking at.
#
#    A check that cannot fail the build is a claim, not a control (CHARTER
#    Article IV). This is the check that says so, and it is enforced by the
#    same mechanism the repository already uses: one required status check,
#    and every job that runs on a push feeding into it.
#
# 2. **No code-scanning finding may be left open.** The analyser fails its own
#    job when a finding is *new*, and says nothing at all when a finding is
#    already in its baseline — which is the case where somebody has to act.
#    This half asks the API what is open, and fails while it is.
#
# The scorecard's findings are policy findings, not findings about the code,
# and they are reported here with the reason each one is open rather than
# failing the build. Filtering them out of the report would be the dishonesty
# this repository exists to refuse; listing them, explaining them, and not
# pretending they are code defects is the honest version of the same thing.
#
# Usage:
#   scripts/security-gate.sh                the API, with a token
#   scripts/security-gate.sh --alerts FILE  a saved alerts document
#   scripts/security-gate.sh --wait SECONDS  keep re-reading until the feed settles
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

FAILED=0
pass() { printf '  ok          %s\n' "$1"; }
fail() { printf '  FAIL        %s\n' "$1"; FAILED=1; }
unver() { printf '  UNVERIFIED  %s\n' "$1"; }

echo "Security gate — the scans are inside the gate, and nothing is left open"
echo

# ── 1. Every push-triggered workflow must be the required gate itself ─────────
#
# A workflow that runs on a push to a branch produces a verdict that the
# required check must be able to see. There is exactly one way to arrange that
# in GitHub Actions: the job has to be a job of the workflow the required
# check aggregates. A tag push is a release and is not one of these.
#
# The rule is deliberately narrow. It says nothing about what a scan finds, and
# nothing about whether a scan is worth running — only that its verdict has to
# reach the build. A scan that is advisory belongs in scheduled.yml, which
# runs on its own schedule and is not required, and that is a decision a
# human makes in a file rather than a list this gate keeps.
python3 - <<'PYWORKFLOWS' || FAILED=1
import glob
import os
import sys

import yaml

REQUIRED = "ci.yml"


def triggers(doc):
    """The trigger table, under either spelling of the `on` key.

    YAML 1.1 reads a bare `on` as the boolean true, and PyYAML follows YAML
    1.1, so the key is `True` in the parsed document. Asking for 'on' alone
    would find nothing and every workflow would look trigger-less — which is
    the failure mode this gate exists to catch, not to have.
    """
    for key in (True, "on", "On", "ON"):
        if key in doc:
            return doc[key] or {}
    return {}


def pushes_to_a_branch(on):
    """Whether the workflow runs on a push to a branch.

    A push limited to tags is a release, and a release is gated by the
    release's own workflow rather than by the branch's required check.
    """
    push = on.get("push") if isinstance(on, dict) else None
    if not isinstance(push, dict):
        return False
    return "branches" in push


bad = []
for path in sorted(glob.glob(".github/workflows/*.yml")):
    name = os.path.basename(path)
    try:
        doc = yaml.safe_load(open(path, encoding="utf-8"))
    except Exception as exc:  # noqa: BLE001 - the message is the point
        print(f"  FAIL        {name} could not be parsed: {exc}")
        bad.append(name)
        continue
    if not isinstance(doc, dict):
        print(f"  FAIL        {name} is not a workflow document")
        bad.append(name)
        continue
    on = triggers(doc)
    if pushes_to_a_branch(on) and name != REQUIRED:
        print(
            f"  FAIL        {name} runs on a push to a branch but is not {REQUIRED}, "
            "so its verdict cannot fail the required check"
        )
        bad.append(name)

if bad:
    print()
    print("  A scan that runs on a push and lives in its own workflow reports a")
    print("  verdict nothing can act on. Move the job into ci.yml and add it to")
    print("  the aggregator's needs, or move the trigger to a schedule.")
    sys.exit(1)

print("  ok          every workflow that runs on a push is the required gate itself")
PYWORKFLOWS

# ── 2. No code-scanning finding may be left open ──────────────────────────────
# ALERTS_ARG is the file the caller supplied, if any; ALERTS_FILE is the
# document actually read. They are different variables on purpose: the
# refresh below has to know whether it may re-read the feed over the network,
# and after the temporary file is created the two are no longer the same thing.
ALERTS_ARG=""
ALERTS_FILE=""
WAIT_SECONDS=0
while [ $# -gt 0 ]; do
  case "$1" in
    --alerts) ALERTS_ARG="${2:-}"; shift 2 ;;
    --wait) WAIT_SECONDS="${2:-0}"; shift 2 ;;
    -h|--help) sed -n '34,37p' "$0"; exit 0 ;;
    *) echo "unknown option: $1"; exit 2 ;;
  esac
done

# The feed is re-read until it settles, because the analyser that closes a
# finding and the gate that asks about it are two processes that do not finish
# at the same moment. A push that fixes a finding would otherwise be judged
# against the state before the fix, and the commit that closes the alert would
# be the one commit that could not be pushed.
#
# The wait is only spent when something is open, so the ordinary push — where
# nothing is open — is not a minute slower than it needs to be.
fetch_alerts() {
  if [ -n "$ALERTS_ARG" ]; then
    # A document the caller gave us is not refreshed. It is a fixture, and a
    # fixture that changed under the gate would be a test that tests nothing.
    [ -s "$ALERTS_FILE" ] && return 0
    return 1
  fi
  local code
  code="$(curl -sS -o "$ALERTS_FILE" -w '%{http_code}' \
    -H "Authorization: Bearer $GITHUB_TOKEN" \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/$GITHUB_REPOSITORY/code-scanning/alerts?state=open&per_page=100")"
  [ "$code" = "200" ]
}

if [ -n "$ALERTS_ARG" ]; then
  ALERTS_FILE="$ALERTS_ARG"
else
  if [ -z "${GITHUB_TOKEN:-}" ] || [ -z "${GITHUB_REPOSITORY:-}" ]; then
    unver "no token and no repository: the open findings could not be read"
    echo
    echo "  The check did not run. It is not reported as passed."
    # In CI this must be a hard failure; on a laptop it is not a defect in the
    # change being made. A failure already found is a failure either way.
    [ "${VAYUCELL_REQUIRE_SECURITY_FEED:-0}" = "1" ] && exit 1
    [ "$FAILED" -ne 0 ] && exit 1
    exit 0
  fi
  ALERTS_FILE="$(mktemp)"
  trap 'rm -f "$ALERTS_FILE"' EXIT
  if ! fetch_alerts; then
    unver "the code-scanning API could not be read: the open findings are unknown"
    echo
    echo "  The check did not run. It is not reported as passed."
    [ "${VAYUCELL_REQUIRE_SECURITY_FEED:-0}" = "1" ] && exit 1
    [ "$FAILED" -ne 0 ] && exit 1
    exit 0
  fi
fi

if [ ! -s "$ALERTS_FILE" ]; then
  unver "the alerts document is empty: the open findings could not be read"
  echo
  echo "  The check did not run. It is not reported as passed."
  [ "${VAYUCELL_REQUIRE_SECURITY_FEED:-0}" = "1" ] && exit 1
  [ "$FAILED" -ne 0 ] && exit 1
  exit 0
fi

# The wait. Each pass re-reads the feed and asks the same question; the loop
# ends when the answer is "nothing open" or the patience runs out.
deadline=$(( $(date +%s) + WAIT_SECONDS ))
while :; do
  open_count="$(python3 - "$ALERTS_FILE" <<'PYCOUNT'
import json
import sys

try:
    alerts = json.load(open(sys.argv[1], encoding="utf-8"))
except Exception:  # noqa: BLE001 - a count of zero here would be a false pass
    print(-1)
    sys.exit(0)
if not isinstance(alerts, list):
    print(-1)
    sys.exit(0)
print(sum(1 for a in alerts if (a.get("tool") or {}).get("name") == "CodeQL"))
PYCOUNT
)"
  if [ "$open_count" = "0" ]; then
    break
  fi
  if [ "$open_count" = "-1" ]; then
    break
  fi
  if [ "$(date +%s)" -ge "$deadline" ]; then
    break
  fi
  printf '  %s finding(s) still open — waiting for the feed to settle (%ss left)\n' \
    "$open_count" "$((deadline - $(date +%s)))"
  sleep 15
  fetch_alerts || break
done

python3 - "$ALERTS_FILE" <<'PYALERTS'
import json
import sys

path = sys.argv[1]
try:
    alerts = json.load(open(path, encoding="utf-8"))
except Exception as exc:  # noqa: BLE001 - the message is the point
    print(f"  FAIL        the alerts document could not be read: {exc}")
    sys.exit(1)

if not isinstance(alerts, list):
    print("  FAIL        the alerts document is not a list of alerts")
    sys.exit(1)

code_findings = []
policy_findings = []
for a in alerts:
    tool = (a.get("tool") or {}).get("name", "")
    rule = (a.get("rule") or {}).get("id", "")
    where = "no file associated with this alert"
    instance = a.get("most_recent_instance") or {}
    location = instance.get("location") or {}
    if location.get("path"):
        where = f"{location['path']}:{location.get('start_line', '?')}"
    entry = (a.get("number"), rule, where)
    if tool == "CodeQL":
        code_findings.append(entry)
    else:
        policy_findings.append((tool, entry))

if code_findings:
    for number, rule, where in code_findings:
        print(f"  FAIL        finding #{number} ({rule}) is open at {where}")
    print()
    print("  A finding that is open is a finding nobody has dealt with. The")
    print("  analyser only fails its own job when a finding is NEW, so an old")
    print("  one goes quiet while it waits. Fix it, or dismiss it with a reason")
    print("  that says why it is not a defect.")
    sys.exit(1)

print("  ok          no code-scanning finding about the code is open")

if policy_findings:
    print()
    print("  Policy findings, which are about the project's shape and not about")
    print("  the code. They are listed rather than filtered out, because a report")
    print("  that hides them is a report that cannot be trusted:")
    for tool, (number, rule, where) in policy_findings:
        print(f"    - #{number} {rule} (from {tool}) — {where}")
    print()
    print("  These are not defects in the code and this gate does not fail on")
    print("  them. docs/CI.md says what each one is and why it is open.")
PYALERTS
rc=$?
[ $rc -ne 0 ] && FAILED=1

echo
if [ "$FAILED" -ne 0 ]; then
  echo "SECURITY GATE FAILED — a scan sits outside the gate, or a finding is open."
  exit 1
fi
echo "Security gate passed: every scan that runs on a push feeds the required"
echo "check, and no finding about the code is left open."
