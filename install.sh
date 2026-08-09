#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
#
# VayuCell installer.
#
# One command, and it is written for somebody who has never used a terminal.
# That constraint drives three rules the rest of this file follows:
#
#   1. Every failure says what to do next, in words. "curl: (22)" is not an
#      error message, it is a receipt for one.
#   2. Nothing is assumed. The architecture, the environment, the presence of
#      each tool and the writability of each directory are checked, and the
#      check that fails names itself.
#   3. It is safe to run twice. A half-finished install that cannot be re-run is
#      worse than no installer, because the person is now stuck somewhere they
#      cannot describe.
#
# It does NOT ask for root, does NOT modify anything outside its own directory,
# and does NOT open a port to the internet. See docs/INSTALL.md.

set -uo pipefail

REPO="johalputt/VayuCell"
RAW="https://github.com/$REPO"
PREFIX="${VAYUCELL_PREFIX:-$HOME/.vayucell}"
BIN="$PREFIX/bin/vayucell"

# ── Talking to a person ───────────────────────────────────────────────────────

if [ -t 1 ] && [ -z "${NO_COLOR:-}" ]; then
  B=$'\033[1m'; G=$'\033[32m'; Y=$'\033[33m'; R=$'\033[31m'; Z=$'\033[0m'
else
  B=""; G=""; Y=""; R=""; Z=""
fi

say()  { printf '%s\n' "$*"; }
step() { printf '\n%s==>%s %s\n' "$B" "$Z" "$*"; }
good() { printf '  %s✓%s %s\n' "$G" "$Z" "$*"; }
warn() { printf '  %s!%s %s\n' "$Y" "$Z" "$*"; }

# Every exit path goes through here, so no failure can end without saying what
# the person should do about it.
die() {
  printf '\n%sSomething stopped the install.%s\n\n' "$R" "$Z" >&2
  printf '  What happened: %s\n' "$1" >&2
  printf '  What to do:    %s\n\n' "$2" >&2
  printf 'If that does not help, open an issue with everything printed above:\n' >&2
  printf '  %s/issues/new\n\n' "$RAW" >&2
  exit 1
}

# ── Verifying a download ──────────────────────────────────────────────────────

# Checks one downloaded asset against the published checksum list.
#
# A function, and defined up here before anything happens, so it can be tested
# without a network, a release, or a device: scripts/install-gate.sh sources
# this file and calls *this* function with planted inputs. A copy of it in the
# gate would drift from the copy that ships, and the one that drifts is always
# the one nobody runs.
#
# Returns non-zero for every way it can fail to establish a match. `grep -F`
# finding nothing is one of those ways: it feeds `sha256sum -c` an empty list,
# which reports no properly formatted lines and exits non-zero, so a build the
# list does not mention is refused rather than passed.
verify_download() {
  ( cd "$1" && grep -F "$2" SHA256SUMS.txt | sha256sum -c - >/dev/null 2>&1 )
}

# Sourced by the gate to reach the function above. Nothing past this line runs
# in that mode, and the variable is never set when the installer is executed
# normally, so a person running this always gets the whole script.
if [ -n "${VAYUCELL_VERIFY_ONLY:-}" ]; then
  return 0 2>/dev/null || exit 0
fi

# ── Where are we ──────────────────────────────────────────────────────────────

step "Checking what kind of device this is"

IS_TERMUX=0
if [ -n "${TERMUX_VERSION:-}" ] || [ -d /data/data/com.termux ]; then
  IS_TERMUX=1
  good "Android, running inside Termux"
elif [ "$(uname -s)" = "Linux" ]; then
  good "Linux"
else
  die "this is $(uname -s), and VayuCell runs on Android or Linux" \
      "If this is a phone, install Termux from F-Droid and run this inside it. See $RAW/blob/main/docs/INSTALL.md"
fi

# The Rust target triple, not a friendly name. It is the exact string the
# release workflow names its tarballs with, so the two cannot drift apart
# silently — scripts/install-gate.sh checks every triple named here is one the
# release matrix actually builds.
case "$(uname -m)" in
  aarch64|arm64) ARCH="aarch64"; TARGET="aarch64-linux-android"   ;;
  armv7l|armv8l) ARCH="armv7";   TARGET="armv7-linux-androideabi" ;;
  x86_64|amd64)  ARCH="x86_64";  TARGET=""                        ;;
  *) die "the processor type '$(uname -m)' is not one VayuCell builds for" \
         "Open an issue saying which device this is — new processor types are usually easy to add" ;;
