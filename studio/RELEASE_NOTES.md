# Saya Studio 1.1.0 × Saya Couple 2.3.1

Saya Studio is a local desktop app that drives **ComfyUI** for anime-style character images. ComfyUI renders,
Saya Studio organizes: casting, a six-pass workflow, gallery, character sheets, LoRA manager, graph editor.
Nothing runs in the cloud.

**1.1.0 adds duo casting, gives invented characters names that suit them and delivers one clearly named file
per render.** Updating keeps your database, images, characters and settings.

## What's new in 1.1.0

### Casting

- **Duo trials (experimental):** when your draw engine provides pair types, Casting offers a Solo / Duo choice.
  A duo shows two validated characters on one image, at pass 1, and has no character sheet of its own.
- **Names that suit the character:** a character invented by a batch is renamed at launch, in a style that
  follows her family and P1 tags. A name already taken is never reused; existing characters keep theirs.
- **Saving a sheet:** the name field starts from the trial's name and stays editable. A saved sheet's name then
  wins in the history, the gallery and the file names of the images still to come.

### Files and gallery

- Delivered images are named `<local date>_<character>_<pose>.png` (finished) and `…_p<N>.png` (earlier passes).
- One image per render: the review gate's copy is no longer downloaded when the pass saved its own, and
  ComfyUI's copy in the run folder is removed once delivered and hash-checked.
- Dates use your local day, not UTC.

### Security

This release follows a full security audit of the app and the nodes. Nothing was known to be exploited.

- A crafted PNG can no longer write an image outside your output folder, nor use gigabytes of memory.
- A crafted workflow can no longer crash the app, and an unreadable one is refused instead of reported complete.
- The Civitai bridge sends your token to Civitai only, refuses non-Civitai and local addresses, checks every id,
  and never downloads another file than the one requested. An empty token file no longer opens it.
- Saya Couple 2.3.1: its HTTP routes refuse DNS rebinding and malformed bodies, and a status request creates no folder.
  A machine reached by its own name on a LAN lists that name in `SAYA_ALLOWED_HOSTS`.
- A duo stops at pass 1 everywhere: the app no longer offers the six passes on a duo trial.

## What was new in 1.0.1

### Fast on real libraries

Measured on a real library (432 casting runs, 3.8 GB database):

| Opening | 1.0.0 | 1.0.1 |
|---|---|---|
| Casting, Create, deleting a batch (run listing) | 1.5 s every time | 5 ms |
| Gallery | 1.8 s | 22 ms |
| Characters | 845 ms | 79 ms |
| Coming back to a page already visited | full reload | ~50 ms, state kept |

The background runner no longer re-reads every run manifest twice every 2.5 seconds while holding the run
lock, which stalled batch deletion and every casting action. Gallery, Casting, Characters and Models stay open
in memory: filters, scroll position, the open image and selections are still there when you come back.

### Casting

- **Six passes after the casting.** Full, keeping an image in the gallery or saving a character queues its six
  passes; they start one at a time once every pass-1 trial of the casting is done (an SDXL pass 1 and a HiDream
  pass 3 no longer share the GPU).
- **Missing images.** In « Cibler des personnages », characters without a picture take their turn: pass 1 in a
  random pose, then **Yes** (six passes, image kept as their picture) or **No** (another pose).
- **Pending actions.** Brouillons lists the bank actions your engine does not draw yet, with the reason. Test
  them in the isolated engine copy, then decide per action: **Validate** writes it through the engine's own
  command (with its backup), **Refuse** writes nothing.
- A failed or stopped trial no longer freezes the rest of its batch; picker tiles show names and images again;
  rotation images picture their character.

### Gallery and menus

- Casting images show the character's name instead of a job id.
- **Right-click menus** on gallery images, casting batches and trials, and models (open, six passes, favourite,
  compare, copy path, delete a batch…). The browser's Back/Reload menu no longer appears in the app.
- **Send to the full workflow** opens an image in Create with the workflow saved inside it.

### Workflows

- **Node check:** the app lists the nodes a workflow needs and those missing from your ComfyUI. Ordinary
  workflows using a Primitive node, a Note or rgthree helpers were wrongly refused; they now open. Scripts can
  use `studio-maintenance check-workflow FILE_OR_FOLDER object_info.json` (exit 0 complete, 2 missing).
- **Developer mode** (Settings): an action console, a preference editor, data folders and the node check.

### Updates

- **The app checks for a new release at startup** and offers to install it: it downloads the package for your
  system, verifies it against `SHA256SUMS`, then asks your system for permission to install it. You can turn
  the check off in Settings.
- **`install.sh` detects your system** and has an update mode: `sh install.sh --update`.

## Linux installer: Couple nodes, Studio app, or both

