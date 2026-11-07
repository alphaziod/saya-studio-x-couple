#!/usr/bin/env bash
# Casting names, duo and delivered files in the real app window: stub engine with a synthetic SFW
# duo module, mock ComfyUI writing like a real Saya run. Prior build: npm run build && cargo build
# -p saya-studio --features custom-protocol,smoke-test. OBJECT_INFO: /object_info capture so the
# Saya preset finds its models. No GPU, no image is opened: the checks read the DOM, IPC and disk.
set -euo pipefail
root="$(pwd)"
base="$root/test-results/casting-naming"
engine="$base/engine-stub"
rm -rf "$engine" && mkdir -p "$engine"
cp "$root/scripts/casting-stub-engine.py" "$engine/test_solo.py"
export SAYA_STUDIO_PRESETS="$root/presets" SAYA_ENGINE_DIR="$engine"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$engine"
cat > "$engine/moteur/duo.py" <<'PY'
POPULATIONS = {"futa_x_futa": "a", "girl_x_girl": "b", "futa_x_girl": "c"}
def tirer_duo(famille, seed, index_action=None):
    return {"stem": "duo-fixture", "name": "Alba Test x Rhea Test", "famille": famille, "model": "noobai", "seed": seed,
            "action_name": "duo_debout", "note": "duo/" + famille, "pose": "standing side by side",
            "p1": "1girl, adult, long black hair", "p1_anatomy": "", "p2": "1girl, adult, short white hair, duo-fixture-p2", "p2_anatomy": ""}
PY
cat > "$engine/moteur/comfy.py" <<'PY'
NOOBAI = "NoobAI/no tags/MKLAN POISONED NOOB AI FUTA_00001_.safetensors"
def casting_passe_un(shot):
    return {"prefixe": "masterpiece", "decor_libre": "park, daylight", "action": shot["pose"], "p1": shot["p1"],
            "anatomy": shot["anatomy"], "negative": "", "p2": "", "p2_anatomy": "", "checkpoint": NOOBAI,
            "lora_loaders": {str(n): {"loras": [], "text": ""} for n in (662, 663, 664)}, "format_paysage": False, "seed": -1}
PY
comfy_output="$base/comfy-output"
rm -rf "$comfy_output" && mkdir -p "$comfy_output"
export SAYA_COMFY_OUTPUT_DIR="$comfy_output" MOCK_COMFY_OUTPUT_DIR="$comfy_output"
extra=()
[ -n "${OBJECT_INFO:-}" ] && extra=(--mock-object-info "$OBJECT_INFO")
python3 scripts/v12-native-smoke.py "${extra[@]}" \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/casting-naming-native-smoke.js" \
  --root "$base" \
  --capture-root \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept --timeout 600
run="$(ls -1dt "$base"/*/ | grep -v 'engine-stub\|comfy-output' | head -1)"
python3 -I - "$run" "$comfy_output" <<'PY'
import json, re, sys
from pathlib import Path
run, output = Path(sys.argv[1]), Path(sys.argv[2])
files = sorted(p.relative_to(run / 'captures') for p in (run / 'captures').rglob('*.png'))
print('captures:', *files, sep='\n  ')
names = [p.name for p in files]
assert names, 'no delivered image'
# Two duo trials share name, pose and day: only they may carry a -2, and exactly once each.
assert not [n for n in names if re.search(r'-\d+\.png$', n) and 'Alba-Test' not in n], 'second copy delivered'
assert len([n for n in names if 'Alba-Test' in n and n.endswith(('_p1.png', '_p1-2.png'))]) == 2, 'one image per duo trial'
day = r'\d{4}-\d{2}-\d{2}'
finals = [p for p in files if 'Final' in p.parts]
assert any(re.fullmatch(day + r'_Essai-Perso_stub-debout\.png', p.name) for p in finals), 'final name'
assert all(p.name.count('finale') == 0 for p in finals), 'final word left'
assert any(re.fullmatch(day + r'_[^_]+_stub-debout_p1\.png', n) for n in names), 'trial name'
assert any(re.fullmatch(day + r'_Alba-Test-x-Rhea-Test_duo-debout_p1\.png', n) for n in names), 'duo name'
copies = [p for p in output.rglob('*.png') if 'image' in p.relative_to(output).parts]
assert not copies, f'ComfyUI copies left: {copies}'
assert list(output.rglob('phase_1_base.candidate.png')), 'phase state removed'
lines = [json.loads(l) for l in (run / 'mock-journal.jsonl').read_text().splitlines()]
prompts = [l['body']['prompt'] for l in lines if l['method'] == 'POST' and l['path'] == '/prompt']
assert any(l['path'] == '/saya/image-phases/validate' for l in lines), 'review decision lost'
assert not [l for l in lines if l['method'] == 'GET' and l['path'] == '/view' and 'review_' in json.dumps(l)], 'review downloaded'
duo = [p for p in prompts if 'duo-fixture-p2' in json.dumps(p)]
assert duo, 'P2 never reached ComfyUI'
def solo_value(prompt, node):
    # The real preset wires `solo` to StringCompare(Couple Mode == 'OFF').
    value = node['inputs'].get('solo')
    if not isinstance(value, list):
        return value
    compare = prompt[value[0]]['inputs']
    return prompt[compare['string_a'][0]]['inputs']['choice'] == compare['string_b']
couple = [(p, n) for p in duo for n in p.values() if n.get('class_type') == 'SayaMultiCouple']
assert couple and all(solo_value(p, n) is False for p, n in couple), f'couple node not live: {len(couple)}'
log = (run / 'mock-journal.jsonl').stat().st_size
print(f'OK: {len(files)} images, {len(prompts)} prompts, {len(duo)} duo prompts, journal {log} bytes')
PY
