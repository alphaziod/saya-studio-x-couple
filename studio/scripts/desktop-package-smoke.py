"""Install and start the desktop package on a disposable GitHub runner; no render server."""
import hashlib
import json
import os
from pathlib import Path
import shutil
import sqlite3
import subprocess
import sys
import tempfile
import time

assert os.environ.get("GITHUB_ACTIONS") == "true", "Disposable CI runners only"
platform = sys.argv[1]
source = Path.cwd()
work = Path(tempfile.mkdtemp(prefix="saya desktop smoke "))
artifact = next((source.parent / "dist-release").glob("*.exe" if platform.startswith("windows") else "*.dmg"))
if platform.startswith("windows"):
    install = work / "installed"
    subprocess.run([str(artifact), "/S", f"/D={install}"], check=True, timeout=120)
    executable = install / "saya-studio.exe"
    resources = install
else:
    mount = work / "mount"
    subprocess.run(["hdiutil", "attach", str(artifact), "-nobrowse", "-mountpoint", str(mount)], check=True)
    try:
        bundle = next(mount.glob("*.app"))
        installed = work / bundle.name
        shutil.copytree(bundle, installed, symlinks=True)
    finally:
        subprocess.run(["hdiutil", "detach", str(mount)], check=True)
    executable = installed / "Contents/MacOS/saya-studio"
    resources = installed / "Contents/Resources"

assert executable.is_file(), executable
# Every shipped preset must be byte-identical to its public source.
for original in (source / "presets").rglob("*"):
    if original.is_file():
        packaged = resources / "presets" / original.relative_to(source / "presets")
        assert packaged.is_file(), packaged
        assert hashlib.sha256(packaged.read_bytes()).digest() == hashlib.sha256(original.read_bytes()).digest(), packaged
assert (resources / "sidecars/f09-semantic-embed.py").is_file()
assert (resources / "civitai_bridge.py").is_file()

env = os.environ.copy()
env.update(SAYA_CONFIG_DIR=str(work / "config"), SAYA_COMFYUI_DIR=str(work / "no-comfy"),
           SAYA_ENGINE_DIR=str(work / "no-engine"), SAYA_STUDIO_SANDBOX=str(work))
if platform.startswith("windows"):
    # Exercise ordinary Windows startup, where HOME is not defined.
    env.pop("HOME", None)
    env["USERPROFILE"] = str(work)
    env["APPDATA"] = str(work / "AppData/Roaming")
    env["LOCALAPPDATA"] = str(work / "AppData/Local")
    data = Path(os.environ["APPDATA"]) / "studio.saya.desktop"
else:
    # Cocoa resolves the runner account's home independently of HOME. This runner is disposable.
    data = Path.home() / "Library/Application Support/studio.saya.desktop"
db_path = data / "studio.sqlite3"
assert not db_path.exists(), "Smoke must start with an empty database"
log_path = source.parent / "dist-release" / f"{platform}-startup.log"
with log_path.open("w") as log:
    child = subprocess.Popen([str(executable)], env=env, stdout=log, stderr=log)
    try:
        deadline = time.monotonic() + 45
        initialized = False
        while time.monotonic() < deadline:
            assert child.poll() is None, f"App exited with {child.returncode}; see {log_path}"
            if db_path.is_file():
                try:
                    with sqlite3.connect(db_path) as db:
                        rows = db.execute("SELECT name FROM sqlite_master WHERE type='table'").fetchall()
                        preset = db.execute("SELECT value_json FROM preferences WHERE key='presetBase'").fetchone()
                        expected = json.loads((resources / "presets/preset.json").read_text())["version"]
                        initialized = len(rows) > 20 and preset is not None and json.loads(preset[0]) == expected
                except sqlite3.Error:
                    pass
            if initialized:
                time.sleep(5)
                assert child.poll() is None, "App crashed after database initialization"
                break
            time.sleep(1)
        assert initialized, f"Database not initialized at {db_path}"
    finally:
        child.terminate()
        child.wait(timeout=15)
report = {"platform": platform, "installer": artifact.name, "preset_hashes": "match", "startup": "passed", "database": "initialized", "gpu_run": False}
print(json.dumps(report))
(source.parent / "dist-release" / f"{platform}-smoke.json").write_text(json.dumps(report, indent=2) + "\n")