esac
if [ "$IS_TERMUX" != "1" ]; then
  case "$ARCH" in
    aarch64) TARGET="aarch64-unknown-linux-gnu"      ;;
    armv7)   TARGET="armv7-unknown-linux-gnueabihf"  ;;
    x86_64)  TARGET="x86_64-unknown-linux-gnu"       ;;
  esac
fi
good "Processor: $ARCH"

# ── The one thing a phone owner must be told before anything is installed ─────

step "Before anything is installed, read this"
cat <<'WARNING'

  VayuCell asks you to leave a phone plugged in, warm, for a long time — in a
  building where you sleep. That is the condition under which a lithium battery
  ages fastest, and a swollen battery is a fire hazard.

  Not every phone can limit its own charging. On an ordinary unrooted phone,
  none can. VayuCell will tell you which case your phone is, on the first
  screen, before you rely on it.

  Put the phone face-down on a flat table now and then. If it rocks, or the
  screen or back is lifting at any edge, stop using it and take it to
  hazardous-waste handling. Software cannot see that. You can.

WARNING

if [ -t 0 ] && [ "${VAYUCELL_ASSUME_YES:-0}" != "1" ]; then
  printf '  Type yes to continue: '
  read -r reply
  case "$reply" in
    [Yy][Ee][Ss]) ;;
    *) say ""; say "Nothing was installed."; exit 0 ;;
  esac
fi

# ── Tools ─────────────────────────────────────────────────────────────────────

step "Checking the tools this needs"

need() {
  command -v "$1" >/dev/null 2>&1 && { good "$1"; return 0; }
  if [ "$IS_TERMUX" = "1" ]; then
    warn "$1 is missing — installing it"
    pkg install -y "$2" >/dev/null 2>&1 \
      || die "could not install $1" \
             "Run: pkg update && pkg install $2   — then run this installer again"
    command -v "$1" >/dev/null 2>&1 \
      || die "$1 still is not available after installing $2" \
             "Close Termux completely, open it again, and re-run this installer"
    good "$1 (installed)"
  else
    die "$1 is not installed" \
        "Install it with your package manager, for example: sudo apt install $2"
  fi
}

need curl curl
need tar tar

# ── Getting the program ───────────────────────────────────────────────────────

mkdir -p "$PREFIX/bin" \
  || die "could not create $PREFIX" "Check there is free space and that $HOME is writable"

step "Fetching VayuCell"

ASSET=""
[ -n "$TARGET" ] && ASSET="vayucell-$TARGET.tar.gz"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT

if [ -n "$ASSET" ] && curl -fsSL -o "$TMP/$ASSET" \
     "$RAW/releases/latest/download/$ASSET" 2>/dev/null; then
  good "Downloaded a published build"

  # A download nobody verified is a download.
  #
  # This used to warn and carry on when the checksum file would not download,
  # which made every other check here decorative: anything able to fail one
  # request — a proxy, a captive portal, a bad minute — silently downgraded the
  # install to no verification at all, behind a yellow mark that scrolls past.
  # Absence is never protection, and this is the first thing a stranger runs.
  if ! curl -fsSL -o "$TMP/SHA256SUMS.txt" \
       "$RAW/releases/latest/download/SHA256SUMS.txt" 2>/dev/null; then
    die "the published checksums could not be downloaded, so this build cannot be checked" \
        "Try again in a minute. If it keeps failing, do not install: an unverified binary is not worth the wait"
  fi
  if verify_download "$TMP" "$ASSET"; then
    good "Checksum matches the published one"
  else
    die "the downloaded file does not match its published checksum" \
        "Do not use it. This can mean a broken download — try again — or a file that is not the one that was published"
  fi
  # Said rather than implied. The checksum proves this file is the one the
  # release lists; it is not an independent signature check, because the file
  # and the list came from the same place over the same connection. The release
  # publishes a cosign signature over the list for anyone who wants the stronger
  # property, and this says so rather than letting a green tick imply it.
  say  "     (checksum only — the release also publishes SHA256SUMS.txt.sig for"
  say  "      anyone who wants to verify it with cosign)"

  tar -xzf "$TMP/$ASSET" -C "$TMP" \
    || die "the downloaded archive could not be opened" "Run the installer again; the download may have been cut short"
  found="$(find "$TMP" -type f -name vayucell -perm -u+x | head -1)"
  [ -n "$found" ] || die "the archive did not contain the program" "Please open an issue — the published build is wrong"
  mv "$found" "$BIN"

