#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# The installer is the first thing a stranger runs, and it runs on their phone
# rather than ours. It is also the one script that cannot be exercised by the
# ordinary test suite, so without this it would be the least-tested file in the
# repository and the most exposed.
#
# What this checks is what can be checked without an Android device:
#   - it parses, and shellcheck is clean
#   - every failure path says what to do, not just what broke
#   - it actually installs, from a clean HOME, and the result runs
#   - running it twice is safe
#
# What it cannot check is Termux itself. That is stated rather than implied.
set -uo pipefail
cd "$(dirname "$0")/.." || exit 1

FAILED=0
pass() { printf '  ok    %s\n' "$1"; }
fail() { printf '  FAIL  %s\n' "$1"; FAILED=1; }

echo "Install gate — the installer must work for somebody who has never used a terminal"
echo

[ -x install.sh ] && pass "install.sh is executable" || fail "install.sh is not executable"
bash -n install.sh 2>/dev/null && pass "install.sh parses" || fail "install.sh has a syntax error"

# Every die() must carry both halves: what happened AND what to do. A failure
# message that only names the error leaves the person exactly where they were.
# Counted rather than pattern-matched. The first version of this check required
# the two arguments to be split across a line continuation, and flagged a
# perfectly good `die "what" "todo"` written on one line — the gate was wrong,
# not the installer. Two quoted arguments means at least four quote characters.
bad_dies=0
while IFS= read -r n; do
  block="$(sed -n "${n},$((n+3))p" install.sh | tr '\n' ' ')"
  quotes="$(printf '%s' "$block" | tr -cd '"' | wc -c)"
  [ "$quotes" -ge 4 ] || bad_dies=$((bad_dies + 1))
done < <(grep -n '^\s*die ' install.sh | cut -d: -f1)
[ "$bad_dies" -eq 0 ] \
  && pass "every failure path names both what happened and what to do" \
  || fail "$bad_dies failure path(s) do not say what to do next"

# The battery warning is not optional and not a footnote. It must appear before
# anything is written to disk.
warn_line="$(grep -n 'swollen battery is a fire hazard' install.sh | cut -d: -f1 | head -1)"
first_write="$(grep -n '^mkdir -p "\$PREFIX' install.sh | cut -d: -f1 | head -1)"
if [ -n "$warn_line" ] && [ -n "$first_write" ] && [ "$warn_line" -lt "$first_write" ]; then
  pass "the battery warning is shown before anything is installed"
else
  fail "the battery warning must come before the first write to disk"
fi

grep -q 'face-down on a flat table' install.sh \
  && pass "the physical inspection instruction is in the installer" \
  || fail "the installer must name physical inspection — Charter III.4"

# The installer downloads `vayucell-<triple>.tar.gz`; the release workflow names
# its tarballs from its own matrix. Nothing connected those two strings, and for
# the whole life of the release workflow they did not match: it packaged `.rlib`
# library files, so no install ever found a usable build and every one of them
# silently fell back to a twenty-minute source build on an old phone. The build
# was green throughout. This is the check that would have caught it.
missing=""
for t in $(grep -oE '(aarch64|armv7|x86_64)-[a-z0-9-]*(android|androideabi|gnu|gnueabihf)' install.sh | sort -u); do
  grep -qE "^ *- +$t\$" .github/workflows/release.yml || missing="$missing $t"
done
if [ -z "$missing" ]; then
  pass "every target the installer downloads is one the release actually builds"
else
  fail "the installer downloads targets the release does not build:$missing"
fi

# And the reverse direction of the same bug: a release that publishes something
# other than a runnable program.
grep -q 'vayucell-\${{ matrix.target }}.tar.gz' .github/workflows/release.yml \
  && pass "the release publishes a runnable binary under the name the installer asks for" \
  || fail "the release does not publish vayucell-<target>.tar.gz — the installer will never find a build"

# It must not quietly acquire privileges.
grep -qE '^\s*sudo |^\s*su ' install.sh \
  && fail "the installer escalates privileges" \
  || pass "the installer never asks for root"

