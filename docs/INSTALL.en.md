# Installation Guide

**[中文](INSTALL.md)** · back to [README](README.en.md)

> **Who this is for**: anyone installing this suite on a reMarkable Paper Pro Move for the first time, updating or uninstalling it, or restoring it after a firmware update (OTA).
> - **First install**: read "Scope → Before you install → Install → After installing" in order.
> - **Installed before, updating now**: just re-run `install-all.sh` (see "Install"); removed components are cleaned up automatically, see "Upgrading".
> - **Uninstall**: read "Uninstall". **Updated the firmware**: read "After a firmware update (OTA)". **Problems**: see "Troubleshooting".
>
> To learn what the whole thing is, read [`OVERVIEW.md`](OVERVIEW.md) (Chinese). How the scripts are written, every parameter and environment variable, what each check means, and how to test them locally are in [`../packaging/README.md`](../packaging/README.md) (Chinese, maintainer-oriented; not repeated here).

## In one sentence

You run **one command** on your computer; the scripts build the programs, install them on the device over ssh, reboot the device once so the changes take effect, and check the result automatically when it comes back. xochitl itself is never modified, and almost everything lives under the device's `/home` (which survives firmware updates).

**Terms used throughout**:

| Term | Meaning |
|---|---|
| xochitl | The device's stock reading/notes app. This project doesn't modify it; it only adds things alongside |
| vellum | A third-party package manager on the device, used to install community components such as xovi |
| xovi / extension | A third-party extension loader: when xochitl starts, it also loads the `.so` plugins in `extensions.d/`. This project has two extensions: `hl-snap` (highlighter snapping) and `ui-font` (UI font) |
| qmd / UI patch | A patch to xochitl's UI description files (QML), applied by qt-resource-rebuilder when xochitl starts |
| files only / take effect | Extensions and UI patches are only read when xochitl starts, so once the files are in place the **whole device has to reboot once** for them to take effect |
| OTA | Over-the-air firmware update. It replaces the system partitions `/usr` and `/etc` wholesale and leaves `/home` (your data) alone |
| dm-verity | Read-only verification of the system partition. While it is on, the scripts never write `/usr` |

![Install and uninstall flow (labels in Chinese)](diagrams/install-flow.svg)

## Scope

**reMarkable Paper Pro Move (imx93-chiappa), firmware 3.28.0.172**. It is the only version verified on real hardware so far, and the installer checks it before doing anything (see "Automatic checks"). Other firmware versions and other reMarkable models are unverified; forcing an install there may misplace the UI patches — best case a feature doesn't work, worst case it affects normal device use.

## Before you install

### On the device: install 2 things by hand first

They belong to the third-party reMarkable ecosystem, not to this repository, and `install-all.sh` will **not** install them. This assumes you can already ssh into the device as root and have vellum installed — for how to enable that (reMarkable requires developer mode first) and how to install vellum, follow reMarkable's official instructions and vellum's own documentation.

