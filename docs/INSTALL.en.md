# Installation Guide

**[中文](INSTALL.md)** · back to [README](README.en.md)

> **Audience and purpose**: anyone installing this suite on a reMarkable Paper Pro Move for the first time, uninstalling it,
> or restoring it after a firmware update (OTA). Read "Before you install" and "Recommended install order" first, then run one
> command; if something goes wrong see "Risk warnings" and "Running into trouble". For what the whole thing is, read
> [`OVERVIEW.md`](OVERVIEW.md) (Chinese). Per-script details are in [`../packaging/README.md`](../packaging/README.md) (Chinese).

## Scope

**reMarkable Paper Pro Move (imx93-chiappa), firmware 3.28.0.172** — the only firmware version
verified on real hardware so far; the installer checks this automatically before doing anything
(see "Firmware safety gate" below). Other firmware versions or other reMarkable models are
unverified — forcing an install there risks misaligned QML injection offsets (best case, a
feature silently doesn't work; worst case, it affects normal device operation).

## Before you install: a few things you need to do by hand

These are reMarkable's own / third-party ecosystem infrastructure, not part of this
repository — `install-all.sh` will **not** install them for you. If one is missing, the
relevant step fails with a clear message telling you what to run:

1. **`vellum add xovi`** — the [xovi](https://github.com/asivery/xovi) extension loader itself.
   Most of what this repository does runs as xovi extensions, so this is the most basic
   prerequisite.
2. **`vellum add qt-resource-rebuilder`** — QML hot-resource replacement. A few optional
   features in `shelf`/`gateway` (the font menu, the trash/new-folder web proxy) depend on it;
   without it, those two features are silently skipped and nothing else is affected.
3. **`vellum add appload`** — the third-party app loader.
4. Sideload **KOReader** through appload.

For how to install vellum itself, or the specifics of sideloading appload/KOReader, refer to
vellum's and the reMarkable community's own documentation — this is general device
infrastructure that isn't maintained by this project, so it isn't repeated here.

## Recommended install order

Doing it in this order minimizes rework (the numbered risk items referenced below are in "Risk
warnings" further down):

1. **Check the firmware version first** — under Settings, only 3.28.0.172 is verified so far
   (see "Scope" above).
2. **Install the ecosystem infrastructure by hand** (previous section), in dependency order —
   don't skip a step:
   1. `vellum add xovi`
   2. `vellum add qt-resource-rebuilder`
   3. `vellum add appload` — once installed, confirm the native "AppLoad" icon actually shows
      up in the sidebar (⚠ risk ①: the official release may not work correctly on 3.28; fix
      that first, or the Sidebar-entry step later will silently skip)
   4. Sideload KOReader through appload
   5. (optional) Want the WeRead sidebar shortcut? Install and log into WeRead first, following
      its own official release instructions

   ⚠ **Leave a gap of a few minutes between steps 3 and 5 — don't do them back-to-back** (see
   risk ③): confirming the AppLoad icon may itself require restarting xochitl once, and WeRead
   stops/starts xochitl once each on launch and on exit. Stacking several restarts in a short
   window has, on real hardware, actually tripped the crash-loop protection and triggered an
   unplanned full device reboot (2026-09-11). Wait until step 3 shows `is-active`=active and
   stable for a few minutes before doing step 5.
3. **Run `install-all.sh`** (no `--skip`, install everything at once):
   ```sh
   cd packaging && sh install-all.sh 10.11.99.1
   ```
4. **Check the closing summary** — confirm the "installed" list matches what you expected, and
   nothing was silently skipped that you actually wanted (skipped ≠ failed, easy to miss — see
   risks ①③). Fix any failures first; everything else already landed, no need to re-run the
   whole thing.
5. **Change the password** — open the gateway in a browser; first login forces a redirect to
   the change-password page.
6. **Verify the Sidebar entry by eye** (if that step wasn't skipped) — go back to the device's
   main screen, confirm the expected entry shows up under KOReader in the sidebar, and click it
   to confirm it actually launches — no script can confirm this step for you.

Walking through the manual prerequisites first, then installing everything with one command,
then eyeballing the UI-level changes last, is the order with the fewest surprises so far; doing
it the other way around (install everything first, only later discover some manual prerequisite
was missing) tends to look like "a few steps silently failed" and takes longer to debug.

## Install everything with one command

Connect your computer to the device over USB (reMarkable configures itself as `10.11.99.1` on
that link by default):

```sh
git clone https://github.com/bbq191/rm-tweak.git   # public release; see the note below
cd rm-tweak/packaging
sh install-all.sh 10.11.99.1
```

> About the repository address: `cang-jie` (`git@github.com:bbq191/cang-jie.git`) is the **private** repository with the full
> development history — only the maintainer can clone it. What is shared publicly is the trimmed release `rm-tweak` (see the top of the
> [README](README.en.md)). The command above uses rm-tweak; its layout (including `packaging/`) is stated by the README to match this
> repository, but **the contents of the public repository have not been verified here** — if `packaging/install-all.sh` is missing
> after cloning, treat the private repository as authoritative.

This runs the following steps in order (each can also be run on its own — see "Installing only
part of it" below):

| Step (name for `--skip`) | What it does | Prerequisite |
|---|---|---|
| Firmware safety gate | Checks the device's firmware against the verified version | — |
| `chrony-cn` | Swaps chrony's servers for reachable ones (Aliyun/Tencent Cloud, etc.) | — |
| `chrony-boot-wakelock` | Holds a wakelock for the first minute after boot so autosuspend can't interrupt chronyd's first sync | — |
| `timezone-cn` | Sets Asia/Shanghai | — |
| `battop` | Battery diagnostics sampler; started after install but **not enabled at boot** (deliberate, see risk ⑥); toggle it under Manage → Battery Detective in the web UI or `systemctl start battop` | — |
| `wifi-watch` | WiFi watchdog: if wlan0 loses carrier it runs `nmcli con up`, and pins the 2.4G band with power-save off; zero forks while the link is healthy | — |
| `xovi-persist` | Installs a unit that re-runs `xovi/start` automatically on every boot, so you no longer have to do it by hand after a reboot | requires `vellum add xovi` |
| `hl-snap` | Precise CJK highlight-snapping (snaps exactly what you drag, not "drag a bit, snap the whole line"); stages files only | same |
| `handwriting-stroke` | Tunes stroke thickness by pen angle/speed; stages files only | same |
| `sidebar-entry` | A shortcut straight to "KOReader" in the sidebar; if the third-party WeRead app is installed, adds a "WeRead" entry too; stages files only | requires `vellum add qt-resource-rebuilder appload` (see risk ① below) |
| `shelf` | Nine web services: gateway, book management (book/koreader), upload-and-use fonts/wallpapers, the four notes services | — |
| `xovi-apply` | Once the xovi extensions above are staged, restarts xochitl exactly once at the end to make them take effect (**interrupts reading**, see risks ③⑤) | — |

When it finishes, it prints a summary: what got installed, which step (if any) failed, and what
still needs to be done by hand (vellum bootstrap, KOReader sideloading, etc.). No step is
retried automatically or silently skipped on failure — just follow the error message. Every
script is idempotent, so re-running the whole command is always safe (the last step first prints an
"about to interrupt reading" notice and waits 5 seconds before restarting xochitl — see risks ③⑤).

## After installing

Open `https://10.11.99.1/` in a browser (or `https://shelf.local/` on the same network segment —
note Android doesn't resolve `.local` domains):

- Default password is `shelf`; **you must change it on first login** (the system forces a
  redirect to the change-password page).
- You'll see an untrusted-certificate warning (self-signed cert) — the login page has a
  "Download CA certificate" link; install it into your browser/system trust store once to stop
  seeing the warning, or just click through "Advanced → Proceed" for a one-off visit.

## Firmware safety gate

Before installing anything, `install-all.sh` SSHes into the device, reads the sha256 of
`/usr/bin/xochitl`, and compares it against the verified hashes recorded in
`packaging/firmware-allowlist.txt` — it only proceeds on a match, refusing otherwise. This is
strict because features like the font menu and the trash/new-folder proxy rely on **byte-level
QML injection offsets** — even a hotfix that keeps the same version string can shift the
internal layout, so a version string alone isn't a safe enough guarantee.

If you've verified this exact firmware yourself and just need to register its hash: pass
`--force`. The current hash is appended to a **local** file, `packaging/firmware-allowlist.local.txt` (gitignored, never committed; the git-tracked allowlist is not modified). After that the same firmware no longer needs `--force`.

```sh
sh install-all.sh 10.11.99.1 --force
```

## Installing only part of it / skipping steps

```sh
sh install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist
```

Skippable step names: `chrony-cn`, `chrony-boot-wakelock`, `timezone-cn`, `battop`, `wifi-watch`, `xovi-persist`,
`hl-snap`, `handwriting-stroke`, `sidebar-entry`, `shelf`, `xovi-apply`. A misspelled name does not abort, but prints a
"not a known step name" warning listing the known ones. Each step's underlying script (`packaging/deploy-<step>.sh <host>`;
shelf is `deploy.sh`) can also be run on its own, independent of `install-all.sh`.

**Install only the bookshelf and set the gateway password**: `cd packaging && sh deploy.sh 10.11.99.1 --only book,koreader --password 'NewPassword'`. The valid `--only` tokens are `gateway book koreader font wallpaper ink transcribe mind note` (the gateway is always installed); any other token makes the on-device `install.sh` exit with status 2. The `--password` value is sent over ssh's standard input into a 0600 temp file on the device and read (then deleted) by `install.sh --password-file` — spaces, quotes or semicolons are never interpreted by a remote shell and never show up in `ps`.

**File dependencies if you run the on-device `install.sh` by hand**: `shelf/install.sh` needs `manifest.sh` and `devlib.sh` in the same directory (`deploy.sh` packs them into the payload — copying only `install.sh` is not enough); the `deploy/install.sh` of `hl-snap`/`handwriting-stroke` needs `xovi-ext-install.sh` and `devlib.sh` next to it (`deploy-hl-snap.sh` and friends push them together); `battop/install.sh` needs `devlib.sh` next to it.

## What this installer deliberately does not do

- **Doesn't install vellum/xovi/qt-resource-rebuilder/appload themselves, doesn't sideload
  KOReader** — see "Before you install" above; these remain manual prerequisites.
- **Doesn't install the Chinese input method** — that line was archived out of this repository
  (see the top-level [README](README.en.md), "History and scope") and isn't distributed by this
  installer.
- **Doesn't apply appload's 3.28 compatibility patch** — that is a separate manual step, `packaging/deploy-appload-patch.sh <host>`, not part of the orchestration (see risk ①).

## Uninstalling

`packaging/uninstall-all.sh` removes everything `install-all.sh` installs, and shares the **same step table**, so the two are symmetric:

```sh
cd packaging
sh uninstall-all.sh 10.11.99.1                    # remove everything
sh uninstall-all.sh 10.11.99.1 --skip shelf       # skip a step (same names as above; a typo warns)
sh uninstall-all.sh 10.11.99.1 --purge            # additionally delete battop's binary and sample history
```

- `chrony-cn` and `timezone-cn` are config overwrites and `xovi-apply` is a pure action: none has uninstall semantics, so they are left alone. vellum/xovi/qt-resource-rebuilder/appload and the KOReader sideload were never installed by this project and are not removed.
- The `shelf` step prefers the device's `~/.local/bin/shelf-uninstall` (the single source of truth) and falls back to the copy in `shelf-pkg`; user data (book masters, config, certificates, font/wallpaper pools) is kept by default. `--purge` does **not** apply to shelf — to delete its data, `--skip shelf` and run `shelf-uninstall --purge` on the device yourself.
- After removing xovi extensions/qmds, the running xochitl still holds the old mappings until its next restart; the uninstaller does **not** restart it. To restart: `systemctl restart xochitl` (never `xovi/start` while xovi is already active — see risk ⑤).

## Backups and idempotency

Every install script is idempotent and backs up what it is about to overwrite on the device:

- Backups always go into `/home/root/cangjie-backups/` (**never** inside `extensions.d/` — xovi loads any file in that directory as an extension, and registering the same extension twice is fatal). Single-file backups are named `<file>.bak.pre-<timestamp>`; shelf uses a `shelf-<timestamp>/` directory.
- Only the most recent **5** are kept (device-side environment variable `CJ_BACKUP_KEEP` adjusts this), and only backups the scripts generated themselves with strictly timestamped names are rotated; hand-named backups, single files over 64MB and any user data are never auto-deleted.
- Unchanged content means nothing is touched (no service restart, no duplicate backups); before writing `/usr` the scripts check dm-verity and skip if it is active (writing `/usr` once triggered an A/B rollback brick, 2026-08-16).

For the full architectural decisions, every real-hardware pitfall found along the way, and the
current verification status, see [`packaging/README.md`](../packaging/README.md) — that document is
aimed at engineering detail; this one is meant as a quick-start guide for a first install.

## Risk warnings: which modules are prone to trouble

These aren't "random low-probability glitches" — they're known issues with clear trigger
conditions. Knowing about them ahead of time saves a lot of guessing later.

① **The official AppLoad release (v0.5.3) has a compatibility issue on 3.28 firmware, and it
   fails silently — but it won't fail to install, won't stop xochitl from starting, and won't
   brick the device.** AppLoad's own built-in injection patch targets 3.27's old UI anchors,
   which were renamed in 3.28 — without a third-party compatibility patch, the launcher
   component AppLoad injects into the UI never gets built, and `journalctl` logs a qmldiff-level
   "Couldn't resolve the hashed identifier". **This has actually happened on this exact
   device's history** (2026-09-06, when the unpatched original v0.5.3 was installed):
   **`vellum add appload` itself installed successfully** (a plain file-level install that
   doesn't check firmware version), **and xochitl started and worked normally** — the only
   observable symptom was the AppLoad icon never showing up / not being clickable. That's "one
   feature didn't take effect", not "failed to restart" and definitely not "bricked the
   device". The kind of issue that actually can brick a device or prevent it from booting
   (breaking xochitl's systemd startup dependencies into a deadlock) is a completely different
   category of accident from a missing QML anchor — the two mechanisms don't interact.
   **Symptom**: the `sidebar-entry` step detects this automatically and skips (not an error, not
   a failed install) — the sidebar simply won't show a KOReader/WeRead entry, which is easy to
   mistake for "this feature was never built". **How to tell if you hit this**: check whether
   `install-all.sh`'s summary lists `sidebar-entry` as "installed" or "skipped"; if skipped and
   you actually need the shortcut, see the fix pointer under item 3 of "Before you install" in
   `packaging/README.md`.
② **Missing `qt-resource-rebuilder` silently disables several unrelated-looking features at
   once, easy to mistake for a broken install.** The font-menu enhancement, the trash/new-folder
   web proxy, and the Sidebar entry — three otherwise-unrelated features — all share the same
   prerequisite (`vellum add qt-resource-rebuilder`). Miss this beforehand, and you'll find
   several seemingly-unrelated features missing at once, easy to suspect something actually
   failed; in reality they're all the same root cause, and `install-all.sh`'s summary marks each
   one "skipped", not "failed".
③ **Restarting/stopping-and-starting xochitl repeatedly in a short window risks tripping the
   crash-loop protection — it doesn't matter who triggers it.** `xochitl.service` is currently
   configured with `Restart=on-failure` and `StartLimitBurst=4` (within a 10-minute window),
   and the trigger condition doesn't care *who* asked for the restart — this repository's own
   deploy scripts (`hl-snap`/`handwriting-stroke`/`sidebar-entry`, when run standalone, not
   through `install-all.sh`, each restart xochitl once when they finish), a third-party installer like
   `vellum add appload` restarting things on its own, or an app like WeRead that takes over the
   screen on launch and hands it back on exit (each stopping and starting xochitl once per
   round-trip) — all count. **On real hardware, just two restarts in quick succession actually
   tripped this and triggered one unplanned full device reboot** (2026-09-11, see the note
   between steps 3 and 5 in "Recommended install order") — **and that reboot was just the device
   rebooting and coming back up fine, not a brick**: `uptime` showed a clean boot afterward, and
   it incidentally exercised the `xovi-persist` boot-recovery unit installed earlier in the same
   run (see "Verification status" in `packaging/README.md`) with no lasting side effects — purely
   a few extra minutes of waiting, not lost data or a damaged device. `install-all.sh` already
   handles this
   correctly for its own steps (stage everything first, restart xochitl exactly once at the end via
   `xovi-apply`: **`systemctl restart xochitl` when xovi is already active, `xovi/start` only when it is not**,
   with an "about to interrupt reading" notice and a 5-second grace period first; use `--skip xovi-apply`
   to restart later yourself) — **this only needs your attention when you run deploy scripts by hand one at a time, or
   go back and forth between third-party apps like appload/WeRead that restart xochitl on their
   own** — leave a few minutes between each, confirm the previous restart settled into
   `is-active`=active, before doing the next thing. Don't stack them back-to-back.
④ **The firmware safety gate refusing to install isn't a bug — it's working as designed. Verify
   before you `--force`.** Features like the font menu and the trash/new-folder proxy rely on
   byte-level QML injection offsets — a matching version string doesn't guarantee the internal
   layout hasn't shifted (a silent hotfix can move it). When refused, first confirm this device's
   firmware really is identical to the one you verified before deciding to `--force` — forcing an
   install on an unverified firmware risks a feature silently not working, or worse, affecting
   normal device operation.
⑤ **The screen flashes once at the last step, `xovi-apply` — this is expected.** It restarts
   xochitl (the compositor and UI process), after first printing an "about to interrupt reading"
   notice and waiting 5 seconds. Don't use the device while installing; wait for the closing summary
   before touching it. **⚠ The right way to restart xochitl**: once xovi is active inside the running
   xochitl (its `LD_PRELOAD` contains `xovi.so`), always use `systemctl restart xochitl` and **never**
   run `xovi/start` by hand — it makes the running xochitl SEGV and the system reboots itself by design
   (2026-09-20 real-device incident; the old `install-all.sh` hit it on every re-run, the new one has
   this check built in). Use `xovi/start` only when xovi is not active (fresh boot, right after an OTA).

⑥ **battop is deliberately not enabled at boot** — on 2026-08-29 its sampling triggered a kernel
   cgroup/RCU deadlock that froze the whole device and the root cause was never fully ruled out, so
   the installer only `start`s it, never `enable`s it. Turn it on under Manage → Battery Detective in
   the web UI or with `systemctl start battop`. Restoring boot autostart is a decision you evaluate
   yourself; don't expect the installer to do it quietly.

## After a firmware update (OTA)

This is the **authoritative** OTA recovery description (`packaging/README.md`, `shelf/README.md` and the shelf white paper link here instead of keeping their own tables).

![After an OTA: what is lost, how to restore](diagrams/ota-recovery.svg)

**Updating is risk-free and loses no data; but after updating you must re-run the install to get features back** — it is not "update and it just works". By design we leave nothing on the boot path (xovi is preloaded from the `/etc` tmpfs, units live in `/usr`), so the new firmware always boots as stock; `/home` is untouched. The 3.27.3.0 → 3.28.0.172 log is in the shelf white paper §03v (Chinese).

**Recommended procedure**

1. (Before updating, optional) move xovi extensions that are incompatible with the new firmware (e.g. appload) out of `extensions.d/` into `/home/root/xovi-disabled/` — **never leave them in `extensions.d/`** (xovi loads any file there as an extension).
2. After the update, run `xovi/rebuild_hashtable` **by hand at the device** (it needs the root password interactively; `install-all.sh` will not do it for you). It is the prerequisite for qmds being injected again.
3. On the computer: `cd packaging && sh install-all.sh <device IP>`. The new firmware's sha256 is usually not in the allowlist, so the gate refuses — after confirming the device really runs the firmware you intend, add `--force` (appended to the local `firmware-allowlist.local.txt`). Every script is idempotent and fills in whatever is missing.
4. Read the closing summary and open the gateway in a browser; appload's 3.28 compatibility patch is a separate manual step (`deploy-appload-patch.sh`).

**Item by item** (what survives, what must be reinstalled, which step restores it)

| Item | Location | After OTA | Restore |
|---|---|---|---|
| Book masters / KOReader config / font and wallpaper pools / certificates / gateway password / sleep-screen conf key / `cangjie-backups/` / battop history | `/home` | kept | none |
| shelf service binaries (`~/.local/bin`) | `/home` | kept | none |
| hl-snap / handwriting-stroke `.so`, Sidebar entry, and the qmds for the font menu / trash / new-folder proxies | `/home` (`extensions.d/`, `exthome/`) | files present, but the hashtab is stale and they are not injected | `rebuild_hashtable` (step 2), then `install-all.sh` (`xovi-apply` makes them take effect) |
| systemd units of the shelf services and `shelf.target` | `/usr` | **wiped** | the `shelf` step of `install-all.sh` (or alone: `cd packaging && SHELF_NO_BUILD=1 sh deploy.sh <device IP>`) |
| xovi boot-persistence unit `xovi-reenable.service` | `/usr` | **wiped** | `xovi-persist` step |
| `chrony-boot-wakelock.service` | `/usr` | **wiped** | `chrony-boot-wakelock` step |
| battop unit `battop.service` | `/usr` (data in `/home`) | unit **wiped** | `battop` step (started after install, not enabled at boot) |
| WiFi watchdog unit `wifi-watch.service` | `/usr` (script `~/.local/bin/wifi-watch.sh` in `/home`) | unit **wiped** | `wifi-watch` step |
| Domestic NTP (chrony config), default timezone | `/etc` | **wiped** | `chrony-cn` / `timezone-cn` steps |
| appload's 3.28 compatibility patch | `/home` (xovi) | depends on whether appload was reinstalled | separate: `deploy-appload-patch.sh <device IP>` |

**Risk layers** (don't collapse them into one percentage): the shelf layer only uses xochitl's `/upload` web endpoint and standard system components, so reinstalling restores it; qmldiff injections such as the font menu depend on xochitl's internal QML and often need re-adapting on a major version (3.27→3.28 already needed two qmd variants); KOReader itself is independent, but its sidebar entry relies on the third-party appload, which may need re-patching each major version.

**After a "bare-metal restore" check extra**: an OTA itself never deletes `/home`, but if the device went through a more thorough reset, the payload under `/home` (the `.so` files in `extensions.d/`, the service binaries) can disappear with it — this really happened on 2026-09-09. Confirm those files are still there before re-running `install-all.sh`.

## Running into trouble

Start with the summary `install-all.sh` prints at the end to see exactly which step failed; the
corresponding `packaging/deploy-*.sh` script has detailed comments explaining what that step
does and its common failure causes. If you're still stuck, the closing section of `packaging/README.md` ("验证现状", verification status, Chinese) records every real-hardware issue found so far and how it was
fixed.

To check the scripts themselves without touching a real device: `bash packaging/tests/run_sim_tests.sh` (runs the real code against fake ssh/systemctl/mount in a temp directory; refuses to run as root; also invoked by the CI pytest).
