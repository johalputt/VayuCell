# Installing VayuCell on a phone

Written for somebody who has never opened a terminal. If a step assumes
something you do not have, that is a defect in this page — please
[say so](https://github.com/johalputt/VayuCell/issues/new).

---

## What you will have at the end

An old phone on your Wi-Fi, running a program that watches its battery and
shows you a page describing exactly what it has and has not verified.

**What it does not do yet, stated plainly:** it does not host a website, and it
does not store your files. Those are the point of the project and they are not
written. What exists today is the safety layer everything else has to sit on —
the part that decides whether it is reasonable to leave this phone plugged in
and warm in the first place. Building that first was deliberate, and the
[charter](../CHARTER.md) forbids the other order.

If you want a file store or a website **today**, this is not that yet.

---

## What you need

| | |
| --- | --- |
| A phone | Android 7 or newer. It does not need to be unlocked, rooted, or wiped |
| A charger | It stays plugged in |
| Wi-Fi | The same network as the computer or phone you will read the page from |
| Time | About 20 minutes, most of it waiting |

You do **not** need: root, a computer, a cable, a Google account, a domain
name, or any payment.

---

## Step 1 — Install Termux

Termux is a free app that gives your phone a Linux command line. It is what
VayuCell runs inside.

**Install it from F-Droid, not the Play Store.** The Play Store copy is an
abandoned version that will not work — this is the single most common reason an
install fails.

1. On the phone, open <https://f-droid.org/> and tap **Download F-Droid**
2. Open the downloaded file. Android will ask whether to allow installing from
   this source — allow it, then **Install**
3. Open F-Droid, tap the search icon, type `Termux`
4. Tap **Termux**, then **Install**

> **Why an app store you have not heard of?** F-Droid only carries software
> whose source code is public and which it builds itself. It is the ordinary
> route for this app, and Termux's own developers point people there.

---

## Step 2 — Run one command

Open Termux. You will see a black screen with a `$`. That is normal — it is
waiting for you.

Type this exactly, or paste it (long-press the screen → Paste), then press
Enter:

```bash
curl -fsSL https://raw.githubusercontent.com/johalputt/VayuCell/main/install.sh | bash
```

**Before you run it, one honest note.** That command downloads a script and runs
it immediately. That is convenient, and it is also a pattern you should be
suspicious of in general — it means trusting whatever the server sends. If you
would rather look first, which is the better habit:

```bash
curl -fsSL -o install.sh https://raw.githubusercontent.com/johalputt/VayuCell/main/install.sh
less install.sh          # press q to quit
bash install.sh
```

The installer will:

- tell you what kind of device it found
- **show you a safety warning about the battery, and wait for you to type
  `yes`** — read it, it is short and it matters
- install anything missing
- fetch or build VayuCell
- check the program actually runs before claiming success

The first install builds from source and takes **10–20 minutes**. Leave the
screen on and the phone plugged in. It only happens once.

---

## Step 3 — Look at what your phone can actually do

```bash
vayucell status
```

You will see something like this:

```text
BATTERY SAFETY: UNSAFE

  VERIFIED     device tier          T0 established from positive evidence
  FAILED       charge mechanism     this device exposes no charge control,
                                    so no ceiling can be held
  UNVERIFIED   charge ceiling       no mechanism exists to hold a ceiling
  VERIFIED     battery governor     governor at NORMAL; no threshold crossed
  FAILED       outage reserve       ...
```

### `UNSAFE` is very likely, and it is the correct answer

On an ordinary phone that has not been rooted, **there is no supported way for
any software to stop the battery charging at 60%**. Not for VayuCell, and not
for anything else. Most similar tools quietly do not mention this.

VayuCell says it, on the first screen, and the row stays red permanently
because it is permanently true. A green row here would be a lie, and a program
that lies about a battery in your house is worse than no program.

**What it means for you:** the phone will charge to 100% and sit there, which
is the condition that ages a battery fastest. That is a real risk and it is
yours to weigh — it is not made safe by software. Which is why the next section
is not optional.

---

## Step 4 — The check no software can do for you

**Once a month, put the phone face-down on a flat table.**

- Does it rock or wobble instead of lying flat?
- Is the screen or the back cover lifting at any edge, even slightly?
- Is there a gap that was not there before?

If yes to any of those: **stop using it now**, unplug it, and take it to
hazardous-waste or electronics recycling. Do not puncture it, do not put it in
household rubbish.

A swelling battery is the failure that matters, and no sensor on the phone can
detect it. You can, in five seconds, by looking.

---

## Step 5 — Open the panel from another device

```bash
vayucell-start
```

It prints an address like:

```text
vayucell: serving the panel on http://0.0.0.0:8080/ (local only)
```

On any phone or laptop **on the same Wi-Fi**, open `http://<phone-ip>:8080`.
To find the phone's address, in another Termux window run:

```bash
ifconfig 2>/dev/null | grep 'inet ' | grep -v 127.0.0.1
```

**This is your network only.** Nothing is published to the internet, no port is
forwarded, and nobody outside your home can reach it. Publishing is a separate,
deliberate choice you have not made — see [ADR-0003](adr/ADR-0003-sovereign-ingress.md).

Press `Ctrl+C` in Termux to stop it.

---

## Step 6 — Stop Android killing it

Android aggressively stops background apps to save power. Two changes:

**In Termux:**

```bash
termux-wake-lock
```

**In Android Settings** — the exact wording differs by manufacturer:

1. Settings → Apps → **Termux**
2. **Battery** → set to **Unrestricted** (may be called "No restrictions" or
   "Allow background activity")
3. If your phone has a **Protected apps**, **Auto-start** or **App launch**
   list, add Termux to it

> Samsung, Xiaomi, Huawei, OPPO and OnePlus are the strictest. If VayuCell keeps
> stopping when the screen turns off, this step is why.

---

## When something goes wrong

| What you see | What it means | What to do |
| --- | --- | --- |
| `curl: not found` | Termux is fresh | `pkg update && pkg install curl`, then run the installer again |
| Install fails right after the F-Droid step | You have the Play Store Termux | Uninstall it, install from F-Droid instead |
| `the build did not finish` | Out of space or memory | Free up 2 GB, close other apps, re-run the installer |
| It stops when the screen turns off | Android killed it | Step 6 |
| `vayucell: command not found` | PATH not picked up yet | Close Termux fully and reopen, or run `~/.vayucell/bin/vayucell status` |
| The page will not open from my laptop | Different networks | Both devices must be on the same Wi-Fi. Guest networks usually block this |
| Status says `UNSAFE` | Almost certainly correct | Read Step 3 — it is a true statement about your phone, not a fault |

Running the installer again is always safe. It will not duplicate anything.

---

## Removing it

```bash
rm -rf ~/.vayucell
```

That is everything. VayuCell writes nothing outside that folder, installs no
system service, and leaves no account anywhere. If you also want Termux gone,
uninstall it like any app.

---

## What this has and has not been tested on

**No VayuCell release has been installed on a physical phone by its author.**

Every device-facing behaviour is exercised against a simulated device in the
test suite — 234 tests, and every safety check is deliberately re-broken in CI
to prove the tests would notice. That is a real standard and it is not the same
as a phone on a bench.

You are, right now, closer to a first tester than a user. If you run this, a
[device report](https://github.com/johalputt/VayuCell/issues/new?template=device-report.yml)
is the most useful thing you can contribute — including one that says it did not
work.
