#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Build and test against the declared MSRV, the way ci.yml does.
#
# Declaring a rust-version and only ever building against a much newer compiler
# is a claim nothing checks. The gates on a developer machine run whatever
# stable is installed there — years ahead of the MSRV — and cheerfully accept
# things the MSRV rejects. `Option::expect` in a const context is one: stable to
# call since 1.83, and a compile error at the declared 1.80. It reached `main`
# and the MSRV job in CI was the only thing that noticed.
#
# RUSTFLAGS is cleared for the same reason ci.yml clears it: an older toolchain
# emits lints the current one has since renamed or removed, and failing the
# build on them would be testing the compiler rather than the code.
set -uo pipefail

cd "$(dirname "$0")/.." || exit 1

msrv="$(grep -m1 '^rust-version' core/Cargo.toml | cut -d'"' -f2)"
if [ -z "$msrv" ]; then
  echo "core/Cargo.toml declares no rust-version, so there is no MSRV to check"
  exit 1
fi

if ! rustup toolchain list 2>/dev/null | grep -q "^$msrv"; then
  # Not silently passing. A gate that reports success for a check it never ran
  # is the exact failure this project refuses everywhere else.
  echo "NOT CHECKED: the declared MSRV ($msrv) is not installed."
  echo "Install it with:  rustup toolchain install $msrv --profile minimal"
  echo "Until then this build has not been shown to work on the version it claims."
  exit 1
fi

echo "Building and testing against the declared MSRV, $msrv"
RUSTFLAGS='' RUSTDOCFLAGS='' cargo "+$msrv" build --workspace --all-features || exit 1
RUSTFLAGS='' RUSTDOCFLAGS='' cargo "+$msrv" test --workspace --all-features || exit 1
echo "MSRV gate passed: the crate builds and tests on $msrv."
