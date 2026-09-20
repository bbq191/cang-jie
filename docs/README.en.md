# cang-jie

**[中文](../README.md)**

A device-enhancement suite for the reMarkable Paper Pro Move — built **without modifying
`xochitl`** (the device's stock reading app) itself. Everything runs as xovi extensions plus a
set of standalone web services alongside the official system.

> **Audience and purpose**: anyone landing on this repository for the first time. It answers "what is this,
> what are the parts, how do I install it, where are the details". For a 10-minute tour read
> [`OVERVIEW.md`](OVERVIEW.md) (Chinese); for what changed recently read [`CHANGELOG.md`](CHANGELOG.md) (Chinese).
>
> This is the **private** repository with the full development history. A trimmed public release also exists at
> [`rm-tweak`](https://github.com/bbq191/rm-tweak) (no dev history, just the current seven project lines' code plus the
> top-level docs, Apache-2.0-licensed) — share/star that one.

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
| [`shelf/`](../shelf/) — bookshelf | Import books (EPUB/PDF) → master library → optimize on demand → deliver to the stock library or KOReader; web-only, including a batch queue | [README](../shelf/README.md) |
| [`notes/`](../notes/) — notes pipeline | Highlighted text + handwritten notes next to it, auto-ingested on closing the book → review/transcribe/ask-AI on your phone → project back into device notebooks or Obsidian | [README](../notes/README.md) |
| [`enhance/`](../enhance/) — device enhancements | Precise CJK highlight-snap and handwritten-stroke rendering tuning (two standalone xovi extensions); a battery diagnostics sampler; upload-and-use font/wallpaper web services | [README](../enhance/README.md) |
| [`gateway/`](../gateway/) — web gateway | The single shared web entry point for the three lines above: HTTPS (private CA) + login password + reverse proxy to each domain service + batch queue and concurrency gate | [README](../gateway/README.md) |
| [`rmsvc-core/`](../rmsvc-core/) — service foundation | Shared infrastructure crate for the web services above (paths / service registry / HTTP adapter / event bus, etc.) — no business logic | [README](../rmsvc-core/README.md) |
| [`defw/`](../defw/) — firmware reverse engineering | Ghidra reverse-engineering artifacts for `xochitl` 3.28.0.172, backing the hook locations used by the extensions above | [README](../defw/README.md) |
| [`packaging/`](../packaging/) — installer | One command to install everything above on a fresh device (with firmware-compatibility checking) | [README](../packaging/README.md) |

## Recent updates

User-visible changes are logged by date in **[CHANGELOG.md](CHANGELOG.md)** (Chinese). The latest: comic optimization stays EPUB, unified book naming (`Title - 02卷`), the ~100MB xochitl upload limit bypassed (placeholder + on-disk replace), a server-side batch queue and a redone master-library page, faster optimization and lower battery draw.

## Quick start

Only supports the **reMarkable Paper Pro Move on firmware 3.28.0.172** (the only version
verified so far).

```sh
git clone https://github.com/bbq191/rm-tweak.git   # public release; the private dev repo cang-jie is maintainer-only
cd rm-tweak/packaging
sh install-all.sh 10.11.99.1
```

For full prerequisites, a step-by-step breakdown, the firmware safety gate, and
troubleshooting, see **[INSTALL.en.md](INSTALL.en.md)**.

## Documentation map

| To learn about | Read |
|---|---|
| A 10-minute tour (architecture diagram, a book's journey, glossary) | [OVERVIEW.md](OVERVIEW.md) (Chinese) |
| How to install / recover after a firmware update | [INSTALL.en.md](INSTALL.en.md) |
| What changed recently | [CHANGELOG.md](CHANGELOG.md) (Chinese) |
| Details and decision records per line | each line's README and its `docs/` white paper (Chinese) |

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
