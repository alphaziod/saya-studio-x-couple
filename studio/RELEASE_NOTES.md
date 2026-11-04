# Saya Studio 1.0.0-beta.2 × Saya Couple 2.3.0

Saya Studio is a local desktop app that drives **ComfyUI** for anime-style character images. ComfyUI renders,
Saya Studio organizes: casting, a six-pass workflow, gallery, character sheets, LoRA manager, graph editor.
Nothing runs in the cloud.

This is a **beta**. It is the companion app of the Saya Couple node pack and, like the pack, first a backup of my
own setup. Back up your data and your ComfyUI folder before trying it.

## Windows and macOS downloads

Native assets use source commit [`9cd643d`](https://github.com/alphaziod/saya-studio-x-couple/commit/9cd643d65881bdf77ef44336a2bfcf7b75621729); the existing Linux packages and Couple archive are unchanged.

[Native installation, startup and browser checks](https://github.com/alphaziod/saya-studio-x-couple/actions/runs/41745313792) passed on all three platforms on 2026-11-04, using these exact installers. Preset hashes and initial database setup were verified. The packaged extension connected through the real registered host to the running app in Chromium; an unrelated extension was refused. Native trash tests moved and restored a synthetic file on each platform during the [build run](https://github.com/alphaziod/saya-studio-x-couple/actions/runs/41692142023). That first run built all three installers but failed its browser harness; the linked successful check retested the same binaries with the corrected harness. No GPU generation was run.

- **Windows 10/11 x64:** `saya-studio-1.0.0-beta.2-windows-x64.exe`. Run the installer; WebView2 is required (the installer handles it).
- **Mac with Apple Silicon:** `saya-studio-1.0.0-beta.2-macos-arm64.dmg`.
- **Mac with Intel:** `saya-studio-1.0.0-beta.2-macos-x64.dmg`.

On macOS 11 or newer, open the DMG and drag Saya Studio into Applications. Windows builds are unsigned;
Mac builds use ad-hoc signing and are not Apple-notarized, so the OS may block first launch.
Python 3 and a working ComfyUI installation remain separate requirements. The private draw engine is not included.
These packages include the same public presets as Linux. Native package installation, startup and preset checks
run in CI; a complete manual UI/GPU workflow has not been validated on these platforms.
The gallery now uses Windows Recycle Bin and macOS Trash, with recoverable deletion and no permanent-delete fallback.
Launching Studio registers its browser host for Chrome, Chromium, Brave and Edge on Windows and macOS;
the extension connects without manually copying a token. Linux keeps its existing integration.
The nodes remain available in `saya-couple-2.3.0.tar.gz`; extract it and run `python installer/saya.py install`
(or `python3` on macOS) from that folder. `install.sh` selects Linux app packages only.

## Linux installer: Couple nodes, Studio app, or both

Download **[install.sh](https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.0-beta.2/install.sh)**
from this release, then run it in a terminal:

```sh
sh install.sh
```

Choose **1: Couple nodes**, **2: Studio app**, or **3: both**. The installer downloads the public Couple
bundle, verifies its pinned SHA-256, and opens the existing installer. It detects the Linux app package,
checks its release checksum, and asks before installation. Existing node backups and restore are preserved.

To choose both and specify ComfyUI explicitly:

```sh
sh install.sh --what both --comfyui /path/to/ComfyUI
```

Requires `python3`, `curl`, `tar` and `sha256sum`. The shell launcher must be run as your normal user;
the app installer requests `sudo` only when the selected system package needs it.

For the **nodes as a separate download**, use
**[saya-couple-2.3.0.tar.gz](https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.0-beta.2/saya-couple-2.3.0.tar.gz)**.
It contains the public node files, workflows, installer and licenses. Extract it and run `./saya install`.
Both downloads are listed in `SHA256SUMS` alongside the app packages.

## Requirements

- Linux x86_64 with WebKitGTK 4.1 (Debian 12+, Ubuntu 22.04+, Fedora 39+, openSUSE Tumbleweed / Leap 15.6,
  Arch / EndeavourOS, NixOS).
- A running ComfyUI (default `~/AI/ComfyUI`, on `127.0.0.1:8188`) with the **Saya Couple nodes 2.3.0** or newer
  from this repository (`./saya install --comfyui /path/to/ComfyUI`), plus the models your workflow uses.
- Python 3. Optional: `ffmpeg` for LoRA video previews.

## Install

| Distribution | File | Command |
|---|---|---|
| Debian, Ubuntu | `saya-studio_1.0.0-beta.2_amd64.deb` | `sudo apt install ./saya-studio_1.0.0-beta.2_amd64.deb` |
| Fedora | `saya-studio-1.0.0-beta.2.fedora.x86_64.rpm` | `sudo dnf install ./saya-studio-1.0.0-beta.2.fedora.x86_64.rpm` |
| openSUSE | `saya-studio-1.0.0-beta.2.opensuse.x86_64.rpm` | `sudo zypper install ./saya-studio-1.0.0-beta.2.opensuse.x86_64.rpm` |
| Arch, EndeavourOS | `saya-studio-1.0.0-beta.2-x86_64.pkg.tar.zst` | `sudo pacman -U ./saya-studio-1.0.0-beta.2-x86_64.pkg.tar.zst` |
| Any distribution | `saya-studio_1.0.0-beta.2_amd64.AppImage` | `chmod +x` it, then run it |
| Flatpak | `saya-studio-1.0.0-beta.2-x86_64.flatpak` | `flatpak install --user ./saya-studio-1.0.0-beta.2-x86_64.flatpak` |
| Nix | flake in `studio/` | `nix run github:alphaziod/saya-studio-x-couple?dir=studio` |

Check your download: `sha256sum -c SHA256SUMS --ignore-missing`.

## Changes in beta.2

### Interface throughout the app

- Larger, clearer labels and body text, more readable secondary text, and consistent control heights.
- Wider navigation, simpler headings, quieter borders and more space between controls.
- Toolbars and filters wrap on smaller windows; panels and grids keep their content readable.

### Casting and test scripts

- Clearer batch controls, full-width character/action choices and a dedicated row for the result,
  family, checkpoint, LoRA and sort filters.
- New **Test scripts** section with automatic discovery of existing `test_*.py` and `tester.py` files
  in the configured engine tree and the `scripts-tests` config folder, plus the existing LCS launcher.
  The list refreshes when the section opens and with **Refresh**, so newly added scripts can appear
  without maintaining a hardcoded list.
- Select a test and click **Generate**. Discovery does not execute or import scripts. Ordinary engine
  modes remain available separately; unit-test directories, archives, output folders and symbolic
  links are excluded from discovery.
- Execution displays the actual success or failure, exit code, timeout and captured output. Unknown
  selections are rejected; double submissions and overlapping test requests are blocked. Python runs
  from the selected script's folder, adding `--gpu` when that option is detected in its source.

### Gallery, characters and models

- Gallery toolbars and filters wrap correctly; corrected virtual-grid spacing keeps the displayed
  columns aligned with the available width.
- Character lists and sheets have more readable text, better column sizing and revision rows that wrap.
  The default avatar is simpler.
- LoRA and checkpoint controls, filters and inspectors have clearer spacing and readable actions,
  including on narrower windows. References, character galleries and import panels share the same styling.

### Generation and workshop

- Generation and Saya Couple panels have clearer workflow/preparation labels, a dedicated row for saved
  preparations, wider controls and more readable pass/node cards.
- Workflow, module and pipeline views use roomier columns and tables. Graph editor tools sit in the
  page layout instead of covering the canvas; graph and pipeline canvas coordinates are preserved.
- Scene fields wrap into readable columns. A/B lab, prompt tools, engine controls, data management,
  settings, comparisons and the image editor use the updated text sizes and spacing.

### Languages and existing data

- New test-script labels and corrected import wording are translated across all 14 interface languages.
- Existing public presets and the Saya Couple 2.3.0 node package are unchanged. No personal images,
  databases, local draw-engine scripts or private work logs are included in this update.

## Validation and package status

- Local verification: 460 Rust tests passed, 32 explicitly ignored; 99 frontend tests and 10 IPC tests
  passed. Svelte checking and the frontend build passed.
- Native UI checks covered test-script discovery and execution, real failure reporting and duplicate-click
  protection with synthetic scripts. Visual checks covered full-HD and narrower layouts, graph drag/undo,
  dialogs and right-to-left display. No real GPU generation was performed for this interface update.
- The [tag workflow](https://github.com/alphaziod/saya-studio-x-couple/actions/runs/41424291906/attempts/2)
  completed: Arch, Debian/Fedora/AppImage, openSUSE, Nix, Flatpak and the release publication job succeeded.
  Snap failed while downloading its build tooling from the Snapcraft store. No checks were cancelled in
  this attempt. Published file digests were checked against `SHA256SUMS` and GitHub's SHA-256 values.

## Known limitations

- **Snap is not attached to beta.2.** CI could not reliably download the build prerequisites
  from the Snapcraft store. The Snap recipe remains available in `packaging/snap/snapcraft.yaml`.

- **Beta.** Used day to day on Arch (Wayland) with an AMD GPU. The other packages are built and checked by CI but
  have not all been run by hand yet: please report what breaks.
- **Casting needs a draw engine**, which is not distributed; without one, Casting says so and stays unavailable.
- **Flatpak** runs sandboxed: ComfyUI on `127.0.0.1` and your home folder are allowed, but the browser
  extension bridge may not work there.
- The bundled preset ships with **neutral example prompts and empty LoRA stacks**: fill in your own. Its model
  settings are mine; keep the ones that work on your machine.

## Also in this repository

The **Saya Couple 2.3.0** node pack remains unchanged. See
[CHANGELOG.md](https://github.com/alphaziod/saya-studio-x-couple/blob/main/CHANGELOG.md).

## Documentation

[README](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/README.md) ·
[Packaging](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/docs/PACKAGING.md) ·
[For AI agents and contributors](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/AGENTS.md)

License: GPL-3.0-or-later.