| # | Run on the device | What it is | If missing |
|---|---|---|---|
| 1 | `vellum add xovi` | [xovi](https://github.com/asivery/xovi): the extension loader | The plugin-related steps (`xovi-persist`, `hl-snap`, `ui-font`, `xovi-apply`) fail outright |
| 2 | `vellum add qt-resource-rebuilder` | Loader for UI patches (qmd) | No UI patch is installed (not a failure): the font menu, UI font tokens, trash/new-folder proxies, comic-margin proxy, reading-position proxy, and reader tap-to-turn. Everything else is unaffected (see Troubleshooting 1) |

After installing these two, reboot the whole device (`reboot`) so they take effect; don't `systemctl restart xochitl` (see "Why always a full reboot").

Since 2026-09-29 **appload and KOReader are no longer needed** (the device only uses its built-in reader). If the device ever had them, or had the battery sampler or handwriting stroke tuning, see "[Upgrading](#upgrading-cleaning-up-removed-components)".

### On your computer: build tools and ssh

The scripts build the programs on your computer and install them on the device over ssh. They are POSIX sh, developed and verified on Linux; other systems are untested.

| What | Used for | If missing |
|---|---|---|
| Rust (`cargo`) + `rustup target add aarch64-unknown-linux-musl` + `aarch64-linux-gnu-gcc` | Cross-compiling the 8 web services (`shelf/build.sh`, which first runs the tests on your computer); building the `hl-snap` / `ui-font` plugins | Services can't be built: the `shelf` step fails. Plugins can't be built: the prebuilt `.so` committed to the repository is used, with a notice |
| **Passwordless ssh login to the device as root** | Every step (the scripts never stop to ask for a password) | Fails before touching anything, with troubleshooting steps. If you haven't set it up, run `ssh-copy-id root@10.11.99.1` first |
| git | Getting the code | — |

You don't need the xovi source: the plugins' xovi glue code is committed. Only if you change a plugin's `.xovi` description do you need a clone of [asivery/xovi](https://github.com/asivery/xovi), pointed to by `XOVI_DIR`, to regenerate it.

### Connecting to the device

| How | Device address | Notes |
|---|---|---|
| USB cable (recommended, use it for the first install) | `10.11.99.1` (the scripts' default) | A USB network interface appears on your computer. If it's there but you can't connect or the IP is wrong: `sudo ip addr add 10.11.99.2/24 dev <interface>` |
| Same WiFi | The device's IP, or `shelf.local` (only after the shelf has been installed once) | The device must allow ssh over WiFi (the exact switch is not verified in this document). Android doesn't resolve `.local`; use the IP |

Keep the device awake and the cable plugged in during the install. The scripts hold a wake lock on the device that releases itself after 20 minutes, so it doesn't fall asleep between steps; if the connection drops anyway, nothing is left half-done (see Troubleshooting 7).

## Install

### One command

1. **Check the firmware version** in the device settings; only 3.28.0.172 has been verified.
2. **Get the code**:
   ```sh
   git clone https://github.com/bbq191/cang-jie.git
   cd cang-jie
   ```
3. **(Recommended) Build once on its own**: the first build takes a long time, and doing it first surfaces build problems before the device is touched. The `shelf` step calls it again during the install; what's already built isn't rebuilt.
   ```sh
   sh shelf/build.sh
   ```
4. **Dry run** (on your computer only, no device contact):
   ```sh
   sh packaging/install-all.sh --dry-run      # only prints which steps would run; add --skip to see the effect
   ```
   A dry run does **not** check the firmware or the device; it only proves the command line is right.
5. **Install for real** (USB connected):
   ```sh
   sh packaging/install-all.sh 10.11.99.1
   ```
   The script first checks ssh, the device state and the firmware (next section), then runs the steps, reboots once at the end and checks the result automatically when the device comes back.
6. **Read the summary** (see "Reading the summary"), then **log in to the web page, change the password, install the certificate** (see "After installing").

### Automatic checks before touching anything

`install-all.sh` (except with `--dry-run`) first does the three things below; if any of them fails, **no step runs** and nothing on the device has changed:

1. **Can ssh connect**: if not, it reports the error with troubleshooting steps (device asleep or USB unplugged → wrong IP → device host key changed → no passwordless login).
2. **Device pre-check** (read-only): must be root and `/home` must be writable; less than **50MB free on `/home` refuses, less than 200MB warns**; it also reports whether xovi and qt-resource-rebuilder are installed, whether dm-verity is on, and whether xovi is already active inside xochitl. Missing pieces are only reported early; the affected step fails or skips on its own.
3. **Firmware safety gate**: reads the sha256 of `/usr/bin/xochitl` on the device and compares it with `packaging/firmware-allowlist.txt` in the repository and `firmware-allowlist.local.txt` on your computer; it continues only on a match and refuses otherwise. A hash is used instead of the version number because UI patches locate their targets byte by byte, and even a hotfix with the same version number can shift internal layout. If you're sure the device runs the firmware you mean to install on and the hash just isn't listed, add `--force`: the current hash is appended to `firmware-allowlist.local.txt` **on your computer** (not tracked by git), so the same firmware won't need it again.

### What each step installs

The **step names** can be used with `--skip`; the matching script `packaging/deploy-<step name>.sh <device>` (`deploy.sh` for the `shelf` step) can also be run on its own.

| Step | What it does | Needs |
|---|---|---|
| `chrony-cn` | Switches the time servers to ones reachable from mainland China (Alibaba Cloud, Tencent Cloud, etc.). Writing the config counts as success; if the device is offline at the moment it only prints a notice and syncs once online | — |
| `chrony-boot-wakelock` | Keeps the device from auto-suspending for a short while after boot (released as soon as time is synced, at most 120 seconds), so the first time sync isn't interrupted | — |
| `timezone-cn` | Sets the default timezone to Asia/Shanghai | — |
| `wifi-watch` | WiFi watchdog: reconnects when the link silently dies; locks to 2.4 GHz only when the AP is on a 5 GHz channel the device isn't allowed to use (5150–5350 MHz); turns on WiFi power saving (since 09-28; one side-by-side measurement showed idle current about 37% lower). Configurable on the device in `~/.config/wifi-watch.conf` (`BAND=`, `POWERSAVE=`) | — |
| `xovi-persist` | Re-activates xovi automatically after boot, so a device reboot needs no manual fix-up | xovi |
| `hl-snap` | Makes the highlighter snap to exactly what you drew over Chinese text, instead of "a short stroke grabs the whole line"; files only | xovi |
| `ui-font` | UI font plugin: xochitl's interface (library, settings, dialogs, titles) uses the font you pick on the web page; the reader font is unchanged (since 2026-10-07); files only | xovi |
| `shelf` | 8 web services: the gateway, books (book), fonts and wallpapers (font / wallpaper), and the four notes services (ink / transcribe / mind / note); plus 7 UI patches (font menu, UI font tokens, trash proxy, new-folder proxy, comic-margin proxy, reading-position proxy, reader tap-to-turn), files only | The patches need qt-resource-rebuilder; without it only the patches are skipped and the services still install |
| `清理已移除:battop`, `清理已移除:handwriting-stroke` ("clean up removed: …") | Not install steps: remove leftovers of the battery sampler and handwriting stroke tuning from older devices; nothing happens if there are none (see "[Upgrading](#upgrading-cleaning-up-removed-components)"). `--skip battop` / `--skip handwriting-stroke` skips them | — |
| `xovi-apply` | Once everything "files only" is in place, **reboots the whole device once, only if something changed (or xovi isn't active yet)** (about 20–60 seconds, interrupts reading; no change, no reboot). After the reboot it runs `verify-on-device.sh` automatically. **Not run if an earlier step failed** | — |

Pushed files are checked by md5: a file that's already identical on the device isn't sent again (the shelf package too); a corrupted transfer deletes the staged copy and installs nothing, leaving what was on the device untouched.

### Reading the summary

The final summary has five columns (the labels are in Chinese):

| Column | Meaning | What to do |
|---|---|---|
| 已安装 (installed) | The step succeeded | — |
| 已跳过（--skip） (skipped by --skip) | You skipped it on the command line | — |
| 已跳过（前置条件不满足，非失败） (skipped, prerequisite not met — not a failure) | E.g. dm-verity is on so `/usr` couldn't be written; or an earlier step failed so there was no reboot | Read the reason printed after it. "Skipped" is not "failed" and is easy to overlook (see Troubleshooting 1) |
| 失败 (failed) | The step reported an error | Scroll up to that step's original error. There is no automatic retry |
| 未执行（设备连不上） (not run — device unreachable) | Steps not run after the connection dropped | Re-run the same command once the device is back |

When something failed, whatever was already installed stays fine; fix the problem and **re-run the whole command**. Every script can be re-run; unchanged content isn't rewritten and doesn't trigger another reboot.

## After installing

**Check the verification result first**: after the final reboot the script waits for the device to come back and runs `verify-on-device.sh` automatically (`CJ_APPLY_VERIFY=0` turns this off). It is a read-only check in 9 sections (firmware and boot, xochitl and extensions, UI patches, resident services, this boot's alerts, flight recorder, ports, `/usr` units, disk), reported item by item as ✓/⚠/✗; it exits non-zero if anything is ✗. A full install has about 40 items: on real hardware on 2026-10-09 it was 39✓ 1⚠ 0✗, the ⚠ being "booted less than 10 minutes ago", which is normal right after a reboot. If no reboot happened, or after deploying a single step later, run it yourself:

```sh
sh packaging/verify-on-device.sh 10.11.99.1
```

If removed components are still on the device it flags them with ⚠ and prints the cleanup command. What each item means: [`packaging/README.md`](../packaging/README.md#部署后核对verify-on-devicesh2026-09-25) (Chinese).

**Then open the web page**: `https://10.11.99.1/` in a browser (on the same WiFi, `https://shelf.local/` also works; Android doesn't resolve `.local`, use the device IP).

- **Change the password**: the default is `shelf`; the first login forces you to the change-password page.
- **Login rate limit**: 5 wrong passwords within 60 seconds from one IP lock that IP out temporarily; another device (different IP) is unaffected.
- **Install the certificate**: the browser warns that the certificate isn't trusted because the device signs it itself. The login page has a "download CA certificate" link (`https://<device>/ca.crt`); install it into your phone's or computer's trust store once and the warning goes away. On iOS you also have to turn on full trust under "Settings → General → About → Certificate Trust Settings". For one-off use you can click "Advanced → Proceed".
- **Upgrading from a version before 2026-09-24**: on first start the gateway replaces its CA automatically (the new CA can only sign certificates for LAN names and private IPs; the old files are renamed `*.bak-<time>` and kept on the device). **Reinstall the new certificate on every phone and computer and delete the old one** — the old CA has no such restriction, so keeping it alongside the new one keeps the risk. Once the new certificate works, you can delete the `*.bak-*` files under `~/.config/shelf/tls/` on the device.
  - Known limitation: addresses outside that range (e.g. a carrier-assigned 100.64.x.x, a public IP, or an mDNS name you changed yourself) won't match the certificate and the browser will complain.

## Re-running: when does it reboot

Re-running `install-all.sh` is safe and doesn't flash the screen every time. Only when new content was actually written does a script leave a "pending" marker on the device (in the RAM disk `/run/cangjie-pending-apply/`, cleared on reboot); the last step decides from the markers whether to reboot.

| Situation | What the last step `xovi-apply` does |
|---|---|
| Something changed this run (first install, updated plugin or UI patch) | **Reboots the whole device once** (prints "will interrupt reading", waits 5 seconds; back in about 20–60 seconds, then checks automatically) |
| A plugin `.so` was updated while xochitl is using the old one | The new one goes into the staging area first and is swapped in before the reboot (once the swap has started, it completes even if the computer disconnects or you press Ctrl-C) |
| Nothing changed and xovi is active | No reboot |
| Nothing changed, but the device has just rebooted and xovi isn't active yet | With xovi persistence installed: full reboot (restored at boot); without it: runs `xovi/start` |
| A new plugin is staged and you rebooted the device yourself first | At boot `xovi-reenable` swaps it in; nothing else to run |
| An earlier step failed | **No reboot**. Changes already on disk stay pending and take effect when you fix the problem and re-run |
| `--force-apply` was given | Reboots once no matter what |
| You skipped it with `--skip xovi-apply` | The markers remain; run `sh packaging/deploy-xovi-apply.sh <device>` when convenient |

![Making plugins / UI patches take effect: swap in, then reboot (labels in Chinese)](diagrams/so-swap-order.svg)

**Running a single step behaves the same**: `deploy-hl-snap.sh` / `deploy-ui-font.sh` run on their own reboot once if something changed; if the file is byte-identical to what's installed, nothing else is pending and xovi is active, they **don't reboot**.

### Why always a full reboot

Since 2026-09-25, making changes take effect no longer restarts xochitl alone; it reboots the device: **stopping xochitl by itself has a real chance of crashing on its way out**, and when it does the system goes through its emergency path and reboots anyway (seen several times on real hardware; unrelated to this project's plugins — it's a destructor-order problem in xochitl's exit). Rather than risk that, the scripts reboot cleanly. The cost is 20–60 seconds each time. So when you make changes take effect by hand, just `reboot`; **don't** `systemctl restart xochitl`, and **never** run `xovi/start` by hand while xovi is already active (it crashed xochitl on real hardware on 2026-09-20).

## Common options

### Command reference

Run from the repository root; `<device>` defaults to `10.11.99.1`. Every script supports `-h`; a wrong parameter always exits with code 2 without contacting the device. All parameters and environment variables are in [`packaging/README.md`](../packaging/README.md#参数与环境变量) (Chinese).

| Command | Parameters | Effect |
|---|---|---|
| `sh packaging/install-all.sh [device]` | `--dry-run` | Print the plan only; no device contact |
| | `--skip a,b` | Skip the named steps (an unknown name only warns, and lists the known step names) |
| | `--force` | Install even if the firmware isn't allowlisted (see "Automatic checks") |
| | `--force-apply` | Make the last step reboot once whether or not anything changed |
| `sh packaging/uninstall-all.sh [device]` | `--dry-run` / `--skip a,b` / `--purge` | See "Uninstall" |
| `sh packaging/deploy.sh [device]` | `--only a,b` · `--password NEW` · `--no-systemd` | Install or update only the web services (see below) |
| `sh packaging/deploy-xovi-apply.sh [device]` | `--force` | Make the "files only" content take effect on its own |
| Other `sh packaging/deploy-<step name>.sh [device]` | `-h` | Run one step on its own |
| `sh packaging/verify-on-device.sh [device]` | `--json` · `--dump` · `--from FILE` | Read-only check of the device's current state |

### Installing only part of it

```sh
sh packaging/install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist    # skip the named steps
sh packaging/deploy.sh 10.11.99.1 --only book,font --password 'new password'          # only the book and font services, and set the gateway password
```

Names accepted by `--only`: `gateway book font wallpaper ink transcribe mind note`. The gateway is always installed; any other name is an error. `--password` travels over ssh standard input into a temporary file on the device and is deleted after use, so it never shows up on a command line or in a process list. Before pushing, `deploy.sh` checks that the programs to install have been built and stops with a pointer to `sh shelf/build.sh` if not; with `SHELF_NO_BUILD=1` it doesn't build and uses the existing outputs. With `--only`, the on-device uninstaller `shelf-uninstall` and its manifest are refreshed too.

## Uninstall

`uninstall-all.sh` uses the same step table as the installer, in **reverse order** (last installed, first removed), and finally cleans up leftovers of removed components. Do a dry run first:

```sh
sh packaging/uninstall-all.sh 10.11.99.1 --dry-run          # print the plan only; no device contact, nothing deleted
sh packaging/uninstall-all.sh 10.11.99.1                    # remove everything
sh packaging/uninstall-all.sh 10.11.99.1 --skip shelf       # skip a step
```

**What it does**: stops and deletes the installed services, plugins and UI patches, plus the package directories pushed to the device during deployment (only known files are deleted; a directory with anything else in it is kept). Removing a plugin also withdraws any newer version still waiting in the staging area.

**Kept by default**: the staging library, configuration, certificates, font/wallpaper pools, and backups in `cangjie-backups/`. `--purge` currently affects no step and doesn't touch the shelf data; to delete the shelf data too, uninstall the rest with `--skip shelf`, then run `shelf-uninstall --purge` on the device.

**What it doesn't do**:

- `chrony-cn` and `timezone-cn` change configuration and `xovi-apply` is just an action, so none of them is uninstalled. The backups from before the change are in `cangjie-backups/` on the device; restore them yourself if needed.
- vellum, xovi and qt-resource-rebuilder weren't installed by this project and aren't removed.
- Uninstalling **doesn't reboot** the device. Plugins and UI patches already loaded stay active until the next reboot. To stop them right away: `reboot` on the device (not `systemctl restart xochitl`; see "Why always a full reboot").

**When dm-verity is on**: the service units under `/usr` can't be deleted (the scripts never write `/usr` under verity). The uninstaller says so and **keeps** the programs those units need, so they don't fail over and over after a reboot. Run `uninstall-all.sh` again once the device is writable to finish.

If the connection drops midway, the remaining steps are listed as "not run"; re-run the same command once the device is back.

## Upgrading: cleaning up removed components

Some features were cut later. Their source code and install steps are gone, but devices that had them installed still carry files.

| Component | Removed | What may remain on the device | How to clean up |
|---|---|---|---|
| Battery sampler (`battop`, power diagnostics service) | 2026-09-30 | `battop.service` under `/usr` (older versions also `battop.timer`), and the whole `/home/root/battop/` directory | **Re-running `install-all.sh` cleans it automatically** |
| Handwriting stroke tuning (`handwriting-stroke`, plugin `hw-stroke.so`) | 2026-09-30 | `extensions.d/hw-stroke.so`, a copy in the staging area, the package directory `/home/root/hw-stroke/` | **Re-running `install-all.sh` cleans it automatically** |
| The shelf's `koreader-serve` service | 2026-09-29 | Service unit and program | Cleaned along the way by re-running `install-all.sh` (or `deploy.sh`) |
| Sidebar KOReader entry (`sidebar-entry`) | 2026-09-29 | UI patch `koreader-sidebar-entry.qmd` and icon pack `cangjie-icons.rcc` | **Not cleaned automatically**; run `uninstall-all.sh` once by hand (commands below) |

![How install-all cleans up removed components (labels in Chinese)](diagrams/retired-cleanup.svg)

**How the automatic cleanup works**: before the last step `xovi-apply`, `install-all.sh` runs one cleanup each for the battery sampler and handwriting stroke tuning (the same functions `uninstall-all.sh` uses). If nothing is left on the device, nothing happens; if xochitl still had `hw-stroke.so` loaded, a "pending" marker is recorded and the last step therefore **reboots once** to fully unload it. The old `hwStroke*` settings in `reading-qol.json` are left as they are; nothing reads them any more, so they're harmless.

The sidebar entry isn't cleaned automatically because its icon pack file name `cangjie-icons.rcc` was used by other UI patches in the past; it's only deleted when you explicitly ask.

**Clean up only some of them, without reinstalling**: run `uninstall-all.sh` and skip every other step. Dry-run first; the plan should contain only the steps you want:

```sh
# only the battery sampler and handwriting stroke tuning (plan should list just handwriting-stroke and battop)
sh packaging/uninstall-all.sh 10.11.99.1 --dry-run --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,sidebar-entry
sh packaging/uninstall-all.sh 10.11.99.1 --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,sidebar-entry
# only the sidebar entry
sh packaging/uninstall-all.sh 10.11.99.1 --skip chrony-boot-wakelock,wifi-watch,xovi-persist,hl-snap,ui-font,shelf,battop,handwriting-stroke
```

Afterwards reboot the device once (`reboot`). `verify-on-device.sh` flags these leftovers with ⚠ (✗ if the sidebar patch remains while appload is gone), and the cleanup commands it prints are the ones above.

**On real hardware**: on 2026-09-30 15:23, `install-all.sh` on a real device automatically removed the battery sampler (units and `/home/root/battop`) and `hw-stroke.so`, rebooted only once, and then checked out at 36✓ 1⚠ (just booted) 0✗. The "clean up only some" `uninstall-all.sh` commands above have only been tested in local simulation.

## After a firmware update (OTA)

This is the **authoritative description** of OTA recovery; other documents link here.

![After an OTA: what is lost and how to recover (labels in Chinese)](diagrams/ota-recovery.svg)

**The update itself doesn't lose data under `/home`, but you have to re-run the install afterwards to get the features back.** This project deliberately leaves nothing on the boot path (xovi's loading config lives in the RAM layer of `/etc`, the service units in `/usr`), so new firmware always boots in a pure factory state.

### Recommended procedure

1. (Before updating, optional) move any xovi plugins that are incompatible with the new firmware (e.g. an old version of some third-party plugin) out of `extensions.d/` into `/home/root/xovi-disabled/`. **Never leave them inside `extensions.d/`**: xovi loads every file in that directory as a plugin.
2. After the update, **at the device**, run `xovi/rebuild_hashtable` by hand (it asks for the root password; the scripts don't do this). It rebuilds an index of UI resources for the new firmware, which the UI patches use to find their targets, so the patches depend on it.
3. On your computer: `sh packaging/install-all.sh <device>`. The new firmware's hash usually isn't allowlisted; once you've confirmed the version, add `--force`. After an OTA xovi isn't active, so the last step reboots once; at boot the freshly reinstalled `xovi-reenable` restores xovi, and the result is checked automatically when the device is back.
4. Read the summary and open the gateway in a browser to confirm.

### Item by item

| What | Where | After an OTA | How to restore |
|---|---|---|---|
| Staging library, font and wallpaper pools, certificates, gateway password, sleep-screen setting, `cangjie-backups/` | `/home` | Kept | Nothing to do |
| The web services' programs (`~/.local/bin`) | `/home` | Kept | Nothing to do |
| The `hl-snap` and `ui-font` plugins, the UI patches | `/home` (`extensions.d/`, `exthome/`) | Files remain, but need a rebuilt hashtable to take effect | Step 2, then run `install-all.sh` |
| Service units for the web services and `shelf.target` | `/usr` | **Wiped** | The `shelf` step (or on its own: `SHELF_NO_BUILD=1 sh packaging/deploy.sh <device>`) |
| `xovi-reenable.service` (re-activates xovi at boot) | `/usr` | **Wiped** | The `xovi-persist` step |
| `chrony-boot-wakelock.service` | `/usr` | **Wiped** | The `chrony-boot-wakelock` step |
| `wifi-watch.service` (the script lives in `/home`) | `/usr` | Unit **wiped** | The `wifi-watch` step |
| Mainland-China time servers, default timezone | `/etc` | **Wiped** | The `chrony-cn` / `timezone-cn` steps |

**Risk layers**: the shelf layer only uses xochitl's web upload interface and standard system components, so reinstalling after a firmware change brings it back; UI patches such as the font menu depend on xochitl's internal QML and often need re-adapting after a major version upgrade.

**A "bare-metal restore" needs one more check**: an OTA itself doesn't delete `/home`, but after a more thorough reset the plugins and programs under `/home` may be gone too (hit on real hardware on 2026-09-09). Confirm they're still there before re-running `install-all.sh`.

## Troubleshooting

All of these are known issues with a clear trigger, not random failures.

| # | Symptom | Cause | What to do |
|---|---|---|---|
| 1 | The font menu, trash/new folder, comic margins, reading-position recovery and reader tap-to-turn are **all** missing (and the UI font only partly applies) | They share the prerequisite qt-resource-rebuilder. Without it they're patches bundled in the `shelf` step and **not listed separately** — `shelf` still counts as "installed"; only that step's output has a line saying it skipped the qmd files because there's no qt-resource-rebuilder directory | `vellum add qt-resource-rebuilder`, then re-run `install-all.sh` |
| 2 | After xochitl was stopped and started several times in a short while, the device rebooted once | The xochitl service allows at most 4 restarts within 10 minutes, whoever triggers them: a manual `systemctl restart xochitl`, or `vellum add/del` of xochitl plugins. On 2026-09-11 two restarts in a row triggered a full reboot on real hardware — **the device recovers by itself; it's not bricked** | Since 2026-09-25 the deployment scripts reboot the device instead, which doesn't count toward this limit. When adding/removing plugins by hand, leave a few minutes between them |
| 3 | Exits with an error before installing: `连不上 root@…` (can't connect) / `只剩 N MB 可用` (only N MB free) / `需要 root` (needs root) / `固件不在白名单` (firmware not allowlisted) | The automatic checks are blocking it; nothing on the device has changed | Can't connect: follow the steps in the error (asleep/no USB → IP → host key → passwordless login); low space: clean up `/home/root` and `cangjie-backups/` and retry; firmware: first confirm the device runs the verified firmware, then `--force` |
| 4 | The device rebooted once at the end of the install | `xovi-apply` makes the changes take effect: always a **full reboot** (back in about 20–60 seconds), and only when something actually changed or xovi wasn't active yet | Normal; don't use the device during the install. The script waits for it and checks automatically. To avoid the interruption, use `--skip xovi-apply` and later run `sh packaging/deploy-xovi-apply.sh <device>` |
| 5 | The last step reports that the device couldn't schedule the reboot and the changes haven't taken effect; the step is marked failed | The `systemctl reboot` command on the device itself failed. The files are already swapped in, but xochitl is still using the old ones; the script has put the "pending" marker back. This branch has only been tested in local simulation | `reboot` on the device by hand, then run `sh packaging/verify-on-device.sh <device>`; or re-run `sh packaging/deploy-xovi-apply.sh <device>` later |
| 6 | The last step reports that xochitl has no xovi and there's no `xovi/start` on the device; the step is marked failed | xovi isn't installed (or was removed by vellum): a reboot couldn't make plugins and UI patches take effect, so the script reports an error and doesn't reboot (since 2026-10-09). Only tested in local simulation | `vellum add xovi` on the device, then re-run `install-all.sh` |
| 7 | An ssh disconnect midway (e.g. `Timeout, server … not responding`), with "not run" entries in the summary | The device went to sleep, the cable was pulled or WiFi dropped. ssh detects a dead link in about 15 seconds; the script probes once more and, if it still can't connect, runs no further steps. Scripts sent to the device run only when received in full, so the interrupted step executed nothing | Wake the device, confirm the connection, and re-run `install-all.sh` (what's already done isn't redone) |
| 8 | In the summary, `xovi-apply` is in the "prerequisite not met" column, saying an earlier step failed | An earlier step failed, so this run doesn't reboot (since 2026-10-10). Changes already on disk stay pending | Fix the failing step using its error, then re-run `install-all.sh`, which applies them along the way; or run `sh packaging/deploy-xovi-apply.sh <device>` now |

### Other troubleshooting

- Start with the summary to find the failing step; the header comment of the matching `packaging/deploy-*.sh` explains what the step does and common causes of failure.
- The web page's "Manage" view shows whether a plugin is really loaded into xochitl ("loaded / not loaded"). A switch that's on but shows "not loaded" means the plugin isn't installed or the device hasn't rebooted since. "Manage → Device health" shows a fuller picture (services, extensions, last boot log).
- To check the scripts aren't broken without touching a device: `bash packaging/tests/run_sim_tests.sh` (local simulation, 448 assertions, all passing on 2026-10-10). It doesn't replace verification on real hardware.

## Risks and known limitations

- **Writing `/usr` is an exception, not "never touching `/usr`"**: a few boot-time service units (the shelf services, `xovi-reenable`, `chrony-boot-wakelock`, `wifi-watch`) have to live in `/usr`. Before writing, the scripts check dm-verity (and don't write if it's on), then write inside a short read-write window and restore read-only whatever happens. Writing `/usr` once caused a rollback that bricked the device (2026-08-16), which is why everything else lives under `/home`.
- **Every activation is a full reboot**, an interruption of about 20–60 seconds (see "Why always a full reboot").
- **Backups**: before overwriting an existing file on the device it's backed up to `/home/root/cangjie-backups/` (**never** into `extensions.d/`), keeping the latest 5; unchanged content is neither backed up nor touched. Details in [`packaging/README.md`](../packaging/README.md#备份与幂等) (Chinese).
- **No automatic rollback**: when a step fails, things stop in a state that can be re-run; fix and re-run. A rollback would itself have to touch `/usr` and `extensions.d` again, which is riskier than the failure.
- **Running `shelf/install.sh --password <plaintext>` directly on the device briefly shows the password in the device's process list**; passing it through `deploy.sh --password` on your computer doesn't.
- Uninstalling doesn't revert `chrony-cn` / `timezone-cn`; there's no one-click "back to before install".

## Verification status (summary)

The full record is in [`packaging/README.md` "验证现状"](../packaging/README.md#验证现状如实说明不夸大) (Chinese). When trying things on a device, go one step at a time: `--dry-run` first, then single steps or `--skip`.

| Scope | Status |
|---|---|
| Full `install-all.sh` run | **Run on real hardware**: 2026-09-22, 09-24, 09-25, 09-30, 10-09 (twice, once over WiFi and once over USB); 10-09 checks were 38✓ 1⚠ and 39✓ 1⚠ (⚠ = just booted) 0✗ |
| Full `uninstall-all.sh` run | **Run on real hardware**: 2026-09-25; all 8 steps succeeded in reverse order, shelf/notes data and configuration kept, xochitl wasn't restarted during the uninstall |
| "Swap in a new `.so` → full reboot → auto-restore at boot → check" | **Run on real hardware**: 2026-09-25 (43✓) |
| Automatic cleanup of removed components | **Run on real hardware**: 2026-09-30 15:23, a single reboot, 36✓ 1⚠ 0✗ |
| The two 2026-10-10 hardening rounds (stop on disconnect, no reboot after a failure, no re-sending an unchanged package, merged round trips, on-device convergence after interruption, etc.) | **Local simulation only**, not yet on real hardware |
| Disconnect detection, not executing a half-received script, error without reboot when xovi is missing, handling a failed `systemctl reboot`, keeping programs under dm-verity, `--purge`, `uninstall-all` cleaning only removed components | **Local simulation only** |

Note: these only show that things "got installed and the services are healthy"; whether each shelf and notes feature was tested by hand is recorded in their own documents.

## What this installer doesn't do

- **Doesn't install vellum / xovi / qt-resource-rebuilder**: see "Before you install".
- **Doesn't install the Chinese input method**: that feature line's source has moved out of this repository (see [README](README.en.md#history-and-scope)) and isn't shipped by this installer.
