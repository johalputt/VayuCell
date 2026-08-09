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
# EVERY crate's sources, not just core/. This gate previously snapshotted
# core/src by name; the moment a mutation named a file in cli/src it was applied
# and never restored, and the crate stayed mutated on disk. The "is the suite
# green after restore" assertion at the bottom is what caught it — which is the
# only reason that assertion exists, and the reason it is worth its runtime.
SNAPSHOT="$(mktemp -d)"
CRATES=()
while IFS= read -r d; do CRATES+=("$d"); done < <(
  find . -mindepth 2 -maxdepth 2 -type d -name src -not -path './target/*' | sed 's|^\./||' | sort
)
if [ "${#CRATES[@]}" -eq 0 ]; then
  echo "refusing to run: found no crate sources to snapshot."
  exit 1
fi
for c in "${CRATES[@]}"; do
  mkdir -p "$SNAPSHOT/$c"
  cp -a "$c/." "$SNAPSHOT/$c/"
done

# 'cp -a' would preserve the snapshot's original mtimes, leaving the restored
# file OLDER than the object compiled from the mutated source. Cargo fingerprints
# on mtime, so it would skip the rebuild and keep running the mutant — the gate's
# own false-green. The restore therefore stamps a fresh mtime deliberately.
restore() {
  for c in "${CRATES[@]}"; do
    cp -r "$SNAPSHOT/$c/." "$c/"
    find "$c" -type f -exec touch {} +
  done
}
cleanup() { restore; rm -rf "$SNAPSHOT"; }

# INT and TERM as well as EXIT. Bash does not run an EXIT trap for every signal
# that ends the shell, so a Ctrl-C partway through would leave whichever file was
# mutated at that moment sitting on disk, looking like source somebody wrote.
#
# SIGKILL still cannot be caught, and that is not hypothetical — a run killed by
# the harness during this session left a mutation behind. The defence for that
# case is the check below: the suite must be green BEFORE the first mutation, so
# a leaked mutation from a previous run refuses the next one rather than being
# mistaken for the code under test.
trap cleanup INT TERM EXIT

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
SF=core/src/sysfs.rs
SM=core/src/sampler.rs
SH=core/src/shed.rs
P=core/src/panel.rs
RT=core/src/runtime.rs
AR=cli/src/args.rs
MN=cli/src/main.rs
RP=cli/src/report.rs
DU=core/src/durability.rs
IN=core/src/ingress.rs
SV=core/src/serve.rs
ST=core/src/site.rs
VA=core/src/vault.rs
AU=core/src/auth.rs
LI=cli/src/listen.rs
EN=cli/src/enrol.rs
CL=cli/src/cell.rs
HA=core/src/halt.rs
HR=cli/src/halted.rs
SY=cli/src/survey.rs
WR=core/src/wear.rs
SG=cli/src/storage.rs

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
  "        if self.charge_full_design_uah <= 0 || self.charge_full_uah < 0 {
            return StateOfHealth::Unknown;
        }" \
  "        if self.charge_full_design_uah <= 0 || self.charge_full_uah < 0 {
            return StateOfHealth::Measured(Percent::clamped(100));
        }"

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

# -- The power-supply sysfs layer (ADR-0002 §2, §3) ----------------------------

mutate "$SF" a_missing_node_refuses_the_reading_and_names_itself \
  "a missing node is defaulted instead of refusing the reading" \
  "        .ok_or(ReadError::Missing { node })?;" \
  '        .unwrap_or_else(|| "0".to_string());'

mutate "$SF" only_the_threshold_node_is_treated_as_a_ceiling \
  "a current limit is presented as a charge ceiling" \
  "        matches!(self, Kind::EndThreshold)" \
  "        true"

mutate "$SF" a_non_ceiling_mechanism_cannot_be_bound_as_one \
  "a non-ceiling node is bound as a ceiling anyway" \
  "        if !kind.is_ceiling() {
            return None;
        }" \
  "        if false {
            return None;
        }"