# End to end, from a clean HOME.
if [ "${VAYUCELL_SKIP_INSTALL_RUN:-0}" = "1" ]; then
  printf '  --    end-to-end install skipped (VAYUCELL_SKIP_INSTALL_RUN=1)\n'
else
  work="$(mktemp -d)"
  trap 'rm -rf "$work"' EXIT
  if HOME="$work" VAYUCELL_ASSUME_YES=1 VAYUCELL_PREFIX="$work/.vayucell" \
       bash install.sh >"$work/out" 2>&1; then
    pass "installs from a clean HOME"
    # Which path it took matters and is not obvious from a green tick. Once a
    # release exists this exercises the download-and-verify path against the
    # *published* build, which is what a person gets — but it is then no longer
    # a test of the working tree, and saying so is cheaper than someone assuming
    # otherwise.
    # The verification itself, exercised against planted files rather than only
    # observed on a run that happened to succeed. Every case below is one a live
    # install cannot be made to produce on demand, and one of them was a real
    # hole: a checksum list that would not download used to warn and continue.
    v="$(mktemp -d)"
    # The shipped function, not a copy of it. A copy here would keep passing
    # after the real one changed.
    # shellcheck source=/dev/null
    VAYUCELL_VERIFY_ONLY=1 . ./install.sh

    printf 'payload\n' > "$v/vayucell-t.tar.gz"
    ( cd "$v" && sha256sum vayucell-t.tar.gz > SHA256SUMS.txt )
    verify_download "$v" "vayucell-t.tar.gz" \
      && pass "a download matching its published checksum is accepted" \
      || fail "a good download was refused"

    printf 'tampered\n' > "$v/vayucell-t.tar.gz"
    verify_download "$v" "vayucell-t.tar.gz" \
      && fail "a tampered download was accepted" \
      || pass "a download that does not match its checksum is refused"

    printf 'payload\n' > "$v/vayucell-t.tar.gz"
    ( cd "$v" && sha256sum SHA256SUMS.txt > SHA256SUMS.txt.new && mv SHA256SUMS.txt.new SHA256SUMS.txt )
    verify_download "$v" "vayucell-t.tar.gz" \
      && fail "a build absent from the checksum list was accepted" \
      || pass "a build the checksum list does not mention is refused"
    rm -rf "$v"

    # And the hole itself: no checksum list at all must stop the install, not
    # warn past it. Asserted against the script's text, because the branch
    # cannot be reached without failing a real download.
    if grep -q 'continuing unverified' install.sh; then
      fail "the installer still continues when it cannot verify a download"
    else
      pass "an unverifiable download stops the install rather than warning"
    fi

    if grep -q 'the published checksums could not be downloaded' install.sh; then
      pass "a missing checksum list is refused by name"
    else
      fail "nothing refuses a missing checksum list"
    fi

    if grep -q 'Downloaded a published build' "$work/out"; then
      printf '  --    it took the download path; the tree itself was not compiled here\n'
      grep -q 'Checksum matches' "$work/out" \
        && pass "the download was checked against its published checksum" \
        || fail "the installer accepted a download it did not verify"
    else
      printf '  --    no published build for this platform; it compiled from source\n'
    fi
    if "$work/.vayucell/bin/vayucell" version >/dev/null 2>&1; then
      pass "the installed program runs"
    else
      fail "the installer reported success but the program does not run"
    fi
    # Twice. A half-finished install that cannot be re-run strands somebody
    # somewhere they cannot describe.
    if HOME="$work" VAYUCELL_ASSUME_YES=1 VAYUCELL_PREFIX="$work/.vayucell" \
         bash install.sh >"$work/out2" 2>&1; then
      pass "running it a second time is safe"
    else
      fail "running the installer twice fails; see $work/out2"
    fi
  else
    fail "the installer did not complete on a clean HOME"
    tail -20 "$work/out" | sed 's/^/        /'
  fi
fi

echo
printf '  --    Termux itself is not exercised here; no Android device is available in CI\n'
echo
if [ "$FAILED" -ne 0 ]; then
  echo "INSTALL GATE FAILED — the first thing a stranger runs is broken."
  exit 1
fi
echo "Install gate passed."
