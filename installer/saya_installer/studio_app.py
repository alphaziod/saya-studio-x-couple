"""Saya Studio (desktop app) installer: picks the package for this Linux distribution from the latest
`studio-v*` GitHub release, checks it against the release's SHA256SUMS, shows the exact command and
installs it only after the user says yes. Standard library only."""

from __future__ import annotations

import hashlib
import json
import os
import platform
import re
import shutil
import subprocess
import sys
import tempfile
import urllib.request

REPO = os.environ.get("SAYA_REPO", "alphaziod/saya-studio-x-couple")
TAG_PREFIX = "studio-v"
FORMATS = ("deb", "fedora", "opensuse", "arch", "appimage", "flatpak", "nix")
PACKAGE = "saya-studio"
SUPPORTED_ARCH = ("x86_64", "amd64")
FAMILIES = {"deb": "Debian / Ubuntu / Mint", "fedora": "Fedora / RHEL", "opensuse": "openSUSE / SUSE",
            "arch": "Arch / EndeavourOS / Manjaro", "nix": "NixOS", "appimage": "other distribution"}


def os_release(path: str = "/etc/os-release") -> dict[str, str]:
    data: dict[str, str] = {}
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                if "=" in line:
                    key, value = line.rstrip("\n").split("=", 1)
                    data[key] = value.strip().strip('"')
    except OSError:
        pass
    return data


def detect_format(release: dict[str, str]) -> str:
    """Package format for this distribution: ID first, then ID_LIKE; AppImage when unknown."""
    ids = [release.get("ID", "")] + release.get("ID_LIKE", "").split()
    for name in ids:
        if name == "nixos":
            return "nix"
        if name in ("arch", "endeavouros", "manjaro", "cachyos", "garuda"):
            return "arch"
        if name in ("fedora", "rhel", "centos", "rocky", "almalinux", "nobara"):
            return "fedora"
        if name.startswith("opensuse") or name in ("suse", "sles"):
            return "opensuse"
        if name in ("debian", "ubuntu", "linuxmint", "pop", "elementary", "zorin"):
            return "deb"
    return "appimage"


def show_system(say, fmt: str, osrel: dict[str, str] | None = None, machine: str | None = None) -> bool:
    """Print distribution family, architecture and package format. False when the CPU is unsupported."""
    osrel = os_release() if osrel is None else osrel
    machine = (machine or platform.machine()).lower()
    say(f"system         : {osrel.get('PRETTY_NAME') or osrel.get('ID') or 'unknown Linux'}")
    say(f"family         : {FAMILIES.get(detect_format(osrel), 'other distribution')}")
    say(f"architecture   : {machine}")
    say(f"package format : {fmt}" + (" (requested)" if fmt != detect_format(osrel) else ""))
    if machine not in SUPPORTED_ARCH:
        say(f"Unsupported architecture '{machine}': the Saya Studio packages are x86_64 only. Nothing installed.")
        return False
    return True


def asset_matches(name: str, fmt: str) -> bool:
    return {
        "deb": name.endswith("_amd64.deb"),
        "fedora": name.endswith(".fedora.x86_64.rpm"),
        "opensuse": name.endswith(".opensuse.x86_64.rpm"),
        "arch": name.endswith(".pkg.tar.zst"),
        "appimage": name.endswith(".AppImage"),
        "flatpak": name.endswith(".flatpak"),
    }.get(fmt, False)


def install_command(fmt: str, path: str) -> list[str]:
    """The exact command run for a downloaded package (system packages go through sudo)."""
    return {
        "deb": ["sudo", "apt", "install", "-y", path],
        "fedora": ["sudo", "dnf", "install", "-y", path],
        "opensuse": ["sudo", "zypper", "--non-interactive", "install", "--allow-unsigned-rpm", path],
        "arch": ["sudo", "pacman", "-U", "--noconfirm", path],
        "flatpak": ["flatpak", "install", "--user", "-y", path],
    }[fmt]