mutate "$SF" the_mainline_node_is_preferred_over_the_vendor_ones \
  "a vendor node outranks the mainline one" \
  "pub const PROBE_ORDER: [Kind; 4] = [
    Kind::EndThreshold,
    Kind::VoltageMax," \
  "pub const PROBE_ORDER: [Kind; 4] = [
    Kind::InputSuspend,
    Kind::VoltageMax,"

mutate "$SF" a_device_with_no_charge_node_has_no_mechanism_and_that_is_not_an_error \
  "a device with no charge node is given one anyway" \
  "    PROBE_ORDER
        .into_iter()
        .find(|k| host.exists(&format!(\"{dir}/{}\", k.node())))" \
  "    let _ = host;
    let _ = dir;
    Some(Kind::EndThreshold)"

mutate "$SF" verification_reads_the_hardware_not_what_we_remember_writing \
  "verify reports the request instead of reading the hardware" \
  "        let raw = self
            .host
            .read(&self.path)" \
  "        let raw = Some(\"60\".to_string())
            .as_ref()
            .map(std::string::ToString::to_string)"

mutate "$SM" a_cool_idle_cell_is_left_alone \
  "the monitor keeps the device awake polling a cool idle cell" \
  "            Cadence::Steady => Duration::from_secs(30)," \
  "            Cadence::Steady => Duration::from_secs(5),"

mutate "$SM" approaching_a_threshold_tightens_the_cadence_before_it_is_crossed \
  "the approach margin is dropped, so watching starts only after a crossing" \
  "    pub const ALERT_MARGIN: Celsius = Celsius::new(5);" \
  "    pub const ALERT_MARGIN: Celsius = Celsius::new(0);"

mutate "$SM" the_alert_band_follows_the_lowest_rung_rather_than_a_hardcoded_temperature \
  "the alert band is pinned to the hard stop instead of the lowest rung" \
  "        let lowest = warn.min(critical).min(hard_stop);" \
  "        let _ = warn.min(critical);
        let lowest = hard_stop;"

mutate "$SM" a_charging_cell_is_watched_closely_however_cool_it_is \
  "a charging cell is sampled at the idle cadence" \
  "        if reading.is_charging() {
            return Cadence::Alert;
        }" \
  "        if false {
            return Cadence::Alert;
        }"

mutate "$SM" a_device_that_cannot_be_read_is_watched_more_closely_not_less \
  "a device that cannot be read is backed off from instead of watched" \
  "    pub const fn cadence_when_unreadable() -> Cadence {
        Cadence::Alert
    }" \
  "    pub const fn cadence_when_unreadable() -> Cadence {
        Cadence::Steady
    }"

mutate "$G" a_governor_that_has_gone_blind_says_so_rather_than_reporting_health \
  "a governor that has gone blind never says so" \
  "    pub const BLIND_TOLERANCE: u32 = 3;" \
  "    pub const BLIND_TOLERANCE: u32 = u32::MAX;"

mutate "$G" a_reading_that_arrives_is_the_only_thing_that_clears_the_blind_counter \
  "the blind counter is never cleared, so old failures accumulate forever" \
  "        self.consecutive_failures = 0;" \
  "        let _ = self.consecutive_failures;"

mutate "$SH" a_late_tick_hands_back_every_rung_it_skipped_over \
  "a late tick reports the final rung and drops the ones it passed" \
  "        let mut entered = Vec::new();
        for rung in [" \
  "        let mut entered = Vec::new();
        for rung in if true { [Stage::ShuttingDown, target, target, target] } else ["

mutate "$SH" the_ladder_never_walks_back_up_on_its_own \
  "a clock that steps backwards reopens a quiesced database" \
  "            if rung > self.stage && rung <= target {" \
  "            if rung != self.stage && rung <= target {"

mutate "$SH" reaching_the_reserve_shuts_down_however_little_time_has_passed \
  "the timings become a minimum a low cell must still spend" \
  "            Charge::Measured(p) if *p <= self.plan.reserve => (" \
  "            Charge::Measured(p) if *p <= self.plan.reserve && elapsed >= self.plan.quiesce_after => ("

mutate "$SH" the_reserve_is_a_floor_the_node_shuts_down_holding_not_one_it_spends \
  "the reserve is spent down to rather than stopped at" \
  "            Charge::Measured(p) if *p <= self.plan.reserve => (
                Stage::ShuttingDown," \
  "            Charge::Measured(p) if *p < self.plan.reserve => (
                Stage::ShuttingDown,"

mutate "$SH" a_cell_that_cannot_be_read_during_an_outage_is_treated_as_empty \
  "an unreadable cell during an outage is ridden out on optimism" \
  "            Charge::Unreadable(why) => (Stage::ShuttingDown, ShedReason::Unmeasurable(why.clone()))," \
  "            Charge::Unreadable(why) => (
                self.plan.stage_for_elapsed(elapsed),
                ShedReason::Unmeasurable(why.clone()),
            ),"

mutate "$SH" the_reserve_may_be_raised_but_never_lowered \
  "the shutdown reserve can be configured downward" \
  "        if reserve < Self::MIN_RESERVE {
            return Err(PlanError::ReserveTooLow);
        }" \
  "        if false {
            return Err(PlanError::ReserveTooLow);
        }"

mutate "$SH" a_plan_that_quiesces_before_it_sheds_is_refused \
  "a ladder that flushes the database while services still write to it" \
  "        if quiesce_after <= shed_after {" \
  "        if false && quiesce_after <= shed_after {"

mutate "$SH" mains_returning_after_the_database_was_closed_does_not_silently_reopen_it \
  "a flickering supply silently restarts a closed database" \
  "        if self.stage == Stage::Announced {" \
  "        if self.stage != Stage::Serving {"

mutate "$SH" a_node_with_no_cell_does_not_claim_a_ups_and_does_not_pretend_to_ride_it_out \
  "a node with no battery presents a shed ladder it has no energy to run" \
  "        if !self.has_cell {
            return self.walk_to(Stage::ShuttingDown, &ShedReason::NoUps);
        }" \
  "        if false {
            return self.walk_to(Stage::ShuttingDown, &ShedReason::NoUps);
        }"

mutate "$SH" time_alone_never_reaches_shutdown \
  "a node still holding 70% is shut down on a timer" \
  "        if elapsed >= self.quiesce_after {
            Stage::Quiesced" \
  "        if elapsed >= self.quiesce_after {
            Stage::ShuttingDown"

mutate "$P" one_unverified_row_takes_the_headline_off_protected \
  "an unverified row is counted as protection" \
  "                Finding::Unverified(_) => Overall::Unverified," \
  "                Finding::Unverified(_) => Overall::Protected,"

mutate "$P" a_failure_outranks_an_unverified_row \
  "a confirmed failure is filed as a paperwork problem" \
  "pub enum Overall {
    /// Everything on the panel was checked and holds.
    Protected,
    /// Something could not be checked. Not a failure, and not protection.
    Unverified,
    /// Something was checked and does not hold.
    Unsafe,
}" \
  "pub enum Overall {
    /// Everything on the panel was checked and holds.
    Protected,
    /// Something was checked and does not hold.
    Unsafe,
    /// Something could not be checked. Not a failure, and not protection.
    Unverified,
}"

mutate "$P" a_row_cannot_be_built_on_blank_evidence \
  "a row is built on blank evidence and renders as a confident claim" \
  "        if what.trim().is_empty() {
            return None;
        }" \
  "        if false {
            return None;
        }"

mutate "$P" a_device_with_no_charge_control_says_so_rather_than_omitting_the_row \
  "a device with no charge control gets a green mechanism row" \
  "                None => Finding::Refused(evidence(
                    \"this device exposes no charge control, so no ceiling can be held\",
                ))," \
  "                None => Finding::Verified(evidence(
                    \"this device exposes no charge control, so no ceiling can be held\",
                )),"

mutate "$P" a_governor_that_has_left_normal_is_never_a_verified_row \
  "a derated or halted governor still renders as a verified row" \
  "                other => Finding::Refused(evidence(&format!(" \
  "                other => Finding::Verified(evidence(&format!("

mutate "$P" the_inspection_instruction_appears_at_every_risk_level_including_nominal \
  "the inspection prompt is dropped when the estimate looks nominal" \
  "        let _ = write!(out, \"\\n{INSPECTION}\\n\");" \
  "        if self.risk.level != RiskLevel::Nominal {
            let _ = write!(out, \"\\n{INSPECTION}\\n\");
        }"

mutate "$P" the_swelling_estimate_is_rendered_as_an_estimate_and_never_as_a_measurement \
  "the swelling estimate loses the words that stop it reading as a measurement" \
  "            \"\\nSwelling risk: {:?}, {} — an estimate from {basis}, not a measurement.\\n\"," \
  "            \"\\nSwelling risk: {:?} ({} / {basis})\\n\","

mutate "$P" an_estimate_resting_on_nothing_says_so_rather_than_rendering_an_empty_list \
  "an estimate resting on no proxies renders as a clean bill" \
  "            \"no proxies at all\".to_owned()" \
  "            \"the available signals\".to_owned()"

mutate "$P" a_node_with_no_cell_is_not_credited_with_an_outage_reserve \
  "a node with no cell is credited with an outage reserve" \
  "                UpsClaim::Unbacked { why } => Finding::Refused(evidence(why))," \
  "                UpsClaim::Unbacked { why } => Finding::Verified(evidence(why)),"

mutate "$P" the_rendered_panels_match_the_committed_snapshot \
  "the panel's wording is softened without any assertion breaking" \
  "            Overall::Unsafe => \"UNSAFE\"," \
  "            Overall::Unsafe => \"NEEDS ATTENTION\","

mutate "$RT" a_device_that_cannot_be_read_still_produces_a_full_outcome \
  "an unreadable tick backs the cadence off instead of tightening it" \
  "                let cadence = Sampler::cadence_when_unreadable();" \
  "                let cadence = Cadence::Steady;"

mutate "$RT" three_unreadable_ticks_derate_the_device_through_the_loop \
  "the loop never tells the governor it went blind" \
  "                let transition = self.governor.observe_unreadable(&e.to_string());" \
  "                let transition = None;"

mutate "$RT" a_reverted_ceiling_is_caught_on_the_tick_that_wrote_it \
  "the loop stops enforcing the ceiling it was given" \
  "                let mut transition =
                    mechanism.and_then(|m| self.governor.enforce(m, self.ceiling, &reading));" \
  "                let mut transition = None;
                let _ = mechanism;"

mutate "$RT" a_hot_device_escalates_on_the_tick_that_read_it \
  "the loop reads the cell and never shows the governor the reading" \
  "                transition = transition.or_else(|| self.governor.observe(&reading));" \
  "                transition = transition.or(None);"

mutate "$RT" an_outage_on_a_cell_that_stopped_answering_shuts_down_rather_than_riding_it_out \
  "an unreadable cell during an outage is reported as charged" \
  "                let shed = self.advance_shed(power, &Charge::Unreadable(e.to_string()));" \
  "                let shed = self.advance_shed(power, &Charge::Measured(Percent::clamped(100)));"

mutate "$RT" mains_returning_after_the_database_was_closed_does_not_silently_reopen_it \
  "mains returning walks the ladder back up from any rung" \
  "            Power::Mains => {
                self.shed.restored();
                Vec::new()
            }" \
  "            Power::Mains => {
                self.shed = Shed::new(ShedPlan::recommended());
                Vec::new()
            }"

