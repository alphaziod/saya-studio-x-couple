"""Build a standalone Couple archive and a checksum-pinned Linux installer for a Studio release.

python3 tools/build_release_bundle.py OUTPUT_DIRECTORY --tag studio-v1.0.0
Only the public, Git-tracked node/installer payload is included; no local app data or builds.
"""
import argparse
import gzip
import hashlib
import io
import json
from pathlib import Path
import re
import subprocess
import tarfile

from build_manifest import classify, TYPES

ROOT = Path(__file__).resolve().parent.parent
PATHS = ['files', 'installer', 'patches', 'workflows', 'LICENSE', 'LICENSES',
         'THIRD_PARTY_NOTICES.md', 'compatibility.json', 'saya', 'saya.bat']
LAUNCHER = '''#!/bin/sh
# Saya Couple nodes + Saya Studio app. No root required to start this installer.
# Run: sh install.sh   (choose nodes, app, or both)
# Or:  sh install.sh --what both --comfyui /path/to/ComfyUI
# Preview app install: sh install.sh --what app --dry-run
set -eu
case "${1:-}" in
  install|studio|verify|restore|check-compat|amd|-h|--help) ;;
  *) set -- install "$@" ;;
esac
for tool in curl tar sha256sum python3; do
  command -v "$tool" >/dev/null 2>&1 || { echo "Missing prerequisite: $tool" >&2; exit 1; }
done
work=$(mktemp -d "${TMPDIR:-/tmp}/saya-install.XXXXXXXX")
trap 'rm -rf "$work"' 0
trap 'exit 130' INT
trap 'exit 143' TERM
printf '%s\\n' 'Downloading Saya Couple @VERSION@ and its installer...'
curl --fail --location --retry 3 --connect-timeout 15 --max-time 180 \
  --output "$work/payload.tar.gz" \
  'https://github.com/alphaziod/saya-studio-x-couple/releases/download/@TAG@/@ARCHIVE@'
printf '%s  %s\\n' '@SHA256@' "$work/payload.tar.gz" | sha256sum --check -
tar -xzf "$work/payload.tar.gz" -C "$work"
PYTHONDONTWRITEBYTECODE=1 python3 "$work/@PREFIX@/installer/saya.py" "$@"
'''


def build(output: Path, tag: str):
    if not re.fullmatch(r'studio-v[0-9A-Za-z._-]+', tag):
        raise ValueError('Expected a studio-v* release tag')
    version = (ROOT / 'files/custom_nodes/Saya_Couple/VERSION.txt').read_text().strip()
    if not re.fullmatch(r'[0-9A-Za-z._-]+', version):
        raise ValueError('Invalid node version')
    prefix = f'saya-couple-{version}'
    paths = subprocess.check_output(['git', 'ls-files', '-z', '--', *PATHS], cwd=ROOT).decode().split('\0')
    payload = {}
    entries = []
    for rel in sorted(p for p in paths if p):
        p = ROOT / rel
        if p.is_symlink() or not p.is_file():
            raise ValueError(f'Not a regular tracked file: {rel}')
        data = p.read_bytes()
        payload[rel] = (data, 0o755 if p.stat().st_mode & 0o111 else 0o644)
        entry = {'path': rel, 'type': 'script' if rel == 'saya' else TYPES.get(p.suffix, p.suffix.lstrip('.') or 'file'),
                 'sha256': hashlib.sha256(data).hexdigest()}
        entry.update(classify(rel))
        entries.append(entry)
    payload['INSTALL.txt'] = (b'Run ./saya install to choose Couple nodes, Studio app, or both.\n'
        b'Explicit choice: ./saya install --what both --comfyui /path/to/ComfyUI\n'
        b'Preview app: ./saya studio --dry-run\n'
        b'Existing installer confirmations, backups and restore are preserved.\n'
        b'The app package is downloaded from the latest Studio release and checksum-checked.\n', 0o644)
    data = payload['INSTALL.txt'][0]
    entries.append(dict(path='INSTALL.txt', type='text', sha256=hashlib.sha256(data).hexdigest(), **classify('INSTALL.txt')))
    original = json.loads((ROOT / 'MANIFEST.json').read_text())
    manifest = {'schema': 1, 'saya_version': version, 'tested_upstream': original['tested_upstream'], 'files': entries}
    payload['MANIFEST.json'] = ((json.dumps(manifest, indent=1, ensure_ascii=False) + '\n').encode(), 0o644)
    output.mkdir(parents=True, exist_ok=True)
    archive = output / f'{prefix}.tar.gz'
    with archive.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as gz:
        with tarfile.open(fileobj=gz, mode='w') as tar:
            for rel, (data, mode) in sorted(payload.items()):
                info = tarfile.TarInfo(f'{prefix}/{rel}')
                info.size, info.mode, info.mtime = len(data), mode, 0
                tar.addfile(info, io.BytesIO(data))
    script = LAUNCHER
    for key, value in {'VERSION': version, 'TAG': tag, 'ARCHIVE': archive.name, 'PREFIX': prefix,
                       'SHA256': hashlib.sha256(archive.read_bytes()).hexdigest()}.items():
        script = script.replace(f'@{key}@', value)
    installer = output / 'install.sh'
    installer.write_text(script)
    installer.chmod(0o755)
    print(f'{archive.name}: {len(entries)} payload files; {archive.stat().st_size} bytes')
    print(installer)
    return archive, installer


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('output', type=Path)
    parser.add_argument('--tag', required=True)
    args = parser.parse_args()
    build(args.output, args.tag)
