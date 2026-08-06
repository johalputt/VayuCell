#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Generate a CycloneDX software bill of materials.
#
# The core is required to have no third-party runtime dependencies (ADR-0005
# §5.1, Charter Article V.5), so this SBOM is currently almost empty. That is
# the point of publishing it: an operator can confirm the claim rather than take
# it on trust, and the day the file grows, it grows visibly.
#
# On finding the output: cargo-cyclonedx writes its file next to each package's
# Cargo.toml, not at the workspace root. The first version of this script printed
# "wrote sbom.cdx.json" on the strength of cargo's exit code while the file was
# somewhere else entirely. The assertion at the end caught it, which is the only
# reason a broken SBOM job was not published as a working one.
#
# So this script does not report where it thinks the file went. It reports where
# the file actually is.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

OUT="${1:-sbom.cdx.json}"

if ! command -v cargo-cyclonedx >/dev/null 2>&1; then
  echo "cargo-cyclonedx is not installed:"
  echo "  cargo install cargo-cyclonedx --locked"
  exit 1
fi

# Anything already present is not evidence that this run produced it.
BEFORE="$(mktemp)"
AFTER="$(mktemp)"
trap 'rm -f "$BEFORE" "$AFTER"' EXIT
find . -name '*.cdx.json' -not -path './target/*' 2>/dev/null | sort > "$BEFORE"

cargo cyclonedx --format json --all || exit 1

find . -name '*.cdx.json' -not -path './target/*' 2>/dev/null | sort > "$AFTER"

mapfile -t PRODUCED < <(comm -13 "$BEFORE" "$AFTER")

# A rerun overwrites rather than creates, which is not a failure.
if [ "${#PRODUCED[@]}" -eq 0 ]; then
  mapfile -t PRODUCED < <(comm -12 "$BEFORE" "$AFTER")
fi

if [ "${#PRODUCED[@]}" -eq 0 ]; then
  echo "cargo-cyclonedx exited successfully but produced no .cdx.json file."
  echo "Its output location has changed. Do not paper over this by touching a file."
  exit 1
fi

echo "  produced:"
printf '    %s\n' "${PRODUCED[@]}"

# One workspace member today. If that changes, merging them is a real decision
# and should be made deliberately rather than by this script picking one.
if [ "${#PRODUCED[@]}" -gt 1 ]; then
  echo
  echo "more than one SBOM was produced; merging them is a decision this script"
  echo "does not make on its own."
  exit 1
fi

[ "${PRODUCED[0]}" = "./$OUT" ] || mv "${PRODUCED[0]}" "$OUT"

python3 - "$OUT" <<'PY'
import json
import sys

path = sys.argv[1]
try:
    doc = json.load(open(path))
except FileNotFoundError:
    sys.exit(f"{path} was not produced")
except json.JSONDecodeError as exc:
    sys.exit(f"{path} is not valid JSON: {exc}")

comps = doc.get("components", [])
print(f"  {path}: {len(comps)} component(s)")
for c in comps:
    print(f"    {c.get('name')} {c.get('version')}")
PY