mutate "$AR" a_ceiling_outside_the_range_is_refused_rather_than_clamped \
  "a ceiling of 200 is clamped to 100, which holds no ceiling at all" \
  "                    .filter(|c| *c <= 100)" \
  "                    .map(|c| c.min(100))"

mutate "$AR" a_flag_with_no_value_is_refused_rather_than_falling_back_to_the_default \
  "a flag with no value falls back to the default path" \
  "        .filter(|v| !v.starts_with(\"--\"))" \
  "        .filter(|_| true)"

mutate "$AR" two_commands_are_refused_rather_than_last_one_winning \
  "a second command silently replaces the first" \
  "    if slot.is_some() {
        return Err(ArgError(format!(
            \"only one command at a time; {name:?} came after another\"
        )));
    }" \
  "    if false {
        return Err(ArgError(format!(
            \"only one command at a time; {name:?} came after another\"
        )));
    }"

mutate "$RP" the_exit_code_distinguishes_unmeasured_from_failed \
  "an unmeasured device and a failed one exit with the same code" \
  "        Overall::Unverified => EXIT_UNVERIFIED," \
  "        Overall::Unverified => EXIT_UNSAFE,"

mutate "$RP" a_present_ceiling_node_is_not_reported_verified_before_anything_was_written \
  "a detected ceiling node is reported verified before anything was written" \
  "            Some(k) if k.is_ceiling() => Finding::Unverified(evidence(&format!(
                \"{} is present and holds a percentage; run \`vayucell run\` to \\
                 write {}% and read it back\"," \
  "            Some(k) if k.is_ceiling() => Finding::Verified(evidence(&format!(
                \"{} is present and holds a percentage; run \`vayucell run\` to \\
                 write {}% and read it back\","

mutate "$RP" a_machine_that_is_not_a_phone_reports_unverified_rather_than_crashing_or_passing \
  "an unreadable cell is credited with an outage reserve anyway" \
  "        Err(_) => UpsClaim::Unbacked {
            why: \"the cell could not be read, so nothing is known to be carrying this node\",
        }," \
  "        Err(_) => UpsClaim::Backed { reserve: ceiling },"

mutate "$DU" an_unreachable_replica_is_not_filtered_out_as_noise \
  "an unreachable replica is filtered out as noise" \
  "            Self::NeverReplicated | Self::Unreachable(_) | Self::NoReplica => true," \
  "            Self::Unreachable(_) => false,
            Self::NeverReplicated | Self::NoReplica => true,"

mutate "$MN" the_ladders_last_rung_stops_the_node \
  "the node announces the shutdown and goes on ticking until the cell is flat" \
  "    rungs.iter().any(|r| r.stage == Stage::ShuttingDown)" \
  "    let _ = rungs;
    false"

mutate "$MN" a_rung_reached_on_the_way_down_does_not_stop_the_node \
  "a mains blip stops the node at the first rung instead of riding it out" \
  "    rungs.iter().any(|r| r.stage == Stage::ShuttingDown)" \
  "    !rungs.is_empty()"

mutate "$MN" the_last_rung_is_found_among_the_ones_walked_with_it \
  "a late tick walks to the reserve and only the first rung is looked at" \
  "    rungs.iter().any(|r| r.stage == Stage::ShuttingDown)" \
  "    rungs.first().is_some_and(|r| r.stage == Stage::ShuttingDown)"

mutate "$AR" every_command_that_serves_traffic_is_named_as_one \
  "a halted phone serves a website and accepts uploads after a restart" \
  "            Self::Site | Self::Vault | Self::All | Self::Run { .. } => true,
            Self::Serve" \
  "            Self::All | Self::Run { .. } => true,
            Self::Site
            | Self::Vault
            | Self::Serve"

mutate "$AR" the_panel_is_the_one_surface_a_halt_does_not_take_away \
  "a halt takes away the panel the person needs to read" \
  "            Self::Serve
            | Self::Status" \
  "            Self::Status"

mutate "$AR" nothing_that_only_reads_or_prints_is_gated_by_a_halt \
  "a halted phone will not let anybody record that they looked at it" \
  "            | Self::Inspect
            | Self::Report" \
  "            | Self::Report"

mutate "$WR" a_range_is_reported_as_its_worse_end \
  "a wear range is reported at its kinder end" \
  "        Some(step) => WearIndicator::Readable(step.saturating_mul(10))," \
  "        Some(step) => WearIndicator::Readable(step.saturating_sub(1).saturating_mul(10)),"

mutate "$WR" the_worse_of_the_two_cell_types_is_the_answer \
  "the better-wearing cell type is reported and the worse one discarded" \
  "        worst = Some(worst.map_or(step, |w: u8| w.max(step)));" \
  "        worst = Some(worst.map_or(step, |w: u8| w.min(step)));"

mutate "$WR" a_device_that_declines_to_estimate_is_not_reported_as_new \
  "a device declining to estimate is read as brand new flash" \
  "        if step == DECLINES_TO_SAY {
            continue;
        }" \
  "        if false {
            continue;
        }"

mutate "$WR" a_device_past_its_rated_life_reads_as_a_hundred_and_not_as_more \
  "a device past its rated life is refused instead of reported at a hundred" \
  "        Some(PAST_RATED_LIFE) => WearIndicator::Readable(100)," \
  "        Some(PAST_RATED_LIFE) => WearIndicator::Unreliable(String::new()),"

mutate "$WR" a_node_that_does_not_parse_is_unreliable_rather_than_absent \
  "a node that does not parse is reported as no node at all" \
  "            return WearIndicator::Unreliable(format!(\"{field:?} is not a life-time estimate\"));" \
  "            return WearIndicator::Absent;"

mutate "$SG" a_shed_ladder_nobody_has_watched_is_not_credited_here_either \
  "the producer credits a shed ladder nobody has watched complete" \
  "        graceful_shutdown: GracefulShutdown::NeverObserved," \
  "        graceful_shutdown: GracefulShutdown::Verified,"

mutate "$SG" a_cell_with_no_replicator_says_it_is_the_only_copy \
  "a cell with no replicator reports a lag instead of no replica" \
  "        recovery_point: RecoveryPoint::NoReplica," \
  "        recovery_point: RecoveryPoint::Behind {
            lag: core::time::Duration::ZERO,
            measured_at: core::time::Duration::ZERO,
        },"

mutate "$SG" a_device_that_exposes_no_wear_node_says_absent_rather_than_omitting_the_line \
  "an absent wear node is left out of the report entirely" \
  "            \"  wear       ABSENT   this device exposes no life-time estimate\".to_owned()" \
  "            String::new()"

mutate "$DU" a_lag_nobody_has_re_measured_stops_being_a_live_figure \
  "a lag nobody has re-measured goes on reading as no concern" \
  "                if !self.is_live(now) {
                    return true;
                }" \
  "                if false {
                    return true;
                }"

mutate "$DU" a_lag_nobody_has_re_measured_stops_being_a_live_figure \
  "a measurement stands for a century, so nothing ever goes stale" \
  "pub const MEASUREMENT_STANDS_FOR: Duration = Duration::from_secs(5 * 60);" \
  "pub const MEASUREMENT_STANDS_FOR: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);"

mutate "$DU" a_lag_measured_a_moment_ago_is_still_live \
  "a measurement is stale the instant it is taken" \
  "pub const MEASUREMENT_STANDS_FOR: Duration = Duration::from_secs(5 * 60);" \
  "pub const MEASUREMENT_STANDS_FOR: Duration = Duration::from_secs(0);"

mutate "$DU" a_measurement_stamped_ahead_of_the_clock_is_not_a_live_figure \
  "a measurement the clock cannot account for reads as a live one" \
  "                Some(age) => age.checked_sub(MEASUREMENT_STANDS_FOR).is_none(),
                None => false," \
  "                Some(age) => age.checked_sub(MEASUREMENT_STANDS_FOR).is_none(),
                None => true,"

