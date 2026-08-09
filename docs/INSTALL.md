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

And it can **store files** — upload, download and delete, with every device that
may do so holding its own credential you can revoke on its own. See
[Step 8](#step-8--store-files-optional).

All of it runs from a single command — [Step 9](#step-9--the-command-to-actually-leave-running).

**What it does not do yet, stated plainly:** nothing it serves is reachable from
outside your own network. There is no sync, no folder that mirrors itself onto
your laptop, no phone app, and no link you can send to somebody in another
building. Putting a file on it means typing a command, or pointing something you
already have at the address yourself.

If you want a file store that **syncs on its own**, this is not that yet.

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

For trying things out, run `vayucell-start` in a second Termux session (swipe
from the left edge → **New session**). The panel and the site are separate ports
on purpose, so a page on your site can never read the screen that reports
whether your battery is safe.

Once you are past trying things out, do not run them separately at all —
[Step 9](#step-9--the-command-to-actually-leave-running) runs everything from one
command, and explains why that is a safety matter rather than a tidiness one.

---

## Step 8 — Store files (optional)

A folder on the phone that your laptop can put files into and take them back
out of. Three commands, and the first one is the one that matters.

### 8a — Enrol the device that will be allowed in

```bash
vayucell enrol --device laptop
```

It prints a long random secret. **Copy it now.** There is no command that shows
it again — a credential a program will re-display is one that leaks through a
scrollback or a screen share. If you lose it, run `enrol` again with a different
name; that takes five seconds.

> **Why can I not choose a password?** Because this project has no
> dependencies, it has no password-hashing library, and writing one by hand
> would be the worst possible use of that rule. So the secret is not chosen at
> all — it is 256 bits from the kernel's random source, which is far past
> guessing. See [ADR-0010](adr/ADR-0010-per-device-credentials.md).

Enrol one per device — laptop, phone, whatever else. That is the point: you can
revoke one without disturbing the others.

### 8b — Start the vault

```bash
mkdir -p ~/files
vayucell vault --dir ~/files --bind 0.0.0.0:8080
```

It prints how many devices are enrolled. **If that number is zero, every request
is refused** — "nobody enrolled" never quietly means "authentication off".

### 8c — Put a file on it, and take it back

On your laptop, on the same Wi-Fi. Put the secret from step 8a where the
`Bearer` goes:

```bash
# upload
curl -T ./report.pdf http://<phone-ip>:8080/report.pdf \
     -H 'Authorization: Bearer PASTE-THE-SECRET-HERE'

# download
curl -O http://<phone-ip>:8080/report.pdf \
     -H 'Authorization: Bearer PASTE-THE-SECRET-HERE'

# delete
curl -X DELETE http://<phone-ip>:8080/report.pdf \
     -H 'Authorization: Bearer PASTE-THE-SECRET-HERE'
```

### Managing the devices

```bash
vayucell devices                      # what is enrolled — never a secret
vayucell revoke --device laptop       # that one credential stops working
```

Revoking rewrites the list. **A vault that is already running still holds the
old one, so stop it (`Ctrl+C`) and start it again** — the command says so too.

### What it will not do, on purpose

| It refuses | Why |
| --- | --- |
| A request with no credential, or a wrong one | Checked **first**, before the name, before the battery, before the disk. Somebody who is not enrolled learns exactly one thing: that they are not enrolled |
| A name containing `/` or `\`, or `..` | This stores files, not folders. A name that is really a path is the oldest way out of a directory there is |
| A name starting with a dot | The same rule the website uses. `.env` and `.ssh` do not become storable by being uploaded instead of served |
| An upload while the battery is in trouble | Stricter than the website: a page keeps being served at `DERATED`, an upload does not. A refused upload costs one retry; a half-written file outlives the event that interrupted it |
| An upload past the quota | 1 GB by default, `--quota <BYTES>` to change it. What the folder already holds is **measured before every upload**, and if the folder cannot be read at all the upload is refused rather than waved through |

### The receipt never says "saved"

A successful upload answers with what actually happened — the bytes were
written, flushed, and renamed into place — and stops there. It does not tell you
your file is safe. Nothing was copied anywhere else, this is one phone, and a
phone can be dropped. **Keep your only copy somewhere else.**

### Keep it on its own port

Never point the vault and the website at the **same folder** — that would
publish everything anybody uploads. And do not run them as two separate
commands either. Run them together, with the next step.

---

## Step 9 — The command to actually leave running

Steps 5, 7 and 8 each start one thing, which is the right way to *learn* what
this does. It is the wrong way to leave it running. This is the whole lot, in
one command:

```bash
vayucell all --site-dir ~/mysite --vault-dir ~/files --bind 0.0.0.0:8080
```

It prints exactly what it is doing:

```text
vayucell: one governor, 3 surface(s):
  panel  http://0.0.0.0:8080/   is the battery safe
  site   http://0.0.0.0:8081/   /data/data/com.termux/files/home/mysite
  vault  http://0.0.0.0:8082/   /data/data/com.termux/files/home/files
```

**One Termux session. Three addresses, counted up from the one you gave.** Leave
off `--site-dir` and no website is served; leave off `--vault-dir` and no storage
is. It says which, rather than letting you find out.

### It is also the only command that protects the battery

Steps 5, 7 and 8 all **ask** the governor before answering a request — that is
what makes them stop when the cell is in trouble. None of them **runs** one.
Nothing samples the cell on a schedule, nothing writes a charge ceiling, and a
`HALT` reached while they are running is forgotten as soon as the phone cools.

`vayucell all` runs the supervisor as well: it holds the charge ceiling on a
phone that can hold one, re-reads the cell on its own cadence, and treats a
`HALT` as final — it stops, and says so, rather than carrying on serving.

> **If your phone cannot hold a ceiling** — which Step 3 says is almost certainly
> the case — this changes nothing about the charging, because nothing can. It
> still samples, still escalates, and still stops. The ceiling is held on the
> devices that can, and the panel tells you which yours is.

### Why this is not just a convenience

If you run the site and the vault as two separate commands, **you get two
copies of the shutdown ladder.** That ladder is what walks the phone down
through its stages during a power cut, and it latches: once a stage is entered it
is never walked back up.

Two copies, started at different moments, can disagree about which stage the
phone has reached — and the one that disagrees in the reassuring direction is the
one that carries on serving after the other has already stopped. One phone has
one battery, so it should have one ladder.

`vayucell all` is one process with one governor and one ladder, and every
surface asks the same one.

It asks in the strictest way available, too: each request is answered on the
**worse** of two readings — what the cell is doing right now, and the worst the
supervisor has seen and latched. A phone that just got hot is refused before the
supervisor's next sample, and a phone that halted and then cooled stays refused
until a person has looked at it.

### They are still separate ports, on purpose

The panel says whether your battery is safe. The site serves whatever you put in
a folder. Your browser stops one reading the other by checking the **origin** —
and it counts a different port as a different site, while a different path on the
same port is the same site. So they get ports, not paths.

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
| Uploading says `401` | The credential is missing, wrong, or nobody is enrolled | Check the `Authorization: Bearer …` header, and run `vayucell devices`. An empty list refuses everything |
| Uploading says `401` right after revoking somebody else | The vault is still running with the old list | Stop it with `Ctrl+C` and start it again |
| Uploading says `503` | The battery governor or the outage ladder withheld it | Read the message — it names which. Downloads still work at `DERATED`; uploads do not. Wait, or plug the phone in |
| Uploading says `503` and mentions the folder "could not be read" | The vault folder was moved, deleted, or its permissions changed | Check `ls -ld ~/files`. Usage that cannot be measured refuses the upload rather than assuming there is room |
| Uploading says `507` | The quota is used up | Delete something, or restart with a larger `--quota`. Replacing a file needs room for both copies until the new one lands |
| Uploading says `400` | The filename is really a path, hidden, or ends in a space or a dot | The message names which rule and what to change |
| `vault needs --dir` | No folder given | There is no default on purpose. Pass `--dir ~/files` |
| `all counts three ports from --bind` | The address has no number to count from | Use a numeric address and port, like `--bind 0.0.0.0:8080`, not a name |
| `all needs --site-dir, --vault-dir, or both` | Neither was given | With neither, `all` would just be the panel. Say which one you want served |
| `Address already in use` on one of the three | Something is already on that port | Another copy is probably still running. Close it, or pick a different `--bind` |
| `the governor has halted` and everything stops | The cell crossed the hard-stop threshold | This is the software working, and it is meant to be final. **Unplug the phone and look at it** — Step 4. Restarting does not clear the reason it halted |
| `this command consults the governor but does not run one` | You started `site` or `vault` on its own | Fine for trying things out. For anything you leave running, use `vayucell all` |

Running the installer again is always safe. It will not duplicate anything.

---

## Removing it

```bash
rm -rf ~/.vayucell
```

That is everything — the program, and the list of enrolled devices with it, so
every credential you handed out stops working.

**Your folders are not touched.** The site folder VayuCell only ever reads; the
vault folder and everything uploaded into it stays exactly where it is, and
removing VayuCell does not delete a single stored file. VayuCell writes nothing
outside those two folders, installs no system service, and leaves no account
anywhere. If you also want Termux gone, uninstall it like any app.

---

## What this has and has not been tested on

**No VayuCell release has been installed on a physical phone by its author.**

Every device-facing behaviour is exercised against a simulated device in the
test suite — 328 tests, and every safety check is deliberately re-broken in CI
to prove the tests would notice. That is a real standard and it is not the same
as a phone on a bench.

You are, right now, closer to a first tester than a user. If you run this, a
[device report](https://github.com/johalputt/VayuCell/issues/new?template=device-report.yml)
is the most useful thing you can contribute — including one that says it did not
work.

There is a command for it:

```bash
vayucell report
```

It prints what your phone actually exposes — which battery files the kernel
provides, which it does not, and whether any charge limit exists — and **sends
nothing anywhere**. It opens by listing what it contains and what it leaves out,
so read it, delete anything you would rather not post, and paste the rest.
