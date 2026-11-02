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
| Snap | `saya-studio_1.0.0-beta.2_amd64.snap` | `sudo snap install --dangerous ./saya-studio_1.0.0-beta.2_amd64.snap` |
| Nix | flake in `studio/` | `nix run github:alphaziod/saya-studio-x-couple?dir=studio` |

Check your download: `sha256sum -c SHA256SUMS --ignore-missing`.

## Changes in beta.2

- Readable controls, labels and toolbars across Casting, Gallery, Characters, Models, Generation,
  Workflows and Settings; quieter borders and better spacing on smaller windows.
- Casting discovers existing Python test scripts in the configured engine tree and `scripts-tests`
  folder. Select a test and use **Generate**; refresh the list after adding a script.
- Test execution reports the actual exit status and output, rejects unknown selections, and prevents
  overlapping test requests. Listing never executes a script. The external draw engine is not bundled.
- Improved grid sizing, wrapped filters and editor toolbars without changing graph canvas coordinates.
- Updated interface wording in all 14 languages.

## Known limitations

- **Beta.** Used day to day on Arch (Wayland) with an AMD GPU. The other packages are built and checked by CI but
  have not all been run by hand yet: please report what breaks.
- **Casting needs a draw engine**, which is not distributed; without one, Casting says so and stays unavailable.
- **Flatpak and Snap** run sandboxed: ComfyUI on `127.0.0.1` and your home folder are allowed, but the browser
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
