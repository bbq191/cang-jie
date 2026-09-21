# Installation Guide

**[中文](INSTALL.md)** · back to [README](README.en.md)

> **Audience and purpose**: anyone installing this suite on a reMarkable Paper Pro Move for the first time, uninstalling it,
> or restoring it after a firmware update (OTA). Read "Scope → Before you install → Install" in order and you are done; if something
> goes wrong jump to "Troubleshooting and risk items"; after a firmware update jump to "After a firmware update (OTA)".
> To learn what the whole thing is, read [`OVERVIEW.md`](OVERVIEW.md) (Chinese). How the scripts are built and tested locally is in
> [`../packaging/README.md`](../packaging/README.md) (Chinese, developer-oriented; not repeated here).

## Scope

**reMarkable Paper Pro Move (imx93-chiappa), firmware 3.28.0.172** — the only version verified on real hardware
so far; the installer checks this automatically before doing anything (see "Firmware safety gate"). Other
firmware versions or other reMarkable models are unverified — forcing an install there risks misaligned QML
injection offsets (best case a feature silently doesn't work, worst case it affects normal device operation).

## Before you install: 4 things to install by hand

These are reMarkable's own / third-party ecosystem infrastructure, not part of this repository —
`install-all.sh` will **not** install them for you; if one is missing, the relevant step fails with a clear message
telling you what to run. For how to install vellum (the on-device package manager) itself, or how to sideload
appload/KOReader, follow vellum's and the reMarkable community's own documentation; it is not repeated here.

| # | Run on the device | What it is | If missing |
|---|---|---|---|
| 1 | `vellum add xovi` | [xovi](https://github.com/asivery/xovi): the extension loader | Most of this repository runs as xovi extensions; the related steps fail outright |
| 2 | `vellum add qt-resource-rebuilder` | Loader for UI QML patches (qmd) | Font menu, trash/new-folder proxy, comic-margin proxy and the sidebar entry are **silently skipped** (not a failure); everything else is unaffected |
| 3 | `vellum add appload` (**≥ 0.6.0**) | Third-party app loader | The sidebar KOReader entry doesn't appear (the `sidebar-entry` step skips itself) |
| 4 | Sideload KOReader through appload | The second reader | `koreader-serve` only manages an already-installed KOReader, it doesn't install it; the sidebar entry does nothing when tapped |

Optional: the third-party **WeRead** app (WeChat Read for reMarkable). If installed, `sidebar-entry` detects it and adds a "WeRead" entry too; not installing it affects nothing.

## Install

![install-all.sh flow](diagrams/install-flow.svg)

### Recommended order

1. **Check the firmware version**: only 3.28.0.172 is verified so far (Settings → software version).
2. **Install the 4 prerequisites by hand, in dependency order** (vellum add xovi → qt-resource-rebuilder → appload → sideload KOReader; optionally WeRead). After installing appload, confirm the native "AppLoad" icon shows up in the sidebar (see issue ①).
   ⚠ **Leave a few minutes between the appload step and the WeRead step; don't do them back-to-back**: checking the icon may restart xochitl once, and WeRead stops/starts xochitl once on each launch and exit; stacking restarts in a short window can trip the restart protection and reboot the whole device (seen on real hardware, 2026-09-11, see issue ③).
3. **Run one command** (computer connected over USB; the device is `10.11.99.1` on that link by default):
   ```sh
   git clone https://github.com/bbq191/rm-tweak.git   # public release; the private dev repo cang-jie is maintainer-only
   cd rm-tweak/packaging
   sh install-all.sh 10.11.99.1
   ```
   > About the repository: the public release is stated by its README to match this repository's layout, but **whether the public repo actually contains `packaging/install-all.sh` was not checked file by file**; if it is missing after cloning, treat the private repo as authoritative.
4. **Read the closing summary**: check the "installed" list matches what you expect. "Skipped" is not "failed" and is easy to miss (see issues ①②). Fix failures first; everything else already landed. Re-running is always safe (all scripts are idempotent).
5. **Change the password**: open the address below in a browser; the first login forces a redirect to the change-password page.
6. **Verify the sidebar entry by eye** (if that step wasn't skipped): on the device's home screen confirm the expected entry appears under KOReader and opens — no script can confirm this for you.

### What each step installs

`install-all.sh` first runs the firmware safety gate, then the steps below in order. Every **step name** works with `--skip`; the matching script `packaging/deploy-<step>.sh <host>` (the `shelf` step is `deploy.sh`) can also be run on its own, independent of `install-all.sh`.

| Step | What it does | Prerequisite |
|---|---|---|
| `chrony-cn` | Swaps chrony's time servers for reachable ones (Aliyun/Tencent Cloud, etc.) | — |
| `chrony-boot-wakelock` | Holds a wakelock at boot (released as soon as the clock syncs, at most 120 s) so autosuspend can't interrupt chronyd's first sync | — |
| `timezone-cn` | Sets the default timezone to Asia/Shanghai | — |
| `battop` | Battery diagnostics sampler; started after install but **not enabled at boot** (deliberate, see issue ⑥) | — |
| `wifi-watch` | WiFi carrier watchdog: if wlan0 goes carrier-dead it runs `nmcli con up`, and pins the 2.4G band with power-save off; zero forks while the link is healthy | — |
| `xovi-persist` | Installs a unit that re-runs `xovi/start` automatically at every boot, so you no longer do it by hand after a reboot | `vellum add xovi` |
| `hl-snap` | Precise CJK highlight snapping (snaps exactly what you drag, not "drag a bit, snap the whole line"); stages files only | same |
| `handwriting-stroke` | Tunes handwriting stroke width by pen angle/speed (off by default, enabled in the web UI's "Lab" tab); stages files only | same |
| `sidebar-entry` | A sidebar shortcut to "KOReader"; adds "WeRead" too if the WeRead app is installed; stages files only | qt-resource-rebuilder and appload installed (see issue ①) |
| `shelf` | Nine web services: gateway, book management (book / koreader), fonts/wallpapers (font / wallpaper), the four notes services (ink / transcribe / mind / note); related qmds are staged only | — (the qmds need qt-resource-rebuilder; skipped automatically if missing) |
| `xovi-apply` | Once everything "staged only" above is in place, **restarts xochitl exactly once** to make it take effect (**flashes the screen, interrupts reading**, see issues ③⑤) | — |

A failed step is neither retried automatically nor silently skipped — just follow the error message.

### After installing

Open `https://10.11.99.1/` in a browser (or `https://shelf.local/` on the same network segment; Android doesn't resolve `.local` domains):

- The default password is `shelf`; **you must change it on first login** (the system forces a redirect to the change-password page).
- You'll see an untrusted-certificate warning (private CA): the login page has a "Download CA certificate" link — install it into your browser/system trust store once and the warning goes away; for a one-off visit just click "Advanced → Proceed".

## Common options

### Firmware safety gate

Before installing anything, `install-all.sh` SSHes into the device, reads the sha256 of `/usr/bin/xochitl`, and compares it with `packaging/firmware-allowlist.txt` (verified hashes recorded in the repo) plus the local `firmware-allowlist.local.txt`: it proceeds only on a match and refuses otherwise. A hash is used rather than a version string because features like the font menu and the trash proxy rely on **byte-level QML injection offsets**, and even a hotfix with the same version string can shift the internal layout.

If you have verified that this exact firmware works and only its hash isn't registered, pass `--force`. The current hash is appended to the **local** file `packaging/firmware-allowlist.local.txt` (gitignored, never committed; the git-tracked allowlist is not modified); afterwards the same firmware no longer needs `--force`.

```sh
sh install-all.sh 10.11.99.1 --force
```

### Installing only part of it

```sh
sh install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist    # skip the named steps
```

Skippable step names are the first column of the table above; a misspelled name does not abort but prints a "not a known step name" warning listing the known ones.

**Install only the bookshelf and set the gateway password**:

```sh
cd packaging && sh deploy.sh 10.11.99.1 --only book,koreader --password 'NewPassword'
```

The valid `--only` tokens are `gateway book koreader font wallpaper ink transcribe mind note` (the gateway is always installed; any other token makes the on-device `install.sh` exit with status 2). The `--password` value is sent over ssh's standard input into a 0600 temp file on the device and deleted after `install.sh` reads it — spaces or quotes are never interpreted by a remote shell and never show up in `ps`.

## Uninstall

`packaging/uninstall-all.sh` shares the **same step table** as the installer, so the two are symmetric:

```sh
cd packaging
sh uninstall-all.sh 10.11.99.1                    # remove everything
sh uninstall-all.sh 10.11.99.1 --skip shelf       # skip a step (same names as above; a typo warns)
sh uninstall-all.sh 10.11.99.1 --purge            # additionally delete battop's binary and sample history
```

- `chrony-cn` and `timezone-cn` are config overwrites and `xovi-apply` is a pure action: none has uninstall semantics and they are left alone. vellum / xovi / qt-resource-rebuilder / appload and the KOReader sideload were never installed by this project and are not removed.
- The `shelf` step prefers the device's `~/.local/bin/shelf-uninstall` (the single source of truth) and falls back to the copy in `shelf-pkg`; user data (book masters, config, certificates, font/wallpaper pools) is kept by default. `--purge` does **not** apply to shelf — to delete its data, `--skip shelf` first, then run `shelf-uninstall --purge` on the device.
- After removing xovi extensions/qmds, the running xochitl still holds the old mappings until its next restart; the uninstaller does **not** restart it. To restart: `systemctl restart xochitl` (never `xovi/start` while xovi is already active — see issue ⑤).
- **Not verified on real hardware**: the uninstaller has only been exercised in local simulation (fake ssh/systemctl); whether every unit/extension is actually removed on a real device, and whether other features stay unaffected, remains to be verified.

## After a firmware update (OTA)

This is the **authoritative** OTA recovery description (`packaging/README.md`, `shelf/README.md` and the shelf white paper link here instead of keeping their own copies).

![After an OTA: what is lost, how to restore](diagrams/ota-recovery.svg)

**Updating itself is risk-free and loses no `/home` data; but after updating you must re-run the install to get features back** — it is not "update and it just works". By design we leave nothing on the boot path (xovi is preloaded from the `/etc` tmpfs, units live in `/usr`), so the new firmware always boots as stock. The 3.27.3.0 → 3.28.0.172 log is in the shelf white paper §03v (Chinese).

### Recommended procedure

1. (Before updating, optional) move xovi extensions that are incompatible with the new firmware (e.g. an old appload) out of `extensions.d/` into `/home/root/xovi-disabled/` — **never leave them in `extensions.d/`** (xovi loads any file there as an extension).
2. After the update, run `xovi/rebuild_hashtable` **by hand at the device** (it needs the root password interactively; `install-all.sh` will not do it for you). It is the prerequisite for qmds being injected again.
3. On the computer: `cd packaging && sh install-all.sh <device IP>`. The new firmware's sha256 is usually not in the allowlist, so the gate refuses — after confirming the device really runs the firmware you intend, add `--force`. Every script is idempotent and fills in whatever is missing.
4. Read the closing summary and open the gateway in a browser. appload needs to be ≥ 0.6.0 (upgrade an older one with `vellum upgrade appload` and reboot), outside the orchestration.

### Item by item

| Item | Location | After OTA | Restore |
|---|---|---|---|
| Book masters / KOReader config / font and wallpaper pools / certificates / gateway password / sleep-screen conf key / `cangjie-backups/` / battop history | `/home` | kept | none |
| shelf service binaries (`~/.local/bin`) | `/home` | kept | none |
| hl-snap / handwriting-stroke `.so`, sidebar entry, and the qmds for the font menu / trash / new-folder proxies | `/home` (`extensions.d/`, `exthome/`) | files present, but hashtab is stale and they are not injected | `rebuild_hashtable` (step 2), then `install-all.sh` (`xovi-apply` makes them take effect) |
| systemd units of the shelf services and `shelf.target` | `/usr` | **wiped** | the `shelf` step (or alone: `cd packaging && SHELF_NO_BUILD=1 sh deploy.sh <device IP>`) |
| `xovi-reenable.service` (xovi boot persistence) | `/usr` | **wiped** | `xovi-persist` step |
| `chrony-boot-wakelock.service` | `/usr` | **wiped** | `chrony-boot-wakelock` step |
| `battop.service` | `/usr` (data in `/home`) | unit **wiped** | `battop` step (started after install, not enabled at boot) |
| `wifi-watch.service` | `/usr` (script `~/.local/bin/wifi-watch.sh` in `/home`) | unit **wiped** | `wifi-watch` step |
| Domestic NTP (chrony config), default timezone | `/etc` | **wiped** | `chrony-cn` / `timezone-cn` steps |
| appload ≥ 0.6.0 | `/home` (xovi) | depends on whether appload was reinstalled | separate: `vellum upgrade appload` on the device, then reboot |

**Risk layers** (don't collapse them into one percentage): the shelf layer only uses xochitl's `/upload` web endpoint and standard system components, so reinstalling restores it; qmldiff injections such as the font menu depend on xochitl's internal QML and often need re-adapting on a major version (3.27→3.28 already needed two qmd variants); KOReader itself is independent, but its sidebar entry relies on the third-party appload, so confirm appload supports each new major firmware (3.28 onward needs ≥ 0.6.0).

**After a "bare-metal restore", check extra**: an OTA itself never deletes `/home`, but if the device went through a more thorough reset, the payload under `/home` (the `.so` files in `extensions.d/`, the service binaries) can disappear with it — this really happened on 2026-09-09. Confirm those files are still there before re-running `install-all.sh`.

## Troubleshooting and risk items

These aren't "random low-probability glitches" but known issues with clear trigger conditions. Numbers ①–⑥ are referenced above.

| # | Symptom / scenario | Cause | What to do |
|---|---|---|---|
| ① | No KOReader/WeRead entry in the sidebar; `sidebar-entry` shows "skipped" in the summary; `journalctl` has a qmldiff "Couldn't resolve the hashed identifier" | appload ≤ 0.5.3 ships a built-in patch aimed at 3.27's old UI anchors, renamed in 3.28, so the launcher appload injects never gets built. It will **not** stop the install, stop xochitl from starting, or brick the device — just this one feature doesn't take effect (happened on this device on 2026-09-06 with v0.5.3) | Check the version with `vellum list --installed \| grep appload`; if old, `vellum upgrade appload`. Upstream **v0.6.0 (2026-09-19)** merged 3.28 support (plus 3.29); verified on the real device on 2026-09-21 (log shows "Loaded external AppLoad hooks in main UI", sidebar entries work). **⚠ After upgrading appload, do not run `systemctl restart xochitl`**: the running old process crashes on exit, triggering xochitl's `OnFailure=emergency.target` and rebooting the whole device once (seen 2026-09-21; no data damaged, but it interrupts use) — reboot the device instead (with `xovi-persist` installed xovi takes effect after boot, otherwise run `xovi/start` once by hand) |
| ② | The font menu, trash/new-folder proxy, comic-margin proxy and sidebar entry — unrelated-looking features — are all missing **at once** | They share one prerequisite: `qt-resource-rebuilder`. Without it, `install-all.sh` marks each as "skipped", not "failed" | `vellum add qt-resource-rebuilder`, then re-run `install-all.sh` |
| ③ | The whole device rebooted once after xochitl was restarted repeatedly in a short time | `xochitl.service` is configured with `Restart=on-failure` and `StartLimitBurst=4` (10-minute window) and counts every restart no matter who triggered it: this repository's deploy scripts run on their own (`hl-snap` / `handwriting-stroke` / `sidebar-entry` each restart xochitl once when run outside `install-all`), third-party installers like `vellum add appload`, WeRead on each launch/exit. **Verified on real hardware: just two quick restarts triggered one full device reboot (2026-09-11) — the device rebooted and came back fine, it was not a brick**, and it incidentally exercised the `xovi-persist` boot-recovery unit | `install-all.sh` already handles this for its own steps (stage everything, restart once at the end via `xovi-apply`). Only when **running deploy scripts by hand one at a time, or going back and forth between appload/WeRead**: leave a few minutes between each and wait until the previous restart settles into `is-active`=active |
| ④ | The firmware safety gate refuses to install | Not a bug, by design: a matching version string doesn't guarantee the internal layout hasn't shifted | Confirm the device's firmware is truly the one you verified, then `--force`; forcing on an unverified firmware risks a feature silently not working, or worse, affecting normal operation |
| ⑤ | The screen flashes once at the last step | `xovi-apply` restarts xochitl (compositor + UI process), after printing "about to interrupt reading" and waiting 5 seconds | Expected; don't use the device while installing. To avoid it: `--skip xovi-apply` and restart later yourself. **How to restart**: once xovi is active inside the running xochitl (`LD_PRELOAD` contains `xovi.so`), always `systemctl restart xochitl` and **never** run `xovi/start` by hand — it makes the running xochitl SEGV and the system reboots itself by design (2026-09-20 real-device incident; the new scripts have this check built in). Use `xovi/start` only when xovi is not active (fresh boot, right after an OTA) |
| ⑥ | battop isn't running after a reboot | **Deliberately not enabled at boot**: on 2026-08-29 its sampling triggered a kernel cgroup/RCU deadlock that froze the whole device and the root cause was never fully ruled out, so the installer only `start`s it, never `enable`s it | Turn on the battery-detective switch under Manage → System Enhancements in the web UI (the "Battery Detective" data page appears once it runs), or `systemctl start battop`. Restoring boot autostart is a decision you evaluate yourself |

### More troubleshooting

- Start with the summary `install-all.sh` prints at the end to see which step failed; the header comment of the matching `packaging/deploy-*.sh` explains what it does and its common failure causes.
- The closing section of `packaging/README.md` ("验证现状", verification status, Chinese) records every real-hardware issue found so far and how it was fixed.
- To check the scripts **without touching a real device**: `bash packaging/tests/run_sim_tests.sh` (runs the real code against fake ssh/systemctl/mount in a temp directory; refuses to run as root; also invoked by the CI pytest). This is local simulation and **is no substitute for real-hardware verification**.

### Backups and idempotency (one-paragraph version)

Every install script is idempotent and backs up what it overwrites to `/home/root/cangjie-backups/` (**never** inside `extensions.d/` — xovi loads any file there as an extension and registering one twice is fatal); only the latest 5 are kept; unchanged content is left untouched; before writing `/usr` the scripts check dm-verity and skip if it is active (writing `/usr` once triggered an A/B rollback brick, 2026-08-16). Mechanics in [`packaging/README.md`](../packaging/README.md#备份与幂等) (Chinese).

## What this installer deliberately does not do

- **Doesn't install vellum / xovi / qt-resource-rebuilder / appload themselves, doesn't sideload KOReader**: see "Before you install"; these remain manual prerequisites.
- **Doesn't install the Chinese input method**: that line was archived out of this repository (see [README](README.en.md#history-and-scope)) and isn't distributed by this installer; the parts already deployed on the device keep running.
- **Doesn't upgrade appload**: 3.28 firmware needs appload ≥ 0.6.0; an older install must be upgraded by hand and the device rebooted (see issue ①).