Download **[install.sh](https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/install.sh)**
from this release, then run it in a terminal:

```sh
sh install.sh
```

Choose **1: Couple nodes**, **2: Studio app**, or **3: both**. The installer downloads the public Couple
bundle, verifies its pinned SHA-256, and opens the existing installer. It detects the Linux app package,
checks its release checksum, and asks before installation. To choose both and specify ComfyUI explicitly:

```sh
sh install.sh --what both --comfyui /path/to/ComfyUI
```

For the **nodes as a separate download**, use
**[saya-couple-2.3.1.tar.gz](https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.1/saya-couple-2.3.1.tar.gz)**:
extract it and run `./saya install`.

## Install

| System | File | Command |
|---|---|---|
| Debian, Ubuntu | `saya-studio_1.0.1_amd64.deb` | `sudo apt install ./saya-studio_1.0.1_amd64.deb` |
| Fedora | `saya-studio-1.0.1.fedora.x86_64.rpm` | `sudo dnf install ./saya-studio-1.0.1.fedora.x86_64.rpm` |
| openSUSE | `saya-studio-1.0.1.opensuse.x86_64.rpm` | `sudo zypper install ./saya-studio-1.0.1.opensuse.x86_64.rpm` |
| Arch, EndeavourOS | `saya-studio-1.0.1-x86_64.pkg.tar.zst` | `sudo pacman -U ./saya-studio-1.0.1-x86_64.pkg.tar.zst` |
| Any Linux distribution | `saya-studio_1.0.1_amd64.AppImage` | `chmod +x` it, then run it |
| Flatpak | `saya-studio-1.0.1-x86_64.flatpak` | `flatpak install --user ./saya-studio-1.0.1-x86_64.flatpak` |
| Nix | flake in `studio/` | `nix run github:alphaziod/saya-studio-x-couple?dir=studio` |
| Windows 10/11 x64 | `saya-studio-1.0.1-windows-x64.exe` | run the installer (WebView2 is handled) |
| Mac, Apple Silicon | `saya-studio-1.0.1-macos-arm64.dmg` | open it, drag Saya Studio into Applications |
| Mac, Intel | `saya-studio-1.0.1-macos-x64.dmg` | open it, drag Saya Studio into Applications |

Check your download: `sha256sum -c SHA256SUMS --ignore-missing`.

Windows builds are unsigned; Mac builds use ad-hoc signing and are not Apple-notarized, so the OS may block
the first launch. Updating keeps your database, images, characters and settings.

## Requirements

- A running ComfyUI (default `~/AI/ComfyUI`, on `127.0.0.1:8188`) with the **Saya Couple nodes 2.3.0** or newer
  from this repository, plus the models your workflow uses.
- Python 3. Optional: `ffmpeg` for LoRA video previews.
- Linux: WebKitGTK 4.1 (Debian 12+, Ubuntu 22.04+, Fedora 39+, openSUSE Tumbleweed / Leap 15.6, Arch, NixOS).

## Validation

- Local: 503 Rust tests passed (34 explicitly ignored), 109 frontend tests passed, Svelte check with no errors or
  warnings, interface translations complete in 14 languages; installer tests (bundle, update mode, 7 distribution
  detections) pass without network.
- Speed measured on a copy of a real library (432 runs, 763 MB of run manifests, 3.8 GB database); the table
  above gives those numbers.
- Workflows: 30 synthetic workflows and 30 invented node classes give the exact expected report; the bundled
  presets are complete against a real ComfyUI 0.34.0 node list.
- Native app checks (real window, simulated ComfyUI): casting pickers, « Images manquantes » (Yes, No, Skip),
  gallery names and right-click menu, developer mode and node check, page switching (~50 ms, no reload).
- Not verified end to end: installing an update through `pkexec` and `install.sh --update` against this
  release (both are covered by tests with recorded release data), and a resume after actually killing ComfyUI.

## Known limitations

- **Casting needs a draw engine**, which is not distributed; without one, Casting says so and stays unavailable.
- **Flatpak** runs sandboxed: ComfyUI on `127.0.0.1` and your home folder are allowed, but the browser
  extension bridge may not work there.
- The bundled preset ships with **neutral example prompts and empty LoRA stacks**: fill in your own. Its model
  settings are mine; keep the ones that work on your machine.

## Also in this repository

The **Saya Couple 2.3.1** node pack only hardens its HTTP routes (see above). See
[CHANGELOG.md](https://github.com/alphaziod/saya-studio-x-couple/blob/main/CHANGELOG.md).

## Documentation

[README](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/README.md) ·
[Packaging](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/docs/PACKAGING.md) ·
[For AI agents and contributors](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/AGENTS.md)

License: GPL-3.0-or-later.
