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
SF=core/src/sysfs.rs
SM=core/src/sampler.rs
SH=core/src/shed.rs
P=core/src/panel.rs
RT=core/src/runtime.rs
AR=cli/src/args.rs
RP=cli/src/report.rs
DU=core/src/durability.rs
IN=core/src/ingress.rs

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
  "            RecoveryPoint::Behind(lag) => *lag > target,
            _ => true," \
  "            RecoveryPoint::Behind(lag) => *lag > target,
            RecoveryPoint::Unreachable(_) => false,
            _ => true,"

mutate "$DU" a_backup_nobody_has_restored_is_never_proven \
  "a backup that was merely written counts as proven" \
  "        matches!(self, BackupState::Restored { .. })" \
  "        !matches!(self, BackupState::NotConfigured)"

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
  "        if !self.backup.is_proven() {
            out.push(self.backup.to_string());
        }" \
  "        if false {
            out.push(self.backup.to_string());
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
  "        matches!(self, Reachability::Verified { .. })" \
  "        !matches!(self, Reachability::Unverified(_))"

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
