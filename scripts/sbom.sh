#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Generate a CycloneDX software bill of materials.
#
# The core is required to have no third-party runtime dependencies (ADR-0005
# §5.1, Charter Article V.5), so this SBOM is currently almost empty. That is
# the point of publishing it: an operator can confirm the claim rather than
# take it on trust, and the day the file grows, it grows visibly.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

OUT="${1:-sbom.cdx.json}"

if ! command -v cargo-cyclonedx >/dev/null 2>&1; then
  echo "cargo-cyclonedx is not installed:"
  echo "  cargo install cargo-cyclonedx --locked"
  exit 1
fi

cargo cyclonedx --format json --all --override-filename "${OUT%.json}" || exit 1
echo "wrote $OUT"

python3 - "$OUT" <<'PY'
import json, sys
try:
    doc = json.load(open(sys.argv[1]))
except FileNotFoundError:
    sys.exit(f"{sys.argv[1]} was not produced")
comps = doc.get("components", [])
print(f"  {len(comps)} component(s)")
for c in comps:
    print(f"    {c.get('name')} {c.get('version')}")
PY