mutate "$DU" a_stale_lag_reaches_the_operator_through_the_posture_too \
  "the panel checks the lag against its target and not against its age" \
  "        if self
            .recovery_point
            .needs_attention(lag_target, now.since_start)
        {" \
  "        if !matches!(self.recovery_point, RecoveryPoint::Behind { lag, .. } if lag <= lag_target) {"

mutate "$DU" a_backup_nobody_has_restored_is_never_proven \
  "a backup that was merely written counts as proven" \
  "            (Self::Restored { .. }, None)
            | (Self::NeverRestored | Self::RestoreFailed(_) | Self::NotConfigured, _) => false," \
  "            (Self::Restored { .. }, None) => false,
            (Self::NeverRestored | Self::RestoreFailed(_), _) => true,
            (Self::NotConfigured, _) => false,"

mutate "$DU" a_restore_drill_from_last_year_no_longer_proves_anything \
  "a restore drill proves the backup for the next century" \
  "pub const DRILL_STANDS_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);" \
  "pub const DRILL_STANDS_FOR: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);"

mutate "$DU" a_restore_drill_from_yesterday_still_proves_something \
  "a restore drill is worthless the instant it completes" \
  "pub const DRILL_STANDS_FOR: Duration = Duration::from_secs(30 * 24 * 60 * 60);" \
  "pub const DRILL_STANDS_FOR: Duration = Duration::from_secs(0);"

mutate "$DU" a_cell_that_cannot_tell_what_day_it_is_cannot_call_a_drill_current \
  "a cell that cannot date a drill calls it current anyway" \
  "            (Self::Restored { at_unix }, Some(now)) => match now.checked_sub(*at_unix) {" \
  "            (Self::Restored { .. }, None) => true,
            (Self::Restored { at_unix }, Some(now)) => match now.checked_sub(*at_unix) {"

mutate "$DU" a_drill_stamped_ahead_of_the_clock_is_not_evidence \
  "a drill the clock cannot account for reads as a fresh one" \
  "                Some(age) => age <= DRILL_STANDS_FOR.as_secs(),
                None => false," \
  "                Some(age) => age <= DRILL_STANDS_FOR.as_secs(),
                None => true,"

mutate "$DU" a_posture_on_a_dateless_device_reports_the_backup_as_unsettled \
  "the panel asks whether a drill happened, not whether it is current" \
  "        if !self.backup.is_proven(now.today) {" \
  "        if matches!(self.backup, BackupState::NeverRestored | BackupState::NotConfigured) {"

mutate "$DU" the_default_posture_toward_flash_is_untrusted \
  "the default posture toward consumer flash becomes trusting" \
  "    fn default() -> Self {
        Self::AssumedUntrusted
    }" \
  "    fn default() -> Self {
        Self::LabVerified(LabVerification {
            method: String::new(),
            fixture: String::new(),
            date: String::new(),
        })
    }"

mutate "$DU" an_unrestored_backup_is_a_standing_concern_that_no_amount_of_backing_up_clears \
  "an unrestored backup stops being a standing concern" \
  "        if !self.backup.is_proven(now.today) {
            out.push(self.backup.describe(now.today));
        }" \
  "        if false {
            out.push(self.backup.describe(now.today));
        }"

mutate "$DU" a_shed_ladder_nobody_has_watched_complete_is_not_credited \
  "a shed ladder nobody watched complete is silently credited" \
  "            GracefulShutdown::NeverObserved => out.push(
                \"the shed ladder has never been observed completing on this device\".to_owned(),
            )," \
  "            GracefulShutdown::NeverObserved => {}"

mutate "$DU" a_settled_device_still_required_somebody_to_restore_a_backup \
  "a verified shed ladder is reported as a concern anyway, so nothing ever settles" \
  "            GracefulShutdown::Verified => {}" \
  "            GracefulShutdown::Verified => out.push(\"unsettled\".to_owned()),"

mutate "$DU" assuming_the_flash_lies_is_never_itself_a_concern \
  "the default flash posture is listed beside real problems" \
  "        // Deliberately NOT a concern: DurabilityClass::AssumedUntrusted, and a" \
  "        if !self.durability.is_lab_verified() {
            out.push(\"the flash is not lab verified\".to_owned());
        }
        // Deliberately NOT a concern: DurabilityClass::AssumedUntrusted, and a"

mutate "$DU" an_unconfigured_device_reports_every_field_at_its_least_reassuring_value \
  "an unconfigured device starts out looking replicated" \
  "            recovery_point: RecoveryPoint::NoReplica," \
  "            recovery_point: RecoveryPoint::Behind(Duration::from_secs(0)),"

mutate "$IN" a_newly_installed_cell_publishes_nothing \
  "a newly installed cell publishes itself to the world by default" \
  "pub const DEFAULT: Mode = Mode::LocalOnly;" \
  "pub const DEFAULT: Mode = Mode::Onion;"

mutate "$IN" an_onion_is_not_recorded_as_dependency_free \
  "an onion is recorded as depending on nothing, the draft's flattering ruler" \
  "                dependency: Dependency::Commons,
                // RFC 7686" \
  "                dependency: Dependency::None,
                // RFC 7686"

mutate "$IN" an_onion_is_recorded_as_unreachable_by_ordinary_browsers \
  "an onion is recorded as reachable by an ordinary browser" \
  "                ordinary_browsers: false,
                thermal: ThermalClass::High," \
  "                ordinary_browsers: true,
                thermal: ThermalClass::High,"

mutate "$IN" the_most_sovereign_mode_is_recorded_as_having_the_worst_compromise_story \
  "the onion identity key is recorded as recoverable after theft" \
  "                compromise: CompromiseStory::Permanent," \
  "                compromise: CompromiseStory::Recoverable,"

mutate "$IN" a_derated_governor_sheds_high_thermal_ingress_first \
  "a derated governor no longer sheds the load that is heating the device" \
  "            Level::Derated => m.profile().thermal < ThermalClass::High," \
  "            Level::Derated => true,"

mutate "$IN" protect_and_halt_stop_everything_outward_facing \
  "outward-facing ingress keeps running through PROTECT and HALT" \
  "            Level::Protect | Level::Halt => !m.publishes()," \
  "            Level::Protect | Level::Halt => true,"

mutate "$IN" local_only_survives_every_level_because_it_is_not_what_is_heating_the_device \
  "a halted governor also takes away the panel the operator needs to read" \
  "            Level::Protect | Level::Halt => !m.publishes()," \
  "            Level::Protect | Level::Halt => false,"

mutate "$IN" a_device_that_cannot_hold_a_ceiling_is_told_there_is_no_mitigation_at_all \
  "the device with no mitigation available is not told so" \
  "        if !can_hold_ceiling {" \
  "        if false {"

mutate "$IN" choosing_an_onion_discloses_the_audience_limit_and_the_permanent_compromise \
  "the audience limit is not disclosed before the mode is chosen" \
  "    if !p.ordinary_browsers {" \
  "    if false {"

mutate "$IN" only_a_round_trip_from_outside_counts_as_verified \
  "a path that merely failed a check counts as verified" \
  "            Self::Failed(_) | Self::Unverified(_) => false," \
  "            Self::Failed(_) | Self::Unverified(_) => true,"

mutate "$IN" a_day_old_round_trip_does_not_still_stand \
  "a round trip verifies a path for the next century" \
  "pub const FRESH_FOR: Duration = Duration::from_secs(15 * 60);" \
  "pub const FRESH_FOR: Duration = Duration::from_secs(100 * 365 * 24 * 60 * 60);"

mutate "$IN" a_round_trip_from_a_minute_ago_still_stands \
  "a round trip stops standing the instant it completes" \
  "pub const FRESH_FOR: Duration = Duration::from_secs(15 * 60);" \
  "pub const FRESH_FOR: Duration = Duration::from_secs(0);"

