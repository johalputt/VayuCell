<!-- SPDX-License-Identifier: Apache-2.0 -->

## What this changes

<!-- The change, in a sentence or two. -->

## Why

<!-- The problem it solves. If it implements an ADR, link it. -->

## What could go wrong

<!-- Be specific and be honest. "Nothing" is almost never true, and a reviewer
     who is told the risk can check it. If this touches charging, power, storage
     durability or the capability registry, say what happens on the device where
     it misbehaves. -->

## Verification

<!-- What you ran, and what it said. Not what you intend to run. -->

- [ ] `cargo test --workspace` passes
- [ ] `cargo clippy --workspace --all-targets -- -D warnings` is clean
- [ ] `bash scripts/charter-gate.sh` passes
- [ ] `bash scripts/mutation-gate.sh` passes (if this changes a guard)
- [ ] New behaviour has a test whose name states the consequence of it breaking

## Checks that would fail if this change were wrong

<!-- The important one. A change with no failing-first test is a change nobody
     has shown the test suite can detect. Name the test, or say why one is not
     possible here. -->
