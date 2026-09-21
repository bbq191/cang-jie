# cang-jie

**[中文](../README.md)**

A device-enhancement suite for the reMarkable Paper Pro Move — built **without modifying `xochitl`** (the device's
stock reading/notes app). It adds book management, note-taking enhancements and system tweaks using small plugins
loaded by [xovi](https://github.com/asivery/xovi) (a third-party extension loader) plus a set of standalone web
services that run on the device.

> **This page answers**: what is this, what can it do, how do I install/uninstall it, where are the details.
> For a 10-minute tour read [`OVERVIEW.md`](OVERVIEW.md) (Chinese); for what changed recently read [`CHANGELOG.md`](CHANGELOG.md) (Chinese).
>
> This is the **private** repository with the full development history. What is shared publicly is the trimmed release
> [`rm-tweak`](https://github.com/bbq191/rm-tweak) (no dev history, Apache-2.0; per its GitHub directory listing viewed on
> 2026-09-22 it contains shelf / notes / enhance / gateway / rmsvc-core / packaging, and no `defw/`).

## What it does

| You want | What provides it | Where |
|---|---|---|
| Get books onto the device and make them read well on e-ink | Web upload / article fetch → master library → one-click "optimize" (layout, table of contents, cover, footnotes) → deliver to the stock reader or KOReader; books beyond xochitl's ~100MB upload limit work too; comics get dedicated handling | [`shelf/`](../shelf/README.md) |
| Highlight + handwritten notes beside it, turned into organizable notes | Auto-ingested when you close the book → review/transcribe and ask an AI model on your phone → projected back into a device notebook or exported as Obsidian markdown | [`notes/`](../notes/README.md) |
| Small system-level improvements | Precise CJK highlight snapping, handwriting stroke rendering tuning, battery diagnostics, upload-and-use fonts/wallpapers | [`enhance/`](../enhance/README.md) |
| One place on your phone/computer to operate all of the above | A single HTTPS web entry (login password + reverse proxy to each service) | [`gateway/`](../gateway/README.md) |
| Install everything on a fresh device with one command | With a firmware-compatibility check; a symmetric one-command uninstall | [`packaging/`](../packaging/README.md) |

Two "behind the scenes" directories: [`rmsvc-core/`](../rmsvc-core/README.md) (the shared foundation crate of the web services, no business logic)
and [`defw/`](../defw/README.md) (Ghidra reverse-engineering artifacts for `xochitl` 3.28.0.172, used to locate hooks for the xovi extensions).

Everything runs on top of the device's **stock system**; `xochitl` is never repackaged. The overall architecture (see [`OVERVIEW.md`](OVERVIEW.md)):

![Overall architecture and deployment topology](diagrams/architecture-topology.svg)

## Quick start

Only supports the **reMarkable Paper Pro Move on firmware 3.28.0.172** (the only version verified so far; the installer checks and refuses otherwise).

1. First install the ecosystem infrastructure on the device by hand: `vellum add xovi`, `vellum add qt-resource-rebuilder`, `vellum add appload` (≥ 0.6.0), and sideload KOReader through appload. These are not part of this project and the installer won't do them for you.
2. Connect the computer to the device over USB, then:

   ```sh
   git clone https://github.com/bbq191/rm-tweak.git   # public release; the private dev repo cang-jie is maintainer-only
   cd rm-tweak/packaging
   sh install-all.sh 10.11.99.1
   ```
3. Open `https://10.11.99.1/` in a browser; the default password is `shelf` and you are forced to change it on first login.

To uninstall: `sh uninstall-all.sh 10.11.99.1`. Full prerequisites, recommended order, risk items and how to recover after a firmware update (OTA): **[INSTALL.en.md](INSTALL.en.md)**.

## Documentation map

| To learn about | Read |
|---|---|
| A 10-minute tour (architecture diagram, a book's journey, glossary) | [OVERVIEW.md](OVERVIEW.md) (Chinese) |
| Install / uninstall / recover after a firmware update | [INSTALL.en.md](INSTALL.en.md) |
| How the install scripts are structured and tested locally (developers) | [packaging/README.md](../packaging/README.md) (Chinese) |
| What changed recently | [CHANGELOG.md](CHANGELOG.md) (Chinese) |
| Details and decision records per line | each line's README and its `docs/` white paper (Chinese) |
| Sponsor | [DONATE.en.md](DONATE.en.md) |

## History and scope

The project started as a "reMarkable Chinese input method" (the repo name `cang-jie` / 仓颉 refers to the mythical inventor of Chinese characters), then grew
reading-enhancement, personal-knowledge-management and handwriting-recognition directions. On 2026-09-11 the repository went through a large cleanup: the
Chinese input method, the full reading pipeline and the PKM/handwriting-recognition lines — along with their reverse-engineering groundwork — were moved out of
this git repository (not deleted, relocated to a directory on the maintainer's own machine). **Those features are still deployed and running on the device**; their
source just isn't part of this repository anymore and isn't under active development. What is maintained here today is the lines listed above, with a more
conservative architecture: standalone web services plus a deliberately small xovi-extension surface, coupled more loosely to the stock system.

## Sponsor

If this project has been useful to you, feel free to buy the author a coffee — entirely optional, and it has no effect on any feature. QR codes etc. are on **[DONATE.en.md](DONATE.en.md)**.

## License / disclaimer

A personal-use project; no prebuilt binaries are distributed. Where third-party licensing is relevant (dictionary data, fonts, etc.), the in-code comments record what was
actually verified at the time — this is not legal advice. Not affiliated with reMarkable, [xovi](https://github.com/asivery/xovi), vellum, KOReader, or any other
third-party project or trademark mentioned here.
