# cang-jie

**[中文](../README.md)**

A device-enhancement suite for the reMarkable Paper Pro Move — built **without modifying
`xochitl`** (the device's stock reading app) itself. Everything runs as xovi extensions plus a
set of standalone web services alongside the official system.

> This is the **private** repository with the full development history. As of 2026-09-11 a
> trimmed public release also exists at [`rm-tweak`](https://github.com/bbq191/rm-tweak) (no
> dev history, just the current six project lines' code plus these four top-level docs,
> Apache-2.0-licensed) — share/star that one; this repository stays internal-only.

## What this is

A personal-use toolkit built around one e-ink tablet: get books onto the device, optionally
clean up/convert/optimize them, choose which reader to hand them to (the stock `xochitl` reader
or KOReader); highlight text and jot handwritten notes next to it, have those automatically
picked up, transcribed and answered by an AI model on your phone, then projected back into the
device's own notebooks or into Obsidian. On top of that, a handful of low-level single-purpose
enhancements: precise CJK highlight-snapping, handwritten-stroke rendering tuning, a battery
diagnostics sampler, and one-upload font/wallpaper installs. Everything runs on top of the
device's stock firmware via [xovi](https://github.com/asivery/xovi) (a third-party extension
loader) plus a set of lightweight, independent web services — no repackaging or patching of
`xochitl` itself.

## What's in here

Seven independent top-level project lines, each individually installable:

| Project | What it is | Docs |
|---|---|---|
| [`shelf/`](../shelf/) — bookshelf | Import books → clean up / convert / optimize → deliver to the stock library or KOReader; a web UI plus a host-side CLI | [README](../shelf/README.md) |
| [`notes/`](../notes/) — notes pipeline | Highlighted text + handwritten notes next to it, auto-ingested on closing the book → review/transcribe/ask-AI on your phone → project back into device notebooks or Obsidian | [README](../notes/README.md) |
| [`enhance/`](../enhance/) — device enhancements | Precise CJK highlight-snap and handwritten-stroke rendering tuning (two standalone xovi extensions); a battery diagnostics sampler; upload-and-use font/wallpaper web services | [README](../enhance/README.md) |
| [`gateway/`](../gateway/) — web gateway | The single shared web entry point for the three lines above: HTTPS (private CA) + login password + reverse proxy to each domain service | [README](../gateway/README.md) |
| [`rmsvc-core/`](../rmsvc-core/) — service foundation | Shared infrastructure crate for the web services above (paths / service registry / HTTP adapter / event bus, etc.) — no business logic | [README](../rmsvc-core/README.md) |
| [`defw/`](../defw/) — firmware reverse engineering | Ghidra reverse-engineering artifacts for `xochitl` 3.28.0.172, backing the hook locations used by the extensions above | [README](../defw/README.md) |
| [`packaging/`](../packaging/) — installer | One command to install everything above on a fresh device (with firmware-compatibility checking) | [README](../packaging/README.md) |

## Recent updates

Only actual new features/capabilities, not a full commit log; the detailed pitfall record for
each change lives in the corresponding project line's whitepaper (Chinese only).

| Date | Added |
|---|---|
| 2026-09-13 | A Sidebar shortcut straight to "KOReader"; if the third-party WeRead app is installed it's auto-detected and a "WeRead" entry appears too — both are wired into `packaging/`, repeatable via `install-all.sh` |
| 2026-09-13 | The gateway's "Setup · foundation" page now also probes whether WeRead is installed (read-only, same as the KOReader check) |
| 2026-09-11 | `packaging/install-all.sh`: one command installs everything on a fresh device, including a firmware safety gate (sha256 match required, refuses otherwise) |
| 2026-09-11 | Added xovi boot-persistence (auto re-runs `xovi/start` after a reboot), domestic NTP, and a default timezone — three system-level config steps |
| 2026-09-11 | Top-level bilingual README/INSTALL docs and a donation channel went live |

## Quick start

Only supports the **reMarkable Paper Pro Move on firmware 3.28.0.172** (the only version
verified so far).

```sh
git clone git@github.com:bbq191/cang-jie.git
cd cang-jie/packaging
sh install-all.sh 10.11.99.1
```

For full prerequisites, a step-by-step breakdown, the firmware safety gate, and
troubleshooting, see **[INSTALL.en.md](INSTALL.en.md)**.

## History and scope

This project started out as a "reMarkable Chinese input method" (the repo name `cang-jie` /
仓颉 refers to the mythical inventor of Chinese characters), then grew reading-enhancement,
personal-knowledge-management, and handwriting-recognition directions over time. On
2026-09-11 the repository went through a large-scale cleanup: the Chinese input method, the
full reading pipeline, and the PKM/handwriting-recognition lines — along with their reverse-
engineering groundwork — were moved out of this git repository entirely (not deleted, just
relocated to a directory on the maintainer's own machine that isn't tracked by git). **Those
features are still deployed and running on the device today**; their source just isn't part of
this repository anymore, and they're no longer under active development here. What's actively
maintained in this repository today is the seven lines listed above, built with a more
conservative architecture (standalone web services plus a deliberately small xovi-extension
surface) than the earlier, more deeply invasive hook suite.

## Sponsor

If this project has been useful to you, feel free to buy the author a coffee — entirely
optional, and has no effect on any feature whether you do or don't. QR codes and how the funds
are used are on a separate page: **[DONATE.en.md](DONATE.en.md)**.

## License / disclaimer

A personal-use project; no prebuilt binaries are distributed. Where third-party licensing is
relevant (dictionary data, fonts, etc.), the in-code comments record what was actually verified
at the time — this is not legal advice. Not affiliated with reMarkable,
[xovi](https://github.com/asivery/xovi), vellum, KOReader, or any other third-party project or
trademark mentioned here.