mutate "$IN" a_round_trip_stamped_ahead_of_the_clock_is_not_evidence \
  "a stamp the clock cannot account for is read as a fresh round trip" \
  "                Some(age) => age.checked_sub(FRESH_FOR).is_none(),
                None => false," \
  "                Some(age) => age.checked_sub(FRESH_FOR).is_none(),
                None => true,"

mutate "$IN" a_lapsed_verification_reports_unverified_rather_than_failed \
  "a standing that has aged out is still reported as the round trip it was" \
  "        if self.is_verified(now) {
            return self.clone();
        }" \
  "        if true {
            return self.clone();
        }"

mutate "$SV" traversal_is_refused_rather_than_normalised_away \
  "a path that walks out of the document root is stripped and served" \
  "    if path.split('/').any(|seg| seg == \"..\" || seg == \".\") {
        return Err(BadRequest::Traversal);
    }" \
  "    if false {
        return Err(BadRequest::Traversal);
    }"

mutate "$SV" percent_encoding_is_refused_rather_than_decoded \
  "percent-encoded traversal slips past the check" \
  "    if path.contains('%') || path.contains('\\\\') || path.contains('\\0') {" \
  "    if false {"

mutate "$SV" only_the_four_implemented_verbs_are_accepted \
  "any verb is accepted as a read" \
  "        other => return Err(BadRequest::UnsupportedMethod(other.to_owned()))," \
  "        _ => Method::Get,"

mutate "$SV" even_a_404_carries_the_full_security_posture \
  "error responses are sent without the security posture" \
  "        for (name, value) in SecurityHeaders::production(surface.policy()).render(nonce) {" \
  "        let _ = nonce;
        for (name, value) in Vec::<(&str, String)>::new() {"

mutate "$SV" the_health_path_does_not_restate_the_devices_condition \
  "the health path restates the device condition, so two places can disagree" \
  "            Response::text(\"this process is answering; read /panel for what it found\\n\".to_owned())" \
  "            Response::text(panel.to_owned())"

mutate "$SV" a_request_line_longer_than_the_bound_is_refused_before_it_is_parsed \
  "an unbounded request line is accepted" \
  "    if line.len() > MAX_REQUEST_LINE {
        return Err(BadRequest::Malformed);
    }" \
  "    if false {
        return Err(BadRequest::Malformed);
    }"

mutate "$SV" a_head_request_omits_the_body_but_still_states_its_length \
  "a HEAD response carries a body" \
  "        if method != Method::Head {" \
  "        if true {"

# ── The published site: the first surface here serving strangers ─────────────

mutate "$ST" a_hidden_name_is_refused_as_a_class_rather_than_by_blocklist \
  "a dotfile is served, so .git and .env leave the building" \
  "        if segment.starts_with('.') {
            return Resolved::Refused(Refusal::Hidden(segment.to_owned()));
        }" \
  "        if false {
            return Resolved::Refused(Refusal::Hidden(segment.to_owned()));
        }"

mutate "$ST" a_path_that_walks_upward_is_refused_by_the_segment_that_does_it \
  "a path may walk out of the site directory" \
  "        if segment == \".\" || segment == \"..\" {" \
  "        if false {"

mutate "$ST" a_directory_with_no_index_does_not_become_a_listing \
  "a directory with no index resolves to something rather than refusing" \
  "    if host.exists(&base) {
        return Resolved::Refused(Refusal::NoIndex(path.to_owned()));
    }" \
  "    if false {
        return Resolved::Refused(Refusal::NoIndex(path.to_owned()));
    }"

mutate "$ST" the_shed_rung_is_where_a_website_stops \
  "the outage ladder stops withholding the site" \
  "                Stage::Shed | Stage::Quiesced | Stage::ShuttingDown => {
                    Self::Withheld(Withheld::Outage(stage))
                }" \
  "                Stage::Shed | Stage::Quiesced | Stage::ShuttingDown => Self::Serving,"

mutate "$ST" protect_and_halt_stop_the_site_whatever_the_outage_ladder_says \
  "the governor stops outranking the site" \
  "            Level::Protect | Level::Halt => Self::Withheld(Withheld::Governor(level))," \
  "            Level::Protect | Level::Halt => Self::Serving,"

mutate "$ST" an_unknown_extension_is_an_octet_stream_rather_than_a_guess \
  "an unknown extension is guessed as HTML rather than declared unknown" \
  "        _ => \"application/octet-stream\"," \
  "        _ => \"text/html; charset=utf-8\","

mutate "$ST" a_directory_is_not_resolved_as_though_it_were_a_page \
  "existence is used where the question was whether it is a file" \
  "host.is_file(&base);" \
  "host.exists(&base);"

