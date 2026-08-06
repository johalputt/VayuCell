#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# Mutation gate: re-break each load-bearing guard and prove the matching test
# goes RED.
#
# A green suite proves the code passes its tests. It does not prove the tests
# would notice if the code were wrong — a test that asserts nothing, or asserts
# the wrong thing, is indistinguishable from a working one until the day it
# matters. This gate closes that gap for the guards where being wrong is a
# safety or an honesty failure rather than a bug.
#
# Every mutation below is applied by exact-string replacement with the match
# count asserted, because a mutation that silently fails to apply reports the
# untouched suite as "survived the mutation" — a false green that is worse than
# no gate at all.
#
# Usage: scripts/mutation-gate.sh
set -euo pipefail

cd "$(dirname "$0")/.."

# The sources are snapshotted to a temp directory and restored from there rather
# than with 'git checkout', so the gate is safe to run on a dirty tree: it can
# never discard uncommitted work, and it does not depend on git at all.
SNAPSHOT="$(mktemp -d)"
cp -a core/src "$SNAPSHOT/src"

# 'cp -a' would preserve the snapshot's original mtimes, leaving the restored
# file OLDER than the object compiled from the mutated source. Cargo fingerprints
# on mtime, so it would skip the rebuild and keep running the mutant — the gate's
# own false-green. The restore therefore stamps a fresh mtime deliberately.
restore() {
  cp -r "$SNAPSHOT/src/." core/src/
  find core/src -type f -exec touch {} +
}
cleanup() { restore; rm -rf "$SNAPSHOT"; }
trap cleanup EXIT

# If the suite is not green to begin with, every mutation would "die" for the
# wrong reason and the gate would report success while proving nothing.
if ! cargo test --offline --quiet >/dev/null 2>&1; then
  echo "refusing to run: the test suite is not green before any mutation is applied,"
  echo "so a red result would not be evidence that the mutation caused it."
  exit 1
fi

FAILED=0

# mutate <file> <expect-red-test> <description> <from> <to> [<from> <to> ...]
#
# Some guards need more than one edit to break coherently: adding an enum
# variant also needs its match arm, or the crate simply fails to compile and
# the "red" result would prove nothing about the guard. Extra from/to pairs
# are applied to the same file, each with its match count asserted separately.
mutate() {
  local file="$1" test_name="$2" desc="$3"; shift 3

  python3 - "$file" "$@" <<'PY'
import sys
path, pairs = sys.argv[1], sys.argv[2:]
if len(pairs) % 2 != 0:
    sys.exit("MUTATION MALFORMED: from/to pairs must come in twos")
with open(path) as f:
    src = f.read()
for frm, to in zip(pairs[0::2], pairs[1::2]):
    n = src.count(frm)
    if n != 1:
        sys.exit(f"MUTATION DID NOT APPLY: {n} matches for {frm!r} in {path}")
    src = src.replace(frm, to)
with open(path, "w") as f:
    f.write(src)
PY

  if cargo test --offline --quiet "$test_name" >/dev/null 2>&1; then
    echo "  SURVIVED  $desc"
    echo "            $test_name still passes with the guard broken."
    FAILED=1
  else
    echo "  killed    $desc"
  fi
  restore
}

echo "Mutation gate — each guard broken, matching test must go red"
echo

T=core/src/tier.rs
H=core/src/host.rs
C=core/src/csp.rs
HD=core/src/headers.rs
B=core/src/battery.rs
G=core/src/governor.rs

mutate "$T" a_guest_that_cannot_see_the_phone_reports_unverified_rather_than_guessing \
  "a bare VM is promoted to T2 without the shell's assertion" \
  'if s.platform == Platform::Virtualised && s.assertion == Assertion::AndroidGuest {' \
  'if s.platform == Platform::Virtualised {'

mutate "$T" a_machine_with_no_recognised_evidence_is_unknown_not_t0 \
  "an unrecognised machine falls back to T0 instead of Unknown" \
  '    Verdict::Unknown
}

/// Android userspace leaves' \
  '    Verdict::Established(Tier::T0)
}

