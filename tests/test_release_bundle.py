"""Offline checks for the downloadable installer; never installs into ComfyUI or the system."""
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tarfile
import tempfile
from unittest.mock import patch

sys.dont_write_bytecode = True
ROOT = Path(__file__).resolve().parent.parent
sys.path[:0] = [str(ROOT / 'tools'), str(ROOT / 'installer')]
from build_release_bundle import build
from saya_installer.cli import choose_what

with tempfile.TemporaryDirectory(prefix='saya-release-test-') as tmp:
    root = Path(tmp)
    archive, launcher = build(root / 'output', 'studio-v1.0.1')
    first = archive.read_bytes()
    build(root / 'output', 'studio-v1.0.1')
    assert first == archive.read_bytes(), 'archive must be reproducible'
    with tarfile.open(archive) as tar:
        members = tar.getmembers()
        assert all(m.isfile() and not m.name.startswith('/') and '..' not in Path(m.name).parts for m in members)
        prefix = members[0].name.split('/')[0]
        manifest = json.load(tar.extractfile(prefix + '/MANIFEST.json'))
        assert {e['path'] for e in manifest['files']} == {m.name[len(prefix)+1:] for m in members} - {'MANIFEST.json'}
        for entry in manifest['files']:
            data = tar.extractfile(prefix + '/' + entry['path']).read()
            assert hashlib.sha256(data).hexdigest() == entry['sha256'], entry['path']
        assert not any('/studio/' in m.name or 'test-results' in m.name or '.sqlite' in m.name for m in members)
        assert any(e['component'] == 'saya_node' for e in manifest['files'])
    bindir, tempdir = root / 'bin', root / 'scratch'
    bindir.mkdir(); tempdir.mkdir()
    curl = bindir / 'curl'
    curl.write_text('#!/bin/sh\nwhile [ "$#" -gt 0 ]; do\n case "$1" in --output) shift; out=$1;; esac\n shift\ndone\ncp "$TEST_ARCHIVE" "$out"\n')
    py = bindir / 'python3'
    py.write_text('#!/bin/sh\nprintf "%s\\n" "$@" > "$TEST_ARGS"\n')
    curl.chmod(0o755); py.chmod(0o755)
    argsfile = root / 'args'
    env = dict(os.environ, PATH=str(bindir)+os.pathsep+os.environ['PATH'], TMPDIR=str(tempdir),
               TEST_ARCHIVE=str(archive), TEST_ARGS=str(argsfile))
    for args, expected in [([], ['install']), (['--what','both','--comfyui','/example/Comfy UI'],
        ['install','--what','both','--comfyui','/example/Comfy UI']), (['studio','--dry-run'],['studio','--dry-run']),
        (['--update'],['update']), (['--update','--dry-run','--force-update'],['update','--dry-run','--force-update']),
        (['update','--yes'],['update','--yes'])]:
        result = subprocess.run(['sh', str(launcher), *args], env=env, capture_output=True, text=True)
        assert result.returncode == 0, result.stderr
        forwarded = argsfile.read_text().splitlines()
        assert forwarded[0].endswith('/installer/saya.py') and forwarded[1:] == expected, forwarded
        assert not list(tempdir.iterdir()), 'temporary files must be removed'
    argsfile.unlink()
    archive.write_bytes(b'corrupted download')
    result = subprocess.run(['sh', str(launcher), '--what', 'app'], env=env, capture_output=True, text=True)
    assert result.returncode != 0 and not argsfile.exists(), 'checksum mismatch must prevent execution'
    assert not list(tempdir.iterdir())
    with patch('sys.stdin.isatty', return_value=True):
        for choice, expected in [('1','nodes'), ('2','app'), ('3','both'), ('4','update')]:
            with patch('builtins.input', return_value=choice):
                assert choose_what('ask') == expected
    with patch('sys.stdin.isatty', return_value=False):
        assert choose_what('ask') == 'nodes'
print('release bundle: reproducible, complete manifest, menu routing, quoted arguments, checksum refusal and cleanup OK')
