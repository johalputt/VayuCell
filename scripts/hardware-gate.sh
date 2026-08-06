#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Hardware database gate.
#
# hardware/schema.json defines a device compatibility record, and every file in
# hardware/devices/ must validate against it. The database is the thing an
# operator consults before trusting a phone with their mail, so a malformed or
# over-claiming record is a safety problem, not a formatting one.
#
# On the validator itself: when the JSON Schema library is unavailable this gate
# reports the schema check as UNVERIFIED and says so. It does not print a tick.
# Article IV binds the project's own toolchain exactly as it binds a device
# report — a check that did not run may not be displayed as a check that passed.
# Set VAYUCELL_REQUIRE_SCHEMA_VALIDATOR=1 (CI does) to make a missing validator a
# hard failure, so the authoritative run can never silently skip it.
#
# Usage: scripts/hardware-gate.sh
set -uo pipefail

# Without -e a failed cd would leave the gate running against whatever
# directory it was invoked from, where empty file lists make several checks
# pass trivially. That is a false green, so it exits instead.
cd "$(dirname "$0")/.." || exit 1

FAILED=0
pass() { printf '  ok          %s\n' "$1"; }
fail() { printf '  FAIL        %s\n' "$1"; FAILED=1; }
unver() { printf '  UNVERIFIED  %s\n' "$1"; }

echo "Hardware database gate"
echo

# ── Every file is well-formed JSON ────────────────────────────────────────────
json_ok=1
for f in hardware/schema.json hardware/devices/*.json; do
  if ! python3 -c "import json,sys; json.load(open(sys.argv[1]))" "$f" 2>/dev/null; then
    fail "not valid JSON: $f"
    json_ok=0
  fi
done
[ "$json_ok" = "1" ] && pass "every file in hardware/ is well-formed JSON"

# ── The schema validates against its own metaschema, and profiles against it ──
if python3 -c "import jsonschema" 2>/dev/null; then
  python3 - <<'PY'
import glob, json, sys
import jsonschema

schema = json.load(open("hardware/schema.json"))
cls = jsonschema.validators.validator_for(schema)
try:
    cls.check_schema(schema)
    print("  ok          hardware/schema.json is a valid JSON Schema")
except jsonschema.SchemaError as e:
    print(f"  FAIL        hardware/schema.json is not a valid schema: {e.message}")
    sys.exit(1)

validator = cls(schema)
bad = 0
for path in sorted(glob.glob("hardware/devices/*.json")):
    errors = sorted(validator.iter_errors(json.load(open(path))), key=lambda e: list(e.path))
    if errors:
        bad = 1
        print(f"  FAIL        {path}")
        for e in errors:
            loc = "/".join(str(p) for p in e.path) or "(root)"
            print(f"                {loc}: {e.message}")
    else:
        print(f"  ok          {path} validates against the schema")
sys.exit(bad)
PY
  [ $? -ne 0 ] && FAILED=1
else
  unver "schema validation did not run: python3 -m pip install jsonschema"
  if [ "${VAYUCELL_REQUIRE_SCHEMA_VALIDATOR:-0}" = "1" ]; then
    fail "the validator is required in this environment and is not installed"
  fi
fi

# ── Honesty rules the schema cannot express ───────────────────────────────────
# A record claiming a verified charge limit must name the sysfs node the claim
# was read back from. "It worked" with no path is the same unverifiable assertion
# Article IV forbids everywhere else.
python3 - <<'PY'
import glob, json, sys

bad = 0
checked = 0

def flag(path, msg):
    global bad
    print(f"  FAIL        {path}: {msg}")
    bad = 1

for path in sorted(glob.glob("hardware/devices/*.json")):
    d = json.load(open(path))
    battery = d.get("battery", {})
    cc = battery.get("charge_control", {})
    storage = d.get("storage", {})
    checked += 1

    # ADR-0002 and Article IV.5: a ceiling that was "verified to hold" must name
    # the node that answered. Without the path the claim cannot be reproduced by
    # anyone else, and an unreproducible safety claim is the exact shape of the
    # thing this project refuses to print.
    if cc.get("verified_hold") is True and not cc.get("node_path"):
        flag(path, "battery.charge_control.verified_hold is true but node_path is empty")

    # Article IV.2: absence is never protection. A record cannot claim a working
    # mechanism while reporting the capability as unavailable.
    if cc.get("available") is False and cc.get("mechanism") not in (None, "none"):
        flag(path, f"charge_control.available is false but mechanism is {cc.get('mechanism')!r}")
    if cc.get("available") is True and cc.get("mechanism") == "none":
        flag(path, "charge_control.available is true but mechanism is 'none'")

    # A ceiling cannot be verified to hold on a device that has no mechanism.
    if cc.get("verified_hold") is True and cc.get("available") is not True:
        flag(path, "charge_control.verified_hold is true while available is not true")

    # A device that reports a tier as achieved must say how. ADR-0001: positive
    # evidence only, in the database exactly as in the detector.
    tiers = d.get("tiers", {})
    for key in ("t1_root", "t2_virtualisation", "t3_mainline"):
        probe = tiers.get(key)
        if isinstance(probe, dict) and probe.get("result") == "present" and not probe.get("detail"):
            flag(path, f"tiers.{key} is present but records no detail")

    # The storage claim must be chosen, never defaulted by omission.
    if storage and "durability_class" not in storage:
        flag(path, "storage is present but durability_class is omitted")

if not bad:
    print(f"  ok          {checked} profile(s): every verified claim names its evidence")
sys.exit(bad)
PY
[ $? -ne 0 ] && FAILED=1

echo
if [ "$FAILED" -ne 0 ]; then
  echo "HARDWARE GATE FAILED."
  exit 1
fi
echo "Hardware gate passed."