/// Android userspace leaves'

mutate "$T" an_unreadable_device_tree_makes_the_verdict_unverified_not_unknown \
  "an unreadable device tree is reported as absent hardware" \
  'return Silicon::Unreadable;' \
  'return Silicon::NotMobile;'

mutate "$T" an_unrecognised_shell_assertion_is_refused_rather_than_believed \
  "any value of the assertion variable is believed" \
  'format!("{SHELL_ASSERTION_ENV} set to unrecognised value {other:?}"),
            ));
            Assertion::Absent' \
  'format!("{SHELL_ASSERTION_ENV} set to unrecognised value {other:?}"),
            ));
            Assertion::AndroidGuest'

mutate "$T" stock_android_without_root_is_t0 \
  "an unprivileged handset is granted the rooted tier" \
  'Privilege::Unprivileged => Tier::T0,' \
  'Privilege::Unprivileged => Tier::T1,'

mutate "$T" a_guest_that_cannot_see_the_phone_reports_unverified_rather_than_guessing \
  "Unverified leaks a tier through Verdict::tier()" \
  '            Verdict::Established(t) => Some(*t),
            _ => None,' \
  '            Verdict::Established(t) => Some(*t),
            _ => Some(Tier::T0),'

mutate "$T" android_outranks_the_device_tree_so_a_rooted_handset_is_t1_not_t3 \
  "the device tree is consulted before Android userspace" \
  '    if s.userspace == Userspace::Android {' \
  '    if s.silicon == Silicon::Mobile {
        return Verdict::Established(Tier::T3);
    }
    if s.userspace == Userspace::Android {'

mutate "$H" a_status_file_that_will_not_say_who_we_are_is_never_root \
  "a status file that answers nothing reads as root" \
  '.unwrap_or(NOT_ROOT)' \
  '.unwrap_or(0)'

mutate "$H" a_status_file_that_will_not_say_who_we_are_is_never_root \
  "the not-root sentinel is itself root" \
  'pub const NOT_ROOT: u32 = u32::MAX;' \
  'pub const NOT_ROOT: u32 = 0;'

mutate "$H" the_effective_uid_is_read_not_the_real_one \
  "the real uid is read instead of the effective one" \
  '.and_then(|l| l.split_whitespace().nth(2))' \
  '.and_then(|l| l.split_whitespace().nth(1))'

# -- Content Security Policy (ADR-0006) ----------------------------------------

mutate "$C" a_passive_source_cannot_be_smuggled_onto_an_executable_directive \
  "data: and https: are accepted on script-src" \
  "matches!(self, Source::Data | Source::Https)" \
  "false"

mutate "$C" violation_reports_never_leave_the_device \
  "the report endpoint may point off the device" \
  "            return Err(PolicyError::ReportEndpointNotLocal(endpoint.to_owned()));" \
  "            let _unused = PolicyError::ReportEndpointNotLocal(endpoint.to_owned());"

mutate "$C" a_weak_nonce_is_refused_rather_than_rendered \
  "a guessable nonce is accepted" \
  "pub const MIN_LEN: usize = 22;" \
  "pub const MIN_LEN: usize = 1;"

mutate "$C" a_nonce_cannot_carry_a_character_that_escapes_the_directive \
  "a nonce may carry a quote and rewrite the rest of the policy" \
  "            return Err(NonceError::IllegalCharacter);" \
  "            let _unused = NonceError::IllegalCharacter;"

mutate "$C" the_baseline_denies_everything_it_was_not_asked_about \
  "the baseline defaults to self instead of none" \
  '                    name: "default-src",
                    sources: vec![Source::Nothing],' \
  '                    name: "default-src",
                    sources: vec![Source::Own],'

mutate "$C" allowing_a_source_clears_the_none_that_was_there \
  "none survives beside a real source, so the browser drops the whole directive" \
  "sources.retain(|s| *s != Source::Nothing);" \
  "sources.retain(|_s| true);"

mutate "$C" an_origin_outside_the_closed_allowlist_is_refused \
  "any origin is admitted into the policy" \
  "ALLOWED.contains(&origin)" \
  "ALLOWED.contains(&origin) || !origin.is_empty()"

mutate "$C" script_may_run_only_with_the_per_response_nonce \
  "script-src falls back to self, so any same-origin file executes" \
  '.allow("script-src", &[Source::Nonce])' \
  '.allow("script-src", &[Source::Own])'

mutate "$C" a_page_cannot_be_framed_or_have_its_base_rewritten \
  "the page may be framed, enabling clickjacking" \
  '                    name: "frame-ancestors",' \
  '                    name: "frame-ancestors-disabled",'

# The compile_fail proofs are this module's strongest claim: the unsafe keywords
# cannot be written down at all. Putting the variant back must make the proof
# COMPILE, which turns the compile_fail doctest red. Its match arm is added in
# the same mutation, or the crate fails to build for an unrelated reason and the
# red result would prove nothing about the guard.
mutate "$C" --doc \
  "Source gains an unsafe variant and the compile_fail proof still passes" \
  "    Https," \
  "    Https,
    /// Planted by the mutation gate.
    UnsafeInline," \
  '            Source::Https => "https:",' \
  '            Source::Https => "https:",
            Source::UnsafeInline => "unsafe",'

# -- Response security headers (ADR-0006 §6) -----------------------------------

mutate "$HD" the_production_set_enforces_rather_than_reports \
  "a release ships report-only, enforcing nothing while looking identical" \
  "            mode: Mode::Enforce," \
  '            mode: Mode::ReportOnly("shipped by accident".to_owned()),'

mutate "$HD" content_sniffing_is_never_permitted \
  "the browser is allowed to guess a content type" \
  'out.push(("X-Content-Type-Options", "nosniff".to_owned()));' \
  'out.push(("X-Content-Type-Options", "sniff".to_owned()));'

mutate "$HD" the_page_is_refused_to_framers_by_two_independent_mechanisms \
  "the legacy framing refusal is downgraded to same-origin" \
  'out.push(("X-Frame-Options", "DENY".to_owned()));' \
  'out.push(("X-Frame-Options", "SAMEORIGIN".to_owned()));'

mutate "$HD" a_token_hsts_max_age_is_refused_rather_than_sent \
  "a token HSTS max-age is accepted" \
  "pub const MIN_MAX_AGE: u32 = 60 * 60 * 24 * 180;" \
  "pub const MIN_MAX_AGE: u32 = 1;"

mutate "$HD" the_referrer_never_leaks_a_path_to_another_origin \
  "the default referrer policy starts leaking cross-origin" \
  'Referrer::None_ => "no-referrer",' \
  'Referrer::None_ => "unsafe-url",'

mutate "$HD" device_permissions_are_denied_by_enumeration_not_by_omission \
  "device permissions fall back to whatever the browser defaults to" \
  '                "camera",' \
  '                "camera-was-removed",'

mutate "$HD" the_browsing_context_is_isolated \
  "the browsing context stops being isolated" \
  'out.push(("Cross-Origin-Opener-Policy", "same-origin".to_owned()));' \
  'out.push(("Cross-Origin-Opener-Policy", "unsafe-none".to_owned()));'

mutate "$HD" development_sends_no_hsts_because_it_cannot_honour_it \
  "development pins HTTPS from a machine serving plain HTTP" \
  "            hsts: None,
        }
    }

    /// Overrides the referrer policy." \
  "            hsts: Some(Hsts::ONE_YEAR),
        }
    }

    /// Overrides the referrer policy."

