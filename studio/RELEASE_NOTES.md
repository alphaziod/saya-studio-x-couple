# Saya Studio 1.0.0-beta.2 — interface and Casting update

Saya Studio is a local desktop app that drives **ComfyUI** for anime-style character images. ComfyUI renders,
Saya Studio organizes: casting, a six-pass workflow, gallery, character sheets, LoRA manager, graph editor.
Nothing runs in the cloud.

This is a **beta**. It is the companion app of the Saya Couple node pack and, like the pack, first a backup of my
own setup. Back up your data and your ComfyUI folder before trying it.

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
