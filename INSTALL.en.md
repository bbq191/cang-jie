# Installation Guide

**[中文](INSTALL.md)**

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

## Install everything with one command

Connect your computer to the device over USB (reMarkable configures itself as `10.11.99.1` on
that link by default):

```sh
git clone git@github.com:bbq191/cang-jie.git
cd cang-jie/packaging
sh install-all.sh 10.11.99.1
```

This runs the following steps in order (each can also be run on its own — see "Installing only
part of it" below):

| Step | What it does | Prerequisite |
|---|---|---|
| Firmware safety gate | Checks the device's firmware against the verified version | — |
| Domestic NTP | Swaps chrony's servers for reachable ones (Aliyun/Tencent Cloud, etc.) | — |
| Default timezone | Sets Asia/Shanghai | — |
| Battery diagnostics | A resident sampling service, viewable under the web UI's Manage → Battery Detective | — |
| xovi boot-persistence | Installs a unit that re-runs `xovi/start` automatically on every boot, so you no longer have to do it by hand after a reboot | requires `vellum add xovi` |
| Precise highlight snapping | Precise CJK highlight-snapping (snaps exactly what you drag, not "drag a bit, snap the whole line") | same |
| Handwriting stroke rendering | Tunes stroke thickness by pen angle/speed | same |
| Shelf + gateway + notes + fonts/wallpaper | Nine web services: book management, note ingestion/transcription, upload-and-use fonts/wallpapers | — |
| Apply | Once the xovi extensions above are staged, runs `xovi/start` exactly once at the end to make them take effect | — |

When it finishes, it prints a summary: what got installed, which step (if any) failed, and what
still needs to be done by hand (vellum bootstrap, KOReader sideloading, etc.). No step is
retried automatically or silently skipped on failure — just follow the error message. Every
script is idempotent, so re-running the whole command is always safe.

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
`--force` (it appends the current hash to the allowlist automatically).

```sh
sh install-all.sh 10.11.99.1 --force
```

## Installing only part of it / skipping steps

```sh
sh install-all.sh 10.11.99.1 --skip chrony-cn,timezone-cn,xovi-persist
```

Skippable step names: `chrony-cn`, `timezone-cn`, `battop`, `xovi-persist`, `hl-snap`,
`handwriting-stroke`, `shelf`, `xovi-apply`. Each step's underlying script
(`packaging/deploy-<step>.sh <host>`) can also be run on its own, independent of
`install-all.sh`.

## What this installer deliberately does not do

- **Doesn't install vellum/xovi/qt-resource-rebuilder/appload themselves, doesn't sideload
  KOReader** — see "Before you install" above; these remain manual prerequisites.
- **Doesn't install the Chinese input method** — that line was archived out of this repository
  (see the top-level [README](README.en.md), "History and scope") and isn't distributed by this
  installer.
- **Doesn't install wifi-watch** (automatic WiFi carrier-loss reconnection).
- **No symmetric one-command uninstall** — `shelf/uninstall.sh` can remove the shelf portion;
  everything else is removed by hand with `systemctl disable --now <unit>`.

For the full architectural decisions, every real-hardware pitfall found along the way, and the
current verification status, see [`packaging/README.md`](packaging/README.md) — that document is
aimed at engineering detail; this one is meant as a quick-start guide for a first install.

## After a firmware update (OTA)

An OTA only wipes the device's system partitions (`/usr` + `/etc`); everything under `/home`
(book masters, configuration, certificates) is preserved as-is. After updating, just re-run:

```sh
cd packaging && sh install-all.sh 10.11.99.1
```

to restore everything — every script is designed to be idempotent, so running it again is
never harmful. For a more detailed table of exactly what OTA affects, see
[`shelf/README.md`](shelf/README.md) (Chinese), section "固件升级（OTA）与恢复".

## Running into trouble

Start with the summary `install-all.sh` prints at the end to see exactly which step failed; the
corresponding `packaging/deploy-*.sh` script has detailed comments explaining what that step
does and its common failure causes. If you're still stuck, `packaging/README.md`'s "验证现状"
(verification status) section records every real-hardware issue found so far and how it was
fixed.