def _get(url: str) -> bytes:
    req = urllib.request.Request(url, headers={"User-Agent": "saya-installer", "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req, timeout=60) as r:
        return r.read()


def latest_release(releases: list[dict] | None = None) -> dict:
    """Newest published (not draft) Saya Studio release; pre-releases count, it is a beta."""
    if releases is None:
        releases = json.loads(_get(f"https://api.github.com/repos/{REPO}/releases?per_page=30"))
    for rel in releases:
        if not rel.get("draft") and rel.get("tag_name", "").startswith(TAG_PREFIX):
            return rel
    raise RuntimeError(f"no published Saya Studio release ({TAG_PREFIX}*) on github.com/{REPO}")


def expected_sha256(sums_text: str, name: str) -> str | None:
    for line in sums_text.splitlines():
        parts = line.split()
        if len(parts) == 2 and parts[1].lstrip("*") == name:
            return parts[0].lower()
    return None


def install(say, ask, assume: bool | None, dry_run: bool, fmt: str | None = None, system_shown: bool = False) -> int:
    if not sys.platform.startswith("linux"):
        say("Saya Studio is a Linux app for now: nothing to install on this system.")
        return 1
    say("=" * 64 + "\nSaya Studio (desktop app)\n" + "=" * 64)
    fmt = fmt or detect_format(os_release())
    if not system_shown and not show_system(say, fmt):
        return 1
    if fmt == "nix":
        cmd = f"nix profile install github:{REPO}?dir=studio"
        say(f"On NixOS / Nix, install it with:\n  {cmd}\n(or try it first: nix run github:{REPO}?dir=studio)")
        return 0
    try:
        rel = latest_release()
    except Exception as e:  # network or GitHub API error: say it, change nothing
        say(f"Could not read the releases of github.com/{REPO}: {e}")
        return 1
    assets = rel.get("assets", [])
    pkg = next((a for a in assets if asset_matches(a["name"], fmt)), None)
    sums = next((a for a in assets if a["name"] == "SHA256SUMS"), None)
    if pkg is None:
        say(f"Release {rel['tag_name']} has no {fmt} package. Packages: {', '.join(a['name'] for a in assets)}")
        return 1
    say(f"release        : {rel['tag_name']}{' (pre-release)' if rel.get('prerelease') else ''}")
    say(f"package        : {pkg['name']} ({pkg.get('size', 0) // (1024 * 1024)} MiB)")
    if fmt == "appimage":
        target = os.path.expanduser("~/.local/bin/saya-studio.AppImage")
        say(f"will be saved  : {target} (no root rights needed)")
    else:
        say(f"will run       : {' '.join(install_command(fmt, '<downloaded package>'))}")
    if dry_run:
        say("\n--dry-run: nothing downloaded, nothing installed.")
        return 0
    if not ask("Download and install Saya Studio now?", True, assume):
        say("Nothing installed.")
        return 0
    tmp = tempfile.mkdtemp(prefix="saya-studio-")
    try:
        data = _get(pkg["browser_download_url"])
        if sums is not None:
            want = expected_sha256(_get(sums["browser_download_url"]).decode(), pkg["name"])
            got = hashlib.sha256(data).hexdigest()
            if want is None or want != got:
                say(f"SHA256 mismatch for {pkg['name']} (expected {want}, got {got}): nothing installed.")
                return 1
            say("SHA256         : OK")
        else:
            say("SHA256         : the release has no SHA256SUMS, file not verified")
        # The name comes from the GitHub API: keep a plain file name inside the temporary folder.
        name = os.path.basename(pkg["name"])
        if not name or name != pkg["name"] or not asset_matches(name, fmt):
            say(f"Unexpected package name {pkg['name']!r}: nothing installed.")
            return 1
        path = os.path.join(tmp, name)
        with open(path, "wb") as f:
            f.write(data)
        if fmt == "appimage":
            os.makedirs(os.path.dirname(target), exist_ok=True)
            shutil.copyfile(path, target)
            os.chmod(target, 0o755)
            say(f"Installed: {target}\nRun it from there, or add ~/.local/bin to your PATH.")
            return 0
        cmd = install_command(fmt, path)
        say("$ " + " ".join(cmd))
        code = subprocess.call(cmd)
        say("Saya Studio installed." if code == 0 else f"The package manager stopped with code {code}.")
        return code
    finally:
        shutil.rmtree(tmp, ignore_errors=True)


# ---------------------------------------------------------------- update mode

API_LATEST = f"https://api.github.com/repos/{REPO}/releases/latest"
_VERSION = re.compile(r"^(\d+(?:\.\d+)*)(?:[-~.]?(alpha|beta|rc|a|b)[.]?(\d*))?$")
_PRE = {"a": 0, "alpha": 0, "b": 1, "beta": 1, "rc": 2}


def _fetch_json(url: str) -> dict:
    req = urllib.request.Request(url, headers={"User-Agent": "saya-installer", "Accept": "application/vnd.github+json"})
    with urllib.request.urlopen(req, timeout=15) as r:
        return json.loads(r.read())


def is_local_build(version: str) -> bool:
    return bool(re.search(r"dev|\.wt\d|\+|git", version.lower()))


def parse_version(text: str):
    """Sortable key for 1.0.0, 1.0.1, 1.0.0-beta.2, 1.0.0beta2, 1.0.0~beta.2-1; None if not understood.
    The trailing package release (-1) is ignored; a final release sorts after its pre-releases."""
    v = text.strip().lower().split(":")[-1].removeprefix("studio-").removeprefix("v")
    v = re.sub(r"-\d+$", "", v)
    m = _VERSION.match(v)
    if not m:
        return None
    nums = [int(x) for x in m.group(1).split(".")]
    while len(nums) > 3 and nums[-1] == 0:
        nums.pop()
    nums += [0] * (3 - len(nums))
    pre = (1, 0, 0) if not m.group(2) else (0, _PRE[m.group(2)], int(m.group(3) or 0))
    return (tuple(nums), pre)


def _run_out(cmd: list[str]) -> str | None:
    try:
        r = subprocess.run(cmd, capture_output=True, text=True, timeout=30)
    except (OSError, subprocess.SubprocessError):
        return None
    return r.stdout.strip() if r.returncode == 0 else None


def installed_version(fmt: str, run=_run_out, appimage: str | None = None) -> str | None:
    """Installed Studio version as reported by the package manager (None when not installed / unknown)."""
    if fmt == "arch":
        out = run(["pacman", "-Q", PACKAGE])
        return out.split()[-1] if out else None
    if fmt == "deb":
        return run(["dpkg-query", "-W", "-f=${Version}", PACKAGE]) or None
    if fmt in ("fedora", "opensuse"):
        return run(["rpm", "-q", "--qf", "%{VERSION}-%{RELEASE}", PACKAGE]) or None
    if fmt == "appimage" and appimage:
        m = re.search(r"(\d+\.\d+\.\d+[0-9A-Za-z.~+-]*?)(?=[_.-](?:amd64|x86_64|linux)|\.AppImage|$)", os.path.basename(appimage))
        return m.group(1) if m else None
    return None


def update(say, ask, assume: bool | None, dry_run: bool, fmt: str | None = None, force: bool = False,
           appimage: str | None = None, *, fetch=_fetch_json, get=_get, run=_run_out, call=subprocess.call,
           osrel: dict[str, str] | None = None, machine: str | None = None, system_shown: bool = False) -> int:
    """Update the Saya Studio app only (not the Couple nodes) from the latest GitHub release."""
    if not sys.platform.startswith("linux"):
        say("Saya Studio is a Linux app for now: nothing to update on this system.")
        return 1
    osrel = os_release() if osrel is None else osrel
    fmt = fmt or detect_format(osrel)
    say("=" * 64 + "\nSaya Studio: update\n" + "=" * 64)
    if not system_shown and not show_system(say, fmt, osrel, machine):
        return 1
    if fmt == "nix":
        say(f"On NixOS / Nix, update with:\n  nix profile upgrade --all   (or reinstall: nix profile install github:{REPO}?dir=studio)")
        return 0
    try:
        rel = fetch(API_LATEST)
    except Exception as e:
        say(f"Could not read the latest release of github.com/{REPO}: {e}")
        return 1
    tag = rel.get("tag_name", "")
    latest_key = parse_version(tag)
    if latest_key is None:
        say(f"Unexpected release tag {tag!r}: nothing changed.")
        return 1
    latest_text = tag.removeprefix(TAG_PREFIX)
    current = installed_version(fmt, run, appimage)
    say(f"latest release : {latest_text}")
    if current is None:
        say("installed      : not found" + (" (give the file with --appimage PATH to compare)" if fmt == "appimage" and not appimage else ""))
    else:
        say(f"installed      : {current}")
        if is_local_build(current):
            say("local build detected (not a release package).")
            if not force:
                say("Refusing to replace it. Use --force-update to replace this local build.")
                return 1
        else:
            cur_key = parse_version(current)
            if cur_key is not None and cur_key >= latest_key:
                say(f"Saya Studio is up to date ({current}).")
                return 0
            if cur_key is not None:
                say(f"update available: {current} -> {latest_text}")
    assets = rel.get("assets", [])
    pkg = next((a for a in assets if asset_matches(a.get("name", ""), fmt)), None)
    sums = next((a for a in assets if a.get("name") == "SHA256SUMS"), None)
    if pkg is None:
        say(f"Release {tag} has no {fmt} package. Packages: {', '.join(a.get('name', '?') for a in assets)}")
        return 1
    name = os.path.basename(pkg["name"])
    if name != pkg["name"] or not asset_matches(name, fmt):
        say(f"Unexpected package name {pkg['name']!r}: nothing changed.")
        return 1
    if sums is None:
        say("The release has no SHA256SUMS: refusing to install an unverified package.")
        return 1
    say(f"package        : {name} ({pkg.get('size', 0) // (1024 * 1024)} MiB)")
    target = os.path.expanduser(appimage or "~/.local/bin/saya-studio.AppImage")
    if fmt == "appimage":
        say(f"will replace   : {target} (no root rights needed)")
    else:
        say(f"will run       : {' '.join(install_command(fmt, '<downloaded package>'))}")
    if dry_run:
        say("\n--dry-run: nothing downloaded, nothing installed.")
        return 0
    if not ask("Download and install this update now?", True, assume):
        say("Nothing changed.")
        return 0
    tmp = tempfile.mkdtemp(prefix="saya-studio-")
    try:
        want = expected_sha256(get(sums["browser_download_url"]).decode(), name)
        data = get(pkg["browser_download_url"])
        got = hashlib.sha256(data).hexdigest()
        if want is None or want != got:
            say(f"SHA256 mismatch for {name} (expected {want}, got {got}): nothing installed.")
            return 1
        say("SHA256         : OK")
        path = os.path.join(tmp, name)
        with open(path, "wb") as f:
            f.write(data)
        if fmt == "appimage":
            os.makedirs(os.path.dirname(target) or ".", exist_ok=True)
            shutil.copyfile(path, target)
            os.chmod(target, 0o755)
            say(f"Updated: {target}")
            return 0
        cmd = install_command(fmt, path)
        say("$ " + " ".join(cmd))
        code = call(cmd)
        say("Saya Studio updated." if code == 0 else f"The package manager stopped with code {code}.")
        return code
    finally:
        shutil.rmtree(tmp, ignore_errors=True)