else
  # No published build for this platform yet. Building from source is slower and
  # it is honest about that rather than appearing to hang.
  warn "No published build for $ARCH yet — building from source instead"
  say  "     This takes roughly 10-20 minutes on a phone and needs about 2 GB free."
  say  "     It only happens once."

  need git git
  # `command -v cargo` answers "is there something on PATH called cargo", which
  # is not the question. On a machine with a rustup shim and no default
  # toolchain the name resolves, the check passes, and the build then dies with
  # "rustup could not choose a version of cargo to run" — a real failure this
  # installer once reported as "free up 2 GB". Presence is not verification, so
  # this runs it.
  if ! cargo --version >/dev/null 2>&1; then
    if [ "$IS_TERMUX" = "1" ]; then
      warn "Installing Rust (this is the big one)"
      pkg install -y rust >/dev/null 2>&1 \
        || die "could not install Rust" "Run: pkg update && pkg install rust   — then run this installer again"
      cargo --version >/dev/null 2>&1 \
        || die "Rust is installed but will not run" \
               "Close Termux completely, open it again, and re-run this installer"
    elif command -v rustup >/dev/null 2>&1; then
      die "Rust is installed but no default toolchain is set, so cargo cannot run" \
          "Run: rustup default stable   — then run this installer again"
    else
      die "Rust is not installed" "Install it from https://rustup.rs and run this installer again"
    fi
  fi
  good "Rust runs: $(cargo --version)"

  step "Building — leave the screen on and the phone plugged in"
  git clone --depth 1 "$RAW.git" "$TMP/src" >/dev/null 2>&1 \
    || die "could not download the source code" "Check the phone is online, then run the installer again"
  # The build's own error is kept and shown. Guessing at the cause — this used
  # to say "usually free space or memory" — is worse than saying nothing, since
  # it sends the person to fix something that was never wrong.
  if ! ( cd "$TMP/src" && cargo build --release --locked -p vayucell ) >"$TMP/build.log" 2>&1; then
    say ""
    say "  The build stopped. Its last lines were:"
    tail -15 "$TMP/build.log" | sed 's/^/      /'
    die "the build did not finish" \
        "Read the lines above — they say why. If they mention space or memory, free up 2 GB and close other apps, then run the installer again"
  fi
  cp "$TMP/src/target/release/vayucell" "$BIN" \
    || die "the build finished but the program could not be copied into place" "Check free space in $HOME"
fi

chmod +x "$BIN" || die "could not make the program runnable" "Check $PREFIX/bin is on a normal filesystem"
good "Installed to $BIN"

# ── Proving it actually runs ──────────────────────────────────────────────────

step "Checking it runs"
if ! "$BIN" version >/dev/null 2>&1; then
  die "the program is installed but will not start" \
      "Open an issue and include the output of: $BIN version"
fi
good "$("$BIN" version)"

# ── Making it easy to start ───────────────────────────────────────────────────

step "Setting up the commands"

LAUNCHER="$PREFIX/bin/vayucell-start"
cat > "$LAUNCHER" <<EOF
#!/usr/bin/env bash
# Starts VayuCell and prints the address to open.
exec "$BIN" serve --bind 0.0.0.0:8080 "\$@"
EOF
chmod +x "$LAUNCHER"
good "vayucell-start  — serve the panel to your home network"

for rc in "$HOME/.bashrc" "$HOME/.profile"; do
  [ -f "$rc" ] || continue
  grep -qF "$PREFIX/bin" "$rc" 2>/dev/null && continue
  printf '\n# VayuCell\nexport PATH="%s/bin:$PATH"\n' "$PREFIX" >> "$rc"
  good "Added to PATH in ${rc##*/}"
done
export PATH="$PREFIX/bin:$PATH"

# ── What now ──────────────────────────────────────────────────────────────────

say ""
say "${B}Installed.${Z}"
say ""
say "  See what this phone can and cannot do:"
say "      ${B}vayucell status${Z}"
say ""
say "  Serve the safety panel to your home network:"
say "      ${B}vayucell-start${Z}"
say "      then open the address it prints, on any device on your Wi-Fi"
say ""
say "  ${Y}Read this:${Z} vayucell status will very likely say UNSAFE on an"
say "  ordinary phone, and that is the correct answer rather than a fault. Most"
say "  phones cannot limit their own charging, and VayuCell refuses to pretend"
say "  otherwise. The panel explains each line."
say ""
say "  Full guide: $RAW/blob/main/docs/INSTALL.md"
say ""
if [ "$IS_TERMUX" = "1" ]; then
  say "  ${Y}One more thing on Android:${Z} the system will kill background apps to"
  say "  save power. In Termux run ${B}termux-wake-lock${Z}, and in Android Settings"
  say "  set Termux battery usage to Unrestricted. The guide has screenshots."
  say ""
fi