mutate "$SV" a_withheld_site_refuses_before_it_resolves_anything \
  "a withheld site resolves paths anyway, mapping the directory by status code" \
  "    if !availability.is_serving() {
        return Response::refused(503, \"Service Unavailable\", &availability.describe());" \
  "    if false {
        return Response::refused(503, \"Service Unavailable\", &availability.describe());"

mutate "$SV" a_read_is_withheld_at_protect_and_below_exactly_as_the_site_is \
  "the vault hands stored files out while the same cell refuses to serve a page" \
  "            if !availability.is_serving() {
                return Response::refused(" \
  "            if false {
                return Response::refused("

mutate "$SV" a_read_still_answers_where_a_write_would_not \
  "a read is refused wherever a write is, collapsing the two columns into one" \
  "            if !availability.is_serving() {
                return Response::refused(" \
  "            if true {
                return Response::refused("

mutate "$SV" a_read_is_withheld_at_protect_and_below_exactly_as_the_site_is \
  "somebody asking for their own file is told a website is unavailable" \
  "                    &availability.describe_stored_file()," \
  "                    &availability.describe(),"

mutate "$SV" a_file_that_resolved_but_cannot_be_read_answers_exactly_like_a_typo \
  "an unreadable file answers differently from a missing one" \
  "            None => not_published(&request.path, &Refusal::NotFound(request.path.clone()))
                .explaining(format!(\"{} resolved and could not be read\", request.path))," \
  "            None => Response::refused(500, \"Internal Server Error\", \"it is there and I cannot read it\"),"

mutate "$SV" every_site_refusal_says_the_same_thing_on_the_wire \
  "each refusal explains itself, so the bodies map the directory the status hides" \
  "        &Refusal::NotFound(path.to_owned()).to_string()," \
  "        &why.to_string()," \

mutate "$SV" a_traversal_attempt_is_not_told_apart_from_an_ordinary_miss \
  "a traversal attempt answers 403, the status the ADR named as the temptation" \
  "        BadRequest::Traversal => {
            not_published(\"that path\", &Refusal::Escape(String::new())).explaining(bad.to_string())
        }" \
  "        BadRequest::Traversal => Response::refused(403, \"Forbidden\", &bad.to_string()),"

mutate "$SV" the_operator_still_learns_which_refusal_it_was \
  "the reason is taken off the wire and put nowhere, so nobody can see it" \
  "        Resolved::Refused(why) => not_published(&request.path, &why).explaining(why.to_string())," \
  "        Resolved::Refused(why) => not_published(&request.path, &why),"

mutate "$SV" the_operators_line_never_reaches_the_wire \
  "the operator's line is rendered to the visitor after all" \
  "        if method != Method::Head {
            out.extend_from_slice(&self.body);
        }" \
  "        if method != Method::Head {
            out.extend_from_slice(&self.body);
            if let Some(line) = &self.log {
                out.extend_from_slice(line.as_bytes());
            }
        }"

mutate "$SV" something_stored_in_the_way_answers_conflict_rather_than_server_error \
  "a conflict with something already stored is reported as the server breaking" \
  "            Self::Conflict(_) => (409, \"Conflict\")," \
  "            Self::Conflict(_) => (500, \"Internal Server Error\"),"

mutate "$SV" the_two_storage_failures_answer_with_different_statuses \
  "a write that genuinely failed is reported as somebody else's conflict" \
  "            Self::Failed(_) => (500, \"Internal Server Error\")," \
  "            Self::Failed(_) => (409, \"Conflict\"),"

mutate "$LI" nothing_the_caller_is_told_about_a_write_carries_a_filesystem_path \
  "the caller is handed the path the write failed on" \
  "fn logged_failure(doing: &str, path: &str, e: &std::io::Error) -> StorageFailure {
    eprintln!(\"vayucell: {doing} {path}: {e}\");
    not_completed()
}" \
  "fn logged_failure(doing: &str, path: &str, e: &std::io::Error) -> StorageFailure {
    eprintln!(\"vayucell: {doing} {path}: {e}\");
    StorageFailure::Failed(format!(\"{doing} {path}: {e}\"))
}"

mutate "$LI" a_write_cannot_reach_through_a_symlink_at_the_temporary_path \
  "an upload opens its temporary through a link and lands outside the vault" \
  "    for path in [plan.temporary(), plan.destination()] {" \
  "    for path in [plan.destination()] {"

mutate "$LI" a_write_does_not_silently_replace_a_symlink_at_the_destination \
  "an upload destroys an operator's link that a read would have refused" \
  "    for path in [plan.temporary(), plan.destination()] {" \
  "    for path in [plan.temporary()] {"

mutate "$LI" an_ordinary_write_over_a_real_file_is_still_allowed \
  "the vault refuses to replace a file it stored itself" \
  "        if is_symlink(path) {" \
  "        if true {"

mutate "$LI" a_write_cannot_reach_through_a_symlink_at_the_temporary_path \
  "the link check follows the link, so it never sees one" \
  "    std::fs::symlink_metadata(path).is_ok_and(|m| m.file_type().is_symlink())" \
  "    std::fs::metadata(path).is_ok_and(|m| m.file_type().is_symlink())"

mutate "$LI" a_symlink_pointing_out_of_the_root_is_refused \
  "a symbolic link may lead out of the site directory" \
  "    if !real_file.starts_with(&real_root) {
        eprintln!(
            \"vayucell: refusing {} — it resolves to {}, which is outside {}\"," \
  "    if false {
        eprintln!(
            \"vayucell: refusing {} — it resolves to {}, which is outside {}\","

# ── The vault route, and the verbs it added ──────────────────────────────────

mutate "$SV" only_head_omits_the_body_and_every_other_verb_carries_it \
  "every verb but GET loses its body again, so a receipt confirms nothing" \
  "        if method != Method::Head {" \
  "        if method == Method::Get {"

mutate "$SV" a_scheme_this_does_not_implement_reads_as_nothing_presented \
  "any authorization scheme is treated as a bearer credential" \
  "    if !scheme.eq_ignore_ascii_case(\"bearer\") {
        return None;
    }" \
  "    if false {
        return None;
    }"

mutate "$SV" a_body_larger_than_the_limit_is_refused_before_a_byte_of_it_is_read \
  "a body of any declared size is accepted, and allocated" \
  "                if n > MAX_BODY {
                    return Err(BadRequest::BodyTooLarge(n));
                }" \
  "                if false {
                    return Err(BadRequest::BodyTooLarge(n));
                }"

mutate "$SV" a_file_that_does_not_fit_is_told_apart_from_a_device_that_will_not_take_it \
  "a full disk is reported as the device refusing, so nobody knows to free space" \
  "    let (status, reason) = if matches!(admission, Admission::Refusing(Refused::Full(_))) {
        (507, \"Insufficient Storage\")
    } else {
        (503, \"Service Unavailable\")
    };" \
  "    let (status, reason) = (503, \"Service Unavailable\");"

mutate "$SV" an_unauthenticated_put_is_refused_before_anything_else_is_looked_at \
  "the credential stops being checked first, so a stranger learns the device state" \
  "    let verdict = ctx.credentials.verify(headers.bearer());" \
  "    let verdict = Verdict::Authenticated(crate::auth::DeviceName::new(\"anyone\").expect(\"plain\"));"

mutate "$SV" exactly_the_two_changing_verbs_write \
  "DELETE stops counting as a verb that changes anything" \
  "        matches!(self, Method::Put | Method::Delete)" \
  "        matches!(self, Method::Put)"

mutate "$SV" a_delete_obeys_the_governor_exactly_as_a_write_does \
  "a delete stops obeying the governor, so a halted phone still loses files" \
  "            let admission = Admission::for_removal(ctx.level, ctx.stage);
            if !admission.is_accepting() {
                return refused_admission(&admission);
            }" \
  "            let admission = Admission::for_removal(ctx.level, ctx.stage);
            if false {
                return refused_admission(&admission);
            }"

mutate "$SV" a_full_disk_never_refuses_the_request_that_would_free_some \
  "a delete is charged against the quota, so a full disk refuses the fix" \
  "            let admission = Admission::for_removal(ctx.level, ctx.stage);
            if !admission.is_accepting() {" \
  "            let admission = Admission::of(ctx.level, ctx.stage, ctx.quota, 1);
            if !admission.is_accepting() {"

mutate "$SV" a_vault_that_could_not_be_measured_still_allows_a_delete \
  "a delete waits on a usage figure, so an unreadable directory cannot be emptied" \
  "            let admission = Admission::for_removal(ctx.level, ctx.stage);
            if !admission.is_accepting() {" \
  "            let admission = Admission::of(ctx.level, ctx.stage, ctx.quota, 0);
            if !admission.is_accepting() {"

mutate "$LI" a_directory_that_does_not_exist_is_unknown_rather_than_empty \
  "a vault directory that will not open reads as an empty one, so every upload fits" \
  "    let entries = std::fs::read_dir(dir).ok()?;" \
  "    let Ok(entries) = std::fs::read_dir(dir) else {
        return Some(0);
    };"

mutate "$LI" what_is_stored_is_added_up_including_debris_from_an_interrupted_write \
  "stored bytes stop being added up, so the quota is never reached" \
  "            total = total.saturating_add(metadata.len());" \
  "            total = total.saturating_add(0);"

mutate "$LI" a_subdirectory_somebody_created_is_skipped_rather_than_walked \
  "anything in the folder is charged to the vault, including directories" \
  "        if metadata.is_file() {
            total = total.saturating_add(metadata.len());
        }" \
  "        if true {
            total = total.saturating_add(metadata.len());
        }"

mutate "$LI" a_symbolic_link_counts_as_the_link_and_not_as_what_it_points_at \
  "usage follows symbolic links, so a link to a large file elsewhere locks the vault" \
  "        let metadata = entry.ok()?.path().symlink_metadata().ok()?;" \
  "        let metadata = entry.ok()?.path().metadata().ok()?;"

mutate "$LI" a_connection_that_never_speaks_does_not_hold_the_surface \
  "one accept loop again, so a single silent socket closes the whole surface" \
  "const WORKERS: usize = 8;" \
  "const WORKERS: usize = 1;"

mutate "$LI" the_pool_is_large_enough_for_the_timeout_it_is_paired_with \
  "the idle timeout is raised without the pool, so a stall per second saturates it" \
  "const READ_TIMEOUT: Duration = Duration::from_secs(5);" \
  "const READ_TIMEOUT: Duration = Duration::from_secs(30);"

mutate "$LI" a_delete_cannot_reach_through_a_symlink_out_of_the_vault \
  "a delete follows a symbolic link out of the vault" \
  "        if !real_file.starts_with(&real_root) {
            eprintln!(
                \"vayucell: refusing to delete {path} — it resolves to {}, outside {}\"," \
  "        if false {
            eprintln!(
                \"vayucell: refusing to delete {path} — it resolves to {}, outside {}\","

mutate "$EN" revoking_leaves_the_store_private_and_leaves_no_debris \
  "a revoked store is rewritten world-readable" \
  "        options.mode(STORE_MODE);
    }
    let mut file = options.open(path).map_err(|e| format!(\"{path}: {e}\"))?;
    file.write_all(bytes).map_err(|e| format!(\"{path}: {e}\"))?;" \
  "        options.mode(0o644);
    }
    let mut file = options.open(path).map_err(|e| format!(\"{path}: {e}\"))?;
    file.write_all(bytes).map_err(|e| format!(\"{path}: {e}\"))?;"

mutate "$EN" revoking_a_device_stops_its_secret_working_and_leaves_the_others \
  "revocation removes every device rather than the named one" \
  "        if first == Some(device.as_str()) && !line.trim_start().starts_with('#') {" \
  "        if true {"

# ── Credentials ──────────────────────────────────────────────────────────────

mutate "$AU" an_empty_store_accepts_nothing \
  "a store with nobody enrolled stops being a closed door" \
  "        if self.entries.is_empty() {
            return Verdict::Refused(Refusal::StoreEmpty);
        }" \
  "        if false {
            return Verdict::Refused(Refusal::StoreEmpty);
        }"

mutate "$AU" a_prefix_of_an_enrolled_secret_is_refused \
  "the comparison stops at the shorter input, so a one-character secret matches" \
  "    if a.len() != b.len() {
        return false;
    }" \
  "    if false {
        return false;
    }"

mutate "$AU" the_comparison_is_exhaustively_right_on_short_inputs \
  "the difference accumulator stops accumulating" \
  "        difference |= x ^ y;" \
  "        difference &= x ^ y;"

mutate "$AU" a_memorable_secret_cannot_be_enrolled \
  "a secret somebody chose can be enrolled, with nothing to make guessing slow" \
  "        if raw.chars().count() != SECRET_CHARS {
            return Err(SecretError::WrongLength(raw.chars().count()));
        }" \
  "        if false {
            return Err(SecretError::WrongLength(raw.chars().count()));
        }"

mutate "$AU" a_secret_never_appears_in_its_own_debug_output \
  "the secret is printed by every debug format that touches it" \
  "        f.write_str(\"Secret(hidden)\")" \
  "        f.write_str(\&self.0)"

mutate "$AU" a_store_readable_by_anyone_else_is_reported_as_such \
  "a store the whole machine can read is reported as private" \
  "    mode & 0o077 != 0" \
  "    mode & 0o007 != 0"

mutate "$AU" a_name_enrolled_twice_refuses_the_store_and_names_both_lines \
  "a name enrolled twice loads as two credentials for one device" \
  "        if let Some(at) = seen
            .iter()
            .find_map(|(at, name)| (*name == device).then_some(*at))
        {" \
  "        if let Some(at) = seen
            .iter()
            .find_map(|(at, name)| (*name == device).then_some(*at))
            .filter(|_| false)
        {"

mutate "$AU" the_duplicate_names_the_line_it_was_first_seen_on_not_its_place_in_the_list \
  "the duplicate names the offending line twice instead of the earlier one" \
  "                why: StoreProblem::Duplicate { first_seen: at }," \
  "                why: StoreProblem::Duplicate {
                    first_seen: line_number,
                },"

mutate "$AU" two_devices_with_different_names_are_not_a_duplicate \
  "the store refuses two devices that share nothing but a file" \
  "            .find_map(|(at, name)| (*name == device).then_some(*at))" \
  "            .find_map(|(at, _name)| Some(*at))"

mutate "$AU" a_bad_line_refuses_the_whole_store_rather_than_loading_part_of_it \
  "a malformed line is skipped, silently unenrolling a device" \
  "        let secret = Secret::new(secret).map_err(|e| StoreError {
            line: line_number,
            why: StoreProblem::Secret(e),
        })?;" \
  "        let Ok(secret) = Secret::new(secret) else { continue };"

mutate "$AU" a_device_name_may_not_carry_whitespace_because_the_store_separates_on_it \
  "a device name may carry a space, so the store's two fields become three" \
  "        if raw.chars().any(char::is_whitespace) {
            return Err(DeviceError::Whitespace);
        }" \
  "        if false {
            return Err(DeviceError::Whitespace);
        }"

# ── The vault: the first surface here that accepts rather than serves ────────

mutate "$VA" a_name_that_is_really_a_path_is_refused \
  "a filename may be a path, so a write escapes the vault directory" \
  "        if raw.contains('/') || raw.contains('\\\\') {
            return Err(NameError::Separator);
        }" \
  "        if false {
            return Err(NameError::Separator);
        }"

mutate "$VA" a_hidden_name_is_refused_as_a_class \
  "a dotfile may be written into the vault" \
  "        if raw.starts_with('.') {
            return Err(NameError::Hidden);
        }" \
  "        if false {
            return Err(NameError::Hidden);
        }"

mutate "$VA" the_length_limit_counts_bytes_rather_than_characters \
  "the name limit counts characters, so a kilobyte of emoji is accepted" \
  "        if raw.len() > Self::MAX_BYTES {" \
  "        if raw.chars().count() > Self::MAX_BYTES {"

mutate "$VA" a_quota_already_over_its_limit_reports_no_free_space_rather_than_wrapping \
  "free space underflows once usage passes the limit, admitting anything" \
  "        self.limit.saturating_sub(self.used)" \
  "        self.limit.wrapping_sub(self.used)"

mutate "$VA" a_derated_device_refuses_a_write_even_though_it_would_still_serve_a_site \
  "a derated device starts taking uploads again" \
  "            Level::Derated | Level::Protect | Level::Halt => {" \
  "            Level::Protect | Level::Halt => {"

mutate "$VA" the_announced_rung_refuses_because_an_upload_is_new_work \
  "the rung that stopped accepting new work starts accepting uploads" \
  "            Stage::Announced | Stage::Shed | Stage::Quiesced | Stage::ShuttingDown => {" \
  "            Stage::Shed | Stage::Quiesced | Stage::ShuttingDown => {"

mutate "$VA" a_vault_whose_usage_could_not_be_read_refuses_the_write \
  "a vault nobody could measure is treated as one with room" \
  "        let Some(quota) = quota else {
            return Self::Refusing(Refused::Unmeasured);
        };" \
  "        let Some(quota) = quota else {
            return Self::Accepting;
        };"

mutate "$VA" a_removal_still_obeys_the_governor_and_the_ladder \
  "a delete stops asking the device, so a halted phone still loses files" \
  "        Self::of(level, stage, Some(Quota::new(0, 0)), 0)" \
  "        Self::Accepting"

mutate "$VA" the_steps_are_the_only_order_that_survives_a_power_cut \
  "the file is renamed before its bytes are flushed" \
  "            Step::WriteTemporary,
            Step::FlushFile,
            Step::RenameOverDestination,
            Step::FlushDirectory," \
  "            Step::WriteTemporary,
            Step::RenameOverDestination,
            Step::FlushFile,
            Step::FlushDirectory,"

mutate "$VA" a_refused_write_yields_no_plan \
  "a plan is handed back for a write the device refused" \
  "        if !self.is_accepting() {
            return None;
        }" \
  "        if false {
            return None;
        }"

mutate "$VA" the_temporary_is_hidden_so_debris_is_recognisable_as_debris \
  "the partial file is not hidden, so the site could serve a half-written upload" \
  "            temporary: format!(\"{}/.{}.partial\", root.dir(), name.as_str())," \
  "            temporary: format!(\"{}/{}.partial\", root.dir(), name.as_str()),"

mutate "$VA" a_receipt_never_claims_the_file_is_durable \
  "a receipt tells somebody their file is saved" \
  "            \"{} — {} bytes are on this device and on nothing else. {} This device \\" \
  "            \"{} — {} bytes saved. {} This device \\"

mutate "$B" a_charge_full_near_the_integer_limit_does_not_crash_the_reading \
  "a charge_full near the integer limit overflows the health calculation again" \
  "self.charge_full_uah.checked_mul(100)" \
  "Some(self.charge_full_uah * 100)"

mutate "$RP" the_governor_row_comes_from_the_cell_rather_than_from_a_literal \
  "the panel reports a quiet governor without reading the cell" \
  "    let (level, _) = crate::device::observe(host, supply_dir);
    assemble(host, supply_dir, ceiling, level.max(standing.floor()))" \
  "    let _ = crate::device::observe(host, supply_dir);
    assemble(host, supply_dir, ceiling, Level::Normal.max(standing.floor()))"

mutate "$RP" a_cool_cell_still_reports_the_governor_as_verified \
  "the panel reports trouble on every cell, quiet or not" \
  "    let (level, _) = crate::device::observe(host, supply_dir);
    assemble(host, supply_dir, ceiling, level.max(standing.floor()))" \
  "    let _ = crate::device::observe(host, supply_dir);
    assemble(host, supply_dir, ceiling, Level::Halt.max(standing.floor()))"

mutate "$SY" a_node_this_handset_does_not_have_is_named_rather_than_omitted \
  "an absent node is left out of the report, so nobody can tell it was looked for" \
  "            None => {
                let _ = writeln!(out, \"  ABSENT   {node}\");
            }" \
  "            None => {}"

mutate "$SY" the_report_says_what_it_holds_and_what_it_leaves_out \
  "the report stops saying what it contains, so nobody can check before pasting" \
  "        \"CONTAINS   the version, what was probed on this device, which power-supply\\n\\" \
  "        \"contents withheld\\n\\"

mutate "$SY" an_operator_set_assertion_is_flagged_because_the_probe_quotes_it \
  "a value the operator set is quoted into a public report without being flagged" \
  "    if let Some(value) = host.env(SHELL_ASSERTION_ENV) {" \
  "    if let Some(value) = None::<String> {"

mutate "$SY" a_report_with_nothing_operator_set_claims_no_exceptions \
  "the report claims an operator-set value on every device, including none" \
  "    if let Some(value) = host.env(SHELL_ASSERTION_ENV) {" \
  "    if let Some(value) = host.env(SHELL_ASSERTION_ENV).or(Some(String::new())) {"

mutate "$SY" a_supply_directory_the_operator_chose_is_flagged_as_theirs \
  "a path the operator chose is included without being flagged as theirs" \
  "    if supply_dir != SUPPLY {" \
  "    if false {"

mutate "$SF" the_published_node_list_matches_what_a_read_actually_consults \
  "the published node list loses an entry the reader still requires" \
  "    \"cycle_count\",
    \"charge_full\"," \
  "    \"charge_full\","

mutate "$HA" a_standing_halt_floors_any_report_at_halt \
  "a recorded halt stops reaching the panel, so it reports a cooled cell as fine" \
  "            Self::Halted(_) | Self::Unreadable(_) => Level::Halt," \
  "            Self::Halted(_) | Self::Unreadable(_) => Level::Normal,"

mutate "$RP" the_standing_floors_the_reading_rather_than_replacing_it \
  "the panel reports the recorded halt instead of the cell, hiding a live reading" \
  "    assemble(host, supply_dir, ceiling, level.max(standing.floor()))" \
  "    assemble(host, supply_dir, ceiling, standing.floor())"

mutate "$RP" a_recorded_halt_reaches_the_panel_even_though_the_cell_has_cooled \
  "the panel ignores the halt record entirely and reports only the cell" \
  "    assemble(host, supply_dir, ceiling, level.max(standing.floor()))" \
  "    assemble(host, supply_dir, ceiling, level)"

mutate "$HA" a_record_nobody_could_read_is_not_treated_as_no_record \
  "a halt record nobody could read lets the device serve" \
  "            Self::Halted(_) | Self::Unreadable(_) => false," \
  "            Self::Halted(_) => false,
            Self::Unreadable(_) => true,"

mutate "$HA" an_empty_record_is_refused_rather_than_read_as_a_halt_with_no_reason \
  "an empty halt record parses as a halt that names nothing" \
  "        if reason.is_empty() {
            return Err(HaltError::Empty);
        }" \
  "        if false {
            return Err(HaltError::Empty);
        }"

mutate "$HA" a_record_carrying_a_control_character_is_refused \
  "a halt reason may carry a newline and rewrite the operator's terminal" \
  "        if reason.chars().any(char::is_control) {" \
  "        if false {"

mutate "$G" a_recorded_halt_produces_a_governor_that_is_already_halted \
  "an inherited halt comes back at NORMAL, so any restart clears a hard stop" \
  "            level: Level::Halt,
            thresholds,
            history: Vec::new(),
            consecutive_failures: 0,
        }
    }

    /// How many consecutive read attempts have failed." \
  "            level: Level::Normal,
            thresholds,
            history: Vec::new(),
            consecutive_failures: 0,
        }
    }

    /// How many consecutive read attempts have failed."

mutate "$HR" a_record_that_exists_and_will_not_parse_is_unreadable_rather_than_clear \
  "an unparseable record is read as no record, returning a halted phone to service" \
  "            Err(e) => Standing::Unreadable(format!(\"{path}: {e}\"))," \
  "            Err(_) => Standing::Clear,"

mutate "$HR" a_recorded_halt_is_still_there_for_the_next_process \
  "the halt record is never renamed into place, so the next start finds nothing" \
  "    std::fs::rename(&temporary, path).map_err(|e| format!(\"{path}: {e}\"))?;" \
  "    let _ = &temporary;"

mutate "$CL" a_halted_supervisor_keeps_refusing_after_the_cell_cools \
  "the surfaces read only the fresh cell, so a cooled phone clears a hard stop" \
  "        (fresh.max(governed), stage)" \
  "        (fresh, stage)"

mutate "$CL" a_cell_that_spikes_between_ticks_is_refused_on_the_fresh_reading \
  "the surfaces read only the supervisor, so a spike since its last tick is served" \
  "        (fresh.max(governed), stage)" \
  "        (governed, stage)"

mutate "$CL" the_supervisors_ladder_is_the_one_the_surfaces_read \
  "the surfaces stop seeing the supervisor's ladder and never shed during an outage" \
  "        let stage = supervisor.shed().stage();" \
  "        let stage = Stage::Serving;"

mutate "$CL" a_rung_entered_by_one_surface_is_seen_by_the_next \
  "each surface gets its own ladder, so a shed node still serves on the other port" \
  "                let mut ladder = self.ladder.lock().unwrap_or_else(PoisonError::into_inner);" \
  "                let mut ladder = Shed::new(ShedPlan::recommended());"

mutate "$CL" without_a_declared_outage_the_ladder_is_never_walked_at_all \
  "the outage ladder is walked even when nobody claimed mains was lost" \
  "            None => Stage::Serving," \
  "            None => Stage::ShuttingDown,"

mutate "$AR" a_port_too_near_the_top_is_refused_rather_than_wrapped \
  "three ports are counted with wrapping, so the last one lands on whatever was spare" \
  "    base.checked_add(2).ok_or_else(|| {" \
  "    base.wrapping_add(2); Some(()).ok_or_else(|| {"

mutate "$AR" port_zero_is_refused_because_there_is_no_next_one_along \
  "port 0 is counted from, so two surfaces bind ports nobody chose" \
  "    if base == 0 {" \
  "    if false {"


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
