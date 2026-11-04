# Saya Studio 1.0.0 × Saya Couple 2.3.0

Saya Studio is a local desktop app that drives **ComfyUI** for anime-style character images. ComfyUI renders,
Saya Studio organizes: casting, a six-pass workflow, gallery, character sheets, LoRA manager, graph editor.
Nothing runs in the cloud.

**1.0.0 is the first stable release.** It focuses on reliability: a generation interrupted by a ComfyUI crash,
a failed pass or a stop now resumes where it stopped, stopping really stops, and image cleanup never touches
what is not Studio's. It is the companion app of the Saya Couple node pack and, like the pack, first a backup
of my own setup. Back up your data and your ComfyUI folder before upgrading.

## What's new in 1.0.0

### Crash recovery in Create and Casting

- If ComfyUI crashes or restarts during a six-pass run, a pass fails, or you stop the run, **Resume from
  pass N** restarts the unfinished pass with the **same seed**. Finished passes are kept: the pass reads the
  previous pass's state that ComfyUI saved on disk.
- A pass lost by a ComfyUI restart is first confirmed gone (targeted queue delete, then absence from the
  queue and the history) before it is replaced. A pass that ComfyUI may still be running is never sent twice.
- The same button appears in the casting sheet of a kept trial whose six-pass finish was interrupted.
  Pass-1 trials keep the batch **Resume** button.

### Stop and restart

- **Stop** now removes a queued pass from the ComfyUI queue. It used to send an interrupt, which only reaches
  the prompt being executed, so the queued pass still ran on the GPU after the run was marked stopped.
- **Restart** is refused while a pass is still owned by ComfyUI, so two attempts never run at once.
- Targeted interrupt accepts ComfyUI 0.34.0 **and newer** (it required exactly 0.34.0, so Stop was refused
  on any newer ComfyUI).

### Casting

- Resuming a batch no longer revives trials you already decided (rejected, or a stopped finish), which then
  showed as running forever.

### Your images and data

- Replacing a lineage never hides an image of **your own library** that a delivery was deduplicated onto;
  only files inside Studio's own folders are erased.
- Every erased image is journaled in `pass-states/preview-cleanup/removed-assets.log`.
- The startup preview cleanup now runs under the run lock: it could overwrite a pass just submitted by the
  background runner, which then submitted it again. A deletion error no longer loses the cleanup journal.

### Interface

- The error banner at the top of the app clears after the next successful navigation.
- README screenshots refreshed from the current interface.
- New labels translated in all 14 interface languages.

## Linux installer: Couple nodes, Studio app, or both

Download **[install.sh](https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.0/install.sh)**
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
**[saya-couple-2.3.0.tar.gz](https://github.com/alphaziod/saya-studio-x-couple/releases/download/studio-v1.0.0/saya-couple-2.3.0.tar.gz)**:
extract it and run `./saya install`.

## Install

| System | File | Command |
|---|---|---|
| Debian, Ubuntu | `saya-studio_1.0.0_amd64.deb` | `sudo apt install ./saya-studio_1.0.0_amd64.deb` |
| Fedora | `saya-studio-1.0.0.fedora.x86_64.rpm` | `sudo dnf install ./saya-studio-1.0.0.fedora.x86_64.rpm` |
| openSUSE | `saya-studio-1.0.0.opensuse.x86_64.rpm` | `sudo zypper install ./saya-studio-1.0.0.opensuse.x86_64.rpm` |
| Arch, EndeavourOS | `saya-studio-1.0.0-x86_64.pkg.tar.zst` | `sudo pacman -U ./saya-studio-1.0.0-x86_64.pkg.tar.zst` |
| Any Linux distribution | `saya-studio_1.0.0_amd64.AppImage` | `chmod +x` it, then run it |
| Flatpak | `saya-studio-1.0.0-x86_64.flatpak` | `flatpak install --user ./saya-studio-1.0.0-x86_64.flatpak` |
| Nix | flake in `studio/` | `nix run github:alphaziod/saya-studio-x-couple?dir=studio` |
| Windows 10/11 x64 | `saya-studio-1.0.0-windows-x64.exe` | run the installer (WebView2 is handled) |
| Mac, Apple Silicon | `saya-studio-1.0.0-macos-arm64.dmg` | open it, drag Saya Studio into Applications |
| Mac, Intel | `saya-studio-1.0.0-macos-x64.dmg` | open it, drag Saya Studio into Applications |

Check your download: `sha256sum -c SHA256SUMS --ignore-missing`.

Windows builds are unsigned; Mac builds use ad-hoc signing and are not Apple-notarized, so the OS may block
the first launch. Updating keeps your database, images, characters and settings.

## Requirements

- A running ComfyUI (default `~/AI/ComfyUI`, on `127.0.0.1:8188`) with the **Saya Couple nodes 2.3.0** or newer
  from this repository, plus the models your workflow uses.
- Python 3. Optional: `ffmpeg` for LoRA video previews.
- Linux: WebKitGTK 4.1 (Debian 12+, Ubuntu 22.04+, Fedora 39+, openSUSE Tumbleweed / Leap 15.6, Arch, NixOS).

## Validation

- Local: 482 Rust tests passed (33 explicitly ignored), 100 frontend tests passed, Svelte check with no
  errors or warnings, interface translations complete in 14 languages.
- Real GPU, ComfyUI 0.34.0, SFW still-life prompts: stopping a run interrupted the executing prompt and
  deleted the queued one from the ComfyUI queue.
- Native app check with a simulated ComfyUI: a full six-pass run, a casting batch of eight, keeping images in
  the gallery and restarting the app on that data.
- Not verified end to end: a resume after actually killing ComfyUI. The decision logic (refusal while a pass
  runs, confirmation of a lost pass) is covered by unit tests.

## Known limitations

- **Casting needs a draw engine**, which is not distributed; without one, Casting says so and stays unavailable.
- **Flatpak** runs sandboxed: ComfyUI on `127.0.0.1` and your home folder are allowed, but the browser
  extension bridge may not work there.
- Snap is built by CI when the Snapcraft store is reachable; it is attached only if that build succeeds.
- The bundled preset ships with **neutral example prompts and empty LoRA stacks**: fill in your own. Its model
  settings are mine; keep the ones that work on your machine.

## Also in this repository

The **Saya Couple 2.3.0** node pack is unchanged. See
[CHANGELOG.md](https://github.com/alphaziod/saya-studio-x-couple/blob/main/CHANGELOG.md).

## Documentation

[README](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/README.md) ·
[Packaging](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/docs/PACKAGING.md) ·
[For AI agents and contributors](https://github.com/alphaziod/saya-studio-x-couple/blob/main/studio/AGENTS.md)

License: GPL-3.0-or-later.
