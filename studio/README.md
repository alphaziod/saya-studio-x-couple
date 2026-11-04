# Saya Studio

![Saya Studio: the six-pass generation](../docs/images/studio-generation.webp)

| Casting | Gallery |
|:---:|:---:|
| ![Casting grid](../docs/images/studio-casting.webp) | ![Gallery](../docs/images/studio-gallery.webp) |

**1.0.0 — first stable release.** A local desktop app (Tauri 2, Rust, Svelte 5, SQLite) that drives
[ComfyUI](https://github.com/comfyanonymous/ComfyUI) for anime-style character images. Nothing runs in the
cloud: Saya Studio is the interface, ComfyUI is the render engine.

It is the companion app of the **Saya Couple** node pack in this repository, and like the pack it is first a
backup of my own setup, published in case it helps someone. Back up your data, and use it at your own risk.

## New in 1.0.0

Reliability first. A six-pass run or a kept casting finish interrupted by a ComfyUI crash, a failed pass or a
stop resumes from the unfinished pass with the same seed (**Resume from pass N**). Stop really removes queued
passes, restart waits for the running pass, and image cleanup only ever erases Studio's own files, with a journal.

See the [complete 1.0.0 changes and validation](RELEASE_NOTES.md#whats-new-in-100).

## What it does

- **Generation**: open a ComfyUI workflow (UI format), check its faithful API conversion, edit inputs, submit,
  follow progress. The bundled **Saya preset** chains six passes (composition, hires fix, HiDream, post,
  detailers, final upscale) with memory released between passes.
- **Casting**: asks a draw engine for N characters, renders pass 1 only, lets you keep some and send them
  through the six passes. Action drafts can be tested in an isolated copy of the engine before they are added
  for real. *The draw engine itself is not distributed*; without one, Casting stays unavailable and says so.
- **Gallery**: indexes your image folders (incremental, SHA-256, ComfyUI PNG metadata), filters by checkpoint,
  LoRA and character, favourites, review, duplicates; files deliveries by day and family.
- **Characters**: sheets (identity prompt + anatomy), variants with backgrounds, reference images, import.
- **Models**: LoRA manager (send to the generation, stack, right-click actions), checkpoints.
- **Workshop**: graph editor (native ComfyUI graphs, subgraphs), workflows, pipelines, A/B lab, scenes.
- 14 interface languages (French by default).

## Requirements

- Linux x86_64 with WebKitGTK 4.1 (Debian 12+, Ubuntu 22.04+, Fedora 39+, openSUSE Tumbleweed / Leap 15.6,
  Arch / EndeavourOS, NixOS).
- Windows 10/11 x64 with WebView2, or macOS 11+ (Apple Silicon or Intel).
- A running ComfyUI (default `~/AI/ComfyUI`, listening on `127.0.0.1:8188`) with the **Saya Couple** nodes
  2.3.0 or newer from this repository, and the models your workflow needs.
- Python 3 (sidecars). Optional: `ffmpeg` for LoRA video previews.

## Install

Packages are attached to the GitHub release `studio-v1.0.0`:

Snap is attached only when its CI build can reach the Snapcraft store. Its build recipe is in `packaging/snap/snapcraft.yaml`.

| Distribution | File / command |
|---|---|
| Windows 10/11 x64 | `saya-studio-1.0.0-windows-x64.exe` |
| macOS Apple Silicon | `saya-studio-1.0.0-macos-arm64.dmg` |
| macOS Intel | `saya-studio-1.0.0-macos-x64.dmg` |
| Debian, Ubuntu | `sudo apt install ./saya-studio_1.0.0_amd64.deb` |
| Fedora | `sudo dnf install ./saya-studio-1.0.0.fedora.x86_64.rpm` |
| openSUSE | `sudo zypper install ./saya-studio-1.0.0.opensuse.x86_64.rpm` |
| Arch, EndeavourOS | `sudo pacman -U ./saya-studio-1.0.0-x86_64.pkg.tar.zst`, or AUR `saya-studio-bin` |
| NixOS / Nix | `nix run github:alphaziod/saya-studio-x-couple?dir=studio` |
| Any distribution | `saya-studio_1.0.0_amd64.AppImage`, `saya-studio-1.0.0-x86_64.flatpak` |

Release notes: [RELEASE_NOTES.md](RELEASE_NOTES.md). Build steps and per-distribution dependencies: [docs/PACKAGING.md](docs/PACKAGING.md).
French install guide for Arch: [docs/INSTALLATION-ARCH.md](docs/INSTALLATION-ARCH.md).

## Build from source

```sh
cd studio
npm ci
npm run tauri -- dev                                   # development window
npm run build && cargo build -p saya-studio --features custom-protocol
./target/debug/saya-studio
```

Tests: `cargo test -p studio-core`, `node --test scripts/*.test.mjs`, `npx svelte-check`
([docs/TESTING.md](docs/TESTING.md)).

## Your data

- App data: `~/.local/share/studio.saya.desktop/` on Linux, `%APPDATA%/studio.saya.desktop/` on Windows, `~/Library/Application Support/studio.saya.desktop/` on macOS (SQLite database, thumbnails, run manifests).
- Lasting settings and your own presets: `~/Documents/saya studio config/` (copy it to move your setup).
- Delivered images: `~/Images/saya studio/` (changeable in Settings).
- Backups: `saya-studio-maintenance backup|verify|restore` ([docs/SAUVEGARDES.md](docs/SAUVEGARDES.md)).

The app never deletes your originals; temporary previews are cleaned by rule.

The gallery uses each system's recoverable trash (Linux, Windows Recycle Bin, macOS Trash).
On Windows and macOS, launching Studio registers its native host for Chrome, Chromium, Brave and Edge.
The extension connects to the running app without manually copying its token.

## Known limitations

- Tested day to day on Arch (Wayland/Niri) with an AMD GPU. Windows and macOS packages have CI installation/startup checks, not a full manual UI or GPU validation.
- Windows packages are unsigned; macOS packages use ad-hoc signing and are not Apple-notarized. OS security checks can block the first launch.
- Legacy `.bash` engine launchers require Bash; Python test scripts use `python` on Windows and `python3` elsewhere (`SAYA_PYTHON` overrides this).
- AppImage: presets and helpers are found next to the binary (`../share/saya-studio`); not run by hand yet.
- Flatpak and Snap run sandboxed: access to ComfyUI and your folders goes through the permissions listed in
  their manifests; the browser extension bridge may not work there.
- The bundled preset ships with neutral SFW example prompts and empty LoRA stacks: fill in your own.

## For AI agents and contributors

Start with [AGENTS.md](AGENTS.md): build from an empty machine, repository map, rules that protect the user's
data, how-tos (workflow conversion, pipelines, casting, characters, backgrounds), a first-contact questionnaire,
and optional prompt skills in `agent-skills/`.

## License

GPL-3.0-or-later, like the rest of this repository (see `../LICENSE`).