# The compile_fail proof: putting the leaky variant back must make it COMPILE,
# which turns the doctest red. The match arm goes in the same mutation, or the
# crate fails to build for an unrelated reason and proves nothing.
mutate "$HD" --doc \
  "Referrer gains a leaking variant and the compile_fail proof still passes" \
  "    StrictOriginWhenCrossOrigin,
}" \
  "    StrictOriginWhenCrossOrigin,
    /// Planted by the mutation gate.
    UnsafeUrl,
}" \
  '            Referrer::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin",' \
  '            Referrer::StrictOriginWhenCrossOrigin => "strict-origin-when-cross-origin",
            Referrer::UnsafeUrl => "unsafe-url",'

# -- The Battery Safety Governor (ADR-0002) ------------------------------------
#
# The most consequential mutations in this repository. Every one of these, left
# broken, is a device that keeps serving in somebody's home when it should have
# stopped.

mutate "$B" the_decidegree_reading_is_not_mistaken_for_degrees \
  "the kernel's decidegrees are read as degrees, so 60.1 C looks like 6 C" \
  "        Celsius(self.0 / 10)" \
  "        Celsius(self.0)"

mutate "$B" an_unmeasurable_state_of_health_is_unknown_not_zero \
  "an unmeasurable state of health collapses into a number" \
  "            return StateOfHealth::Unknown;" \
  "            return StateOfHealth::Measured(Percent::clamped(100));"

