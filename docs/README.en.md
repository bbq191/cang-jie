# cang-jie

**[中文](../README.md)**

A device-enhancement suite for the reMarkable Paper Pro Move. It **does not modify `xochitl`** (the device's stock
reading/notes app). Instead it adds features with two things: small plugins loaded by [xovi](https://github.com/asivery/xovi)
(a third-party extension loader), and a set of web services that run on the device. What it adds: book management,
note organizing, and some small system-level improvements.

> **This page covers**: what it does, how to install and uninstall it, where to find the details.
> For a 10-minute tour read [`OVERVIEW.md`](OVERVIEW.md) (Chinese); for recent changes read [`CHANGELOG.md`](CHANGELOG.md) (Chinese).
>
> This is the **private** repository with the full development history. What is shared publicly is the trimmed release
> [`rm-tweak`](https://github.com/bbq191/rm-tweak) (no dev history, Apache-2.0). It was last synced on 2026-09-25:
> it has shelf / notes / enhance / gateway / rmsvc-core / packaging (including `install-all.sh` and `verify-on-device.sh`), and no `defw/`.

## What it does

| You want | What provides it | Where |
|---|---|---|
| Get books onto the device and make them read well on e-ink | Upload on the web page or fetch an article; books land in the "master library" first. EPUBs get one-click "optimize" (layout, table of contents, cover, footnotes); PDFs with a text layer are converted to EPUB keeping their original formatting. Then deliver to the stock reader or KOReader (KOReader comes with two reading presets, one for text books and one for comics). Books over xochitl's ~100MB upload limit work too; comics get dedicated handling; books in the master library can be downloaded as the original file, renamed, or given a reading direction (right-to-left for manga) | [`shelf/`](../shelf/README.md) |
| Turn highlighter marks and handwritten notes beside them into organizable notes | Collected automatically when you close the book. On your phone: review the handwriting transcription, full-text search, ask an AI model; then send back into a device notebook (headings, lists and checkboxes use the device's own styles) or export as Obsidian markdown | [`notes/`](../notes/README.md) |
| Small system-level improvements | Precise CJK highlighter snapping, handwriting stroke-width tuning, tap-to-turn and a manga (right-to-left) page-turn rule in the xochitl reader, battery drain diagnostics, upload-and-use fonts and wallpapers | [`enhance/`](../enhance/README.md) |
| One place on your phone/computer to operate all of the above | A single HTTPS web entry with a login password that forwards requests to each service; plus a "Device health" page and a reinstall hint after firmware updates | [`gateway/`](../gateway/README.md) |
| Install everything on a new device with one command | With a firmware-compatibility check; reboots the device once at the end and runs a read-only check; one-command uninstall too | [`packaging/`](../packaging/README.md) |

Two "behind the scenes" directories: [`rmsvc-core/`](../rmsvc-core/README.md) (the shared base library of the web services, no business logic)
and [`defw/`](../defw/README.md) (reverse-engineering material for `xochitl` 3.28.0.172, used to find hook points for the xovi plugins).

Everything runs on the device's **stock system**; `xochitl` is never repackaged. Overall architecture (details in [`OVERVIEW.md`](OVERVIEW.md)):

![Overall architecture and deployment topology](diagrams/architecture-topology.svg)

## Quick start

Only the **reMarkable Paper Pro Move on firmware 3.28.0.172** is supported. It is the only version verified so far; the installer checks and refuses otherwise by default.

1. Install four base components on the device by hand: `vellum add xovi`, `vellum add qt-resource-rebuilder`, `vellum add appload` (≥ 0.6.0), then sideload KOReader through appload. They are not part of this project and the installer won't install them.
2. Your computer needs a Rust cross-compilation setup and passwordless ssh to the device's root (list in [INSTALL.en.md "Before you install"](INSTALL.en.md#before-you-install)). Connect the device over USB, then:

   ```sh
   git clone https://github.com/bbq191/rm-tweak.git
   cd rm-tweak/packaging
   sh install-all.sh --dry-run        # rehearse first: prints the plan, never touches the device
   sh install-all.sh 10.11.99.1
   ```
   The last step reboots the device once (about a minute); when it is back, the script checks it automatically and marks each item ✓/⚠/✗.
3. Open `https://10.11.99.1/` in a browser. The default password is `shelf`, and you must change it on first login.

To uninstall: `sh uninstall-all.sh 10.11.99.1`. Prerequisites, risks and how to recover after a firmware update (OTA) are all in **[INSTALL.en.md](INSTALL.en.md)**.

## Documentation map

![Documentation map (labels in Chinese)](diagrams/docs-map.svg)

| To learn about | Read |
|---|---|
| A 10-minute tour (architecture diagram, a book's journey, glossary) | [OVERVIEW.md](OVERVIEW.md) (Chinese) |
| Install, uninstall, recover after a firmware update | [INSTALL.en.md](INSTALL.en.md) |
| How the install scripts are structured and tested locally (developers) | [packaging/README.md](../packaging/README.md) (Chinese) |
| What changed recently | [CHANGELOG.md](CHANGELOG.md) (Chinese) |
| Details and decision records per line | each directory's README and its `docs/` white paper (Chinese) |
| Sponsor | [DONATE.en.md](DONATE.en.md) |

## History and scope

The project started as a "reMarkable Chinese input method" (the repo name `cang-jie` / 仓颉 refers to the mythical inventor of
Chinese characters), then grew reading enhancements, knowledge management and handwriting recognition. On 2026-09-11 the
repository was reorganized: the Chinese input method, the early full reading pipeline, knowledge management and handwriting
recognition, together with their reverse-engineering material, were moved out of this repository. **Their source is not here,
and this repository's installer does not distribute them.**

What is maintained here today is the lines in the table above. The architecture is more conservative than before: mostly
standalone web services, only a few xovi plugins, touching the stock system's internals as little as possible.

## Sponsor

If this project has been useful to you, feel free to buy the author a coffee. It is entirely optional and has no effect on any feature. QR codes are on **[DONATE.en.md](DONATE.en.md)**.

## License / disclaimer

A personal-use project; no prebuilt binaries are distributed. Where third-party licensing is relevant (dictionary data, fonts, etc.),
the in-code comments record what was actually verified. This is not legal advice. Not affiliated with reMarkable,
[xovi](https://github.com/asivery/xovi), vellum, KOReader, or any other third-party project or trademark mentioned here.
