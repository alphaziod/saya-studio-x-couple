"""Offline checks of the Saya Studio part of the installer: python3 tests/test_studio_app.py"""
import os
import sys

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.join(os.path.dirname(os.path.abspath(__file__)), "..", "installer"))
from saya_installer import studio_app as s  # noqa: E402

assert s.detect_format({"ID": "linuxmint", "ID_LIKE": "ubuntu debian"}) == "deb"
assert s.detect_format({"ID": "fedora"}) == "fedora"
assert s.detect_format({"ID": "opensuse-tumbleweed", "ID_LIKE": "opensuse suse"}) == "opensuse"
assert s.detect_format({"ID": "endeavouros", "ID_LIKE": "arch"}) == "arch"
assert s.detect_format({"ID": "nixos"}) == "nix"
assert s.detect_format({"ID": "void"}) == "appimage"
assert s.asset_matches("saya-studio-1.0.0-beta.1.fedora.x86_64.rpm", "fedora")
assert not s.asset_matches("saya-studio-1.0.0-beta.1.opensuse.x86_64.rpm", "fedora")
assert s.expected_sha256("ab12  a.deb\ncd34 *b.rpm\n", "b.rpm") == "cd34"
assert s.expected_sha256("ab12  a.deb\n", "missing.deb") is None
releases = [{"draft": True, "tag_name": "studio-v9"}, {"draft": False, "tag_name": "v2.3.0"},
            {"draft": False, "prerelease": True, "tag_name": "studio-v1.0.0-beta.1"}]
assert s.latest_release(releases)["tag_name"] == "studio-v1.0.0-beta.1", "drafts and node releases skipped"
assert s.install_command("arch", "/tmp/x.pkg.tar.zst")[:3] == ["sudo", "pacman", "-U"]

# ---- update mode (no network: fetchers, runners and installers are injected)
import hashlib

pv = s.parse_version
assert pv("1.0.0") < pv("1.0.1") and pv("1.0.0-beta.2") < pv("1.0.0") and pv("1.0.0beta2") == pv("1.0.0-beta.2")
assert pv("studio-v1.0.1") == pv("1.0.1-1") and pv("1.0.0~beta.2-1") == pv("1.0.0-beta.2") and pv("1.0.0-beta.1") < pv("1.0.0-beta.2")
assert pv("garbage") is None
assert s.is_local_build("1.0.0.dev.wt202611042337-1") and not s.is_local_build("1.0.0-beta.2")

OSR = {
    "arch": {"ID": "cachyos", "ID_LIKE": "arch"}, "endeavouros": {"ID": "endeavouros", "ID_LIKE": "arch"},
    "manjaro": {"ID": "manjaro", "ID_LIKE": "arch"}, "ubuntu": {"ID": "ubuntu", "ID_LIKE": "debian"},
    "mint": {"ID": "linuxmint", "ID_LIKE": "ubuntu debian"}, "fedora": {"ID": "fedora"},
    "suse": {"ID": "opensuse-leap", "ID_LIKE": "suse opensuse"},
}
for key, fmt in [("arch", "arch"), ("endeavouros", "arch"), ("manjaro", "arch"), ("ubuntu", "deb"),
                 ("mint", "deb"), ("fedora", "fedora"), ("suse", "opensuse")]:
    assert s.detect_format(OSR[key]) == fmt, key
# os-release parsing from a sample file
import tempfile
with tempfile.TemporaryDirectory() as d:
    f = os.path.join(d, "os-release")
    open(f, "w").write('NAME="Fedora Linux"\nID=fedora\nPRETTY_NAME="Fedora Linux 42"\n')
    assert s.os_release(f)["ID"] == "fedora"

DEB = "saya-studio_1.0.1_amd64.deb"
PKG_BYTES = b"package-bytes"
BLOBS = {"u/deb": PKG_BYTES, "u/sums": (hashlib.sha256(PKG_BYTES).hexdigest() + "  " + DEB + "\n").encode()}


def release(tag="studio-v1.0.1", sums=True):
    assets = [{"name": DEB, "size": 5 << 20, "browser_download_url": "u/deb"}]
    if sums:
        assets.append({"name": "SHA256SUMS", "browser_download_url": "u/sums"})
    return {"tag_name": tag, "assets": assets}


def go(installed, *, rel=None, blobs=None, fmt="deb", machine="x86_64", dry=False, force=False, yes=True):
    out, got, calls = [], [], []
    blobs = BLOBS if blobs is None else blobs
    code = s.update(out.append, lambda q, d, a: yes, True, dry, fmt, force,
                    fetch=lambda url: rel or release(), get=lambda u: (got.append(u), blobs[u])[1],
                    run=lambda cmd: installed, call=lambda c: (calls.append(c), 0)[1],
                    osrel=OSR["ubuntu"], machine=machine)
    return code, "\n".join(out), got, calls


code, text, got, calls = go("1.0.1")
assert code == 0 and "up to date" in text and not got and not calls
code, text, got, calls = go("1.0.0")
assert code == 0 and "1.0.0 -> 1.0.1" in text and calls and calls[0][:3] == ["sudo", "apt", "install"]
code, text, got, calls = go("1.0.0-beta.2")
assert code == 0 and calls, "beta -> final is an update"
code, text, got, calls = go("1.0.0.dev.wt202611042337-1")
assert code == 1 and "local build" in text and "--force-update" in text and not got and not calls
code, text, got, calls = go("1.0.0.dev.wt202611042337-1", force=True)
assert code == 0 and calls
code, text, got, calls = go("1.0.0", blobs={"u/deb": b"tampered", "u/sums": BLOBS["u/sums"]})
assert code == 1 and "mismatch" in text and not calls
code, text, got, calls = go("1.0.0", rel=release(sums=False))
assert code == 1 and "SHA256SUMS" in text and not calls
code, text, got, calls = go("1.0.0", dry=True)
assert code == 0 and "--dry-run" in text and not got and not calls
code, text, got, calls = go("1.0.0", yes=False)
assert code == 0 and "Nothing changed" in text and not got and not calls
code, text, got, calls = go("1.0.0", machine="aarch64")
assert code == 1 and "Unsupported architecture" in text and not calls
code, text, got, calls = go(None)
assert code == 0 and "not found" in text and calls, "not installed: offers the package"
# version readers
seen = []
assert s.installed_version("arch", lambda c: (seen.append(c), "saya-studio 1.0.0beta2-1")[1]) == "1.0.0beta2-1" and seen[0][:2] == ["pacman", "-Q"]
assert s.installed_version("deb", lambda c: "1.0.0") == "1.0.0"
assert s.installed_version("fedora", lambda c: "1.0.0-1") == "1.0.0-1"
assert s.installed_version("appimage", lambda c: None, "/x/Saya Studio_1.0.0_amd64.AppImage") == "1.0.0"
assert s.installed_version("appimage", lambda c: None) is None
print("studio_app: OK")