mutate "$G" a_cooling_cell_does_not_walk_back_down_on_its_own \
  "the ladder becomes two-way, so cooling clears a hard stop" \
  "        if to <= self.level {
            return None;
        }" \
  "        if to == self.level {
            return None;
        }"

mutate "$G" a_ceiling_that_was_quietly_reverted_is_detected \
  "a reverted ceiling is accepted as held" \
  "        if got > ceiling {" \
  "        if false {"

mutate "$G" a_hardware_ceiling_below_what_was_asked_for_is_satisfying_it \
  "a stricter hardware ceiling is misread as a revert" \
  "        if got > ceiling {" \
  "        if got != ceiling {"

mutate "$G" a_ceiling_that_cannot_be_read_back_is_unverified_never_working \
  "an unreadable ceiling is treated as a working one" \
  "            Err(e) => {
                return self.escalate(Level::Derated, Reason::Unverifiable(e), evidence);
            }" \
  "            Err(_e) => {
                return None;
            }"

mutate "$G" each_temperature_threshold_fires_at_its_own_level \
  "the hard stop fires above the threshold instead of at it" \
  "        if temp >= self.thresholds.hard_stop_temp {" \
  "        if temp > self.thresholds.hard_stop_temp {"

mutate "$G" the_hard_stop_may_be_lowered_but_never_raised \
  "the hard stop can be configured upward" \
  "        if hard_stop_temp > Self::MAX_HARD_STOP {" \
  "        if false {"

mutate "$G" an_unordered_ladder_is_refused_rather_than_silently_unreachable \
  "an unreachable rung is accepted" \
  "        if !(warn_temp < critical_temp && critical_temp < hard_stop_temp) {" \
  "        if false {"

mutate "$G" a_deformed_cell_does_not_recover_whatever_the_sensors_say_next \
  "a cell somebody saw deforming is allowed to resume serving" \
  "            Inspection::Deformed => {
                self.level = Level::Halt;
                Err(self)
            }" \
  "            Inspection::Deformed => {
                self.level = Level::Normal;
                Ok(self)
            }"

mutate "$G" a_degraded_cell_derates_and_a_spent_one_stops_serving \
  "a spent cell keeps serving" \
  "    pub fn may_serve(&self) -> bool {
        self.level <= Level::Derated
    }" \
  "    pub fn may_serve(&self) -> bool {
        self.level <= Level::Protect
    }"

echo
# The suite was green before the first mutation and every mutation was undone,
# so it must be green again. If it is not, the restore is incomplete and every
# "killed" above may have been recorded against contaminated source rather than
# against the mutation named beside it.
if ! cargo test --offline --quiet >/dev/null 2>&1; then
  echo "MUTATION GATE FAILED — the suite is not green after restore, so the results"
  echo "above are not trustworthy. The harness left the tree or the build dirty."
  exit 1
fi

if [ "$FAILED" -ne 0 ]; then
  echo "MUTATION GATE FAILED — a broken guard went unnoticed by the suite."
  exit 1
fi
echo "Mutation gate passed: every guard above is actually load-bearing."
