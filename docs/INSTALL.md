# Installing VayuCell on a phone

Written for somebody who has never opened a terminal. If a step assumes
something you do not have, that is a defect in this page — please
[say so](https://github.com/johalputt/VayuCell/issues/new).

---

## What you will have at the end

An old phone on your Wi-Fi, running a program that watches its battery and
shows you a page describing exactly what it has and has not verified.

It can also **host a website** — a folder of files, served to your own network,
which stops being served the moment the phone says its battery is in trouble.
See [Step 7](#step-7--host-a-website-optional).

**What it does not do yet, stated plainly:** it does not store your files, and
nothing it serves is reachable from outside your own network. There is no
sync, no upload, no sharing a link with somebody in another building. Those are
the point of the project and they are not written.

If you want a file store **today**, this is not that yet.

---

## What you need

| | |
| --- | --- |
| A phone | Android 7 or newer. It does not need to be unlocked, rooted, or wiped |
| A charger | It stays plugged in |
| Wi-Fi | The same network as the computer or phone you will read the page from |
| Time | About five minutes |

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
- download the build for your phone and **refuse it if the checksum does not
  match** — a download nobody verified is just a download
- check the program actually runs before claiming success

This takes **under a minute** on a normal connection: the installer downloads a
published build for your phone and checks it against a signed checksum before
trusting it. If no build exists for your processor it says so and compiles from
source instead, which takes 10–20 minutes — leave the screen on and the phone
plugged in if that happens.

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

## Step 7 — Host a website (optional)

Put your files in a folder — `index.html` and whatever it needs — and:

```bash
vayucell site --dir ~/mysite --bind 0.0.0.0:8080
```

Open `http://<phone-ip>:8080` from any device on your Wi-Fi. If you do not have
a site yet, three lines is a site:

```bash
mkdir -p ~/mysite
echo '<h1>Served from a phone in a drawer</h1>' > ~/mysite/index.html
vayucell site --dir ~/mysite --bind 0.0.0.0:8080
```

### What it will not do, on purpose

| It refuses | Why |
| --- | --- |
| Any file or folder whose name starts with a dot | This is how a `.git` or a `.env` full of passwords leaves a building. Refused as a class, not by a list of the ones somebody thought of |
| A folder with no `index.html` | It will not generate a listing. A listing publishes everything you happened to leave in that folder |
| A shortcut pointing outside your site folder | Checked against the real filesystem, not against the address that was typed |
| Anything with `..` in the address | Refused, not "cleaned up and served anyway" |
| Publishing to the internet | It binds your own network only. Nothing forwards a port and nothing registers a name |

Every one of those answers the same `404`, deliberately: if a refused file
answered differently from a missing one, somebody could work out what you have
by trying addresses. The real reason is printed in Termux, where only you see it.

### The part no other file server does

**The battery governor is asked on every single request.** If the phone gets hot
and drops to `PROTECT`, your site stops answering and tells visitors that the
device is protecting its battery. If the power goes out and the phone works down
its shutdown ladder, your site is one of the first things it stops.

That is not a fault, and it is not something to work around. A phone that keeps
serving a webpage while its cell is in trouble is the exact failure this whole
project exists to prevent.

### Keep the panel too

Run `vayucell-start` in a second Termux session (swipe from the left edge → **New
session**). The panel and the site are separate ports on purpose, so a page on
your site can never read the screen that reports whether your battery is safe.

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
| My site says `Service Unavailable` | The governor withheld it | Read the message — it says whether the cell is hot or the phone is on battery. This is the software working |
| `site needs --dir` | No folder given | There is no default on purpose, so it cannot publish whatever folder you were in. Pass `--dir ~/mysite` |
| My site shows `404` for a page that exists | A dot-name, a folder with no `index.html`, or a shortcut leading outside | Termux prints the real reason under the command |

Running the installer again is always safe. It will not duplicate anything.

---

## Removing it

```bash
rm -rf ~/.vayucell
```

That is everything. Your site folder is yours and is not touched by this —
VayuCell only ever reads it. VayuCell writes nothing outside that folder, installs no
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
