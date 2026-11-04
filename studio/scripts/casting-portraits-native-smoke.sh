#!/usr/bin/env bash
# « Images manquantes » in the real app window: stub engine with three synthetic SFW characters
# that have no picture, mock ComfyUI. Walks No (another pose), Yes (six passes, image kept and
# shown in the picker), Skip and Quit. Prior build: npm run build && cargo build -p saya-studio
# --features custom-protocol,smoke-test. OBJECT_INFO: optional /object_info capture so the Saya
# preset finds its models (otherwise the missing-models dialog stops the launch).
set -euo pipefail
root="$(pwd)"
engine="$root/test-results/casting-portraits/engine-stub"
rm -rf "$engine" && mkdir -p "$engine"
cp "$root/scripts/casting-stub-engine.py" "$engine/test_solo.py"
python3 - "$engine/negatifs_figes.json" <<'PY'
import json, sys
rows = [{"neg_id": f"CH--92{i:02d}", "nom": name, "slug": name.lower(), "model": "noobai", "famille": "hum", "source": "oc"}
        for i, name in ((1, "Stub Prêtresse"), (2, "Stub Archiviste"), (3, "Stub Jardinière"))]
json.dump({"valides": rows}, open(sys.argv[1], "w"), ensure_ascii=False)
PY
export SAYA_STUDIO_PRESETS="$root/presets" SAYA_ENGINE_DIR="$engine"
python3 -B "$root/scripts/casting-capabilities-fixture.py" "$SAYA_ENGINE_DIR"
extra=()
[ -n "${OBJECT_INFO:-}" ] && extra=(--mock-object-info "$OBJECT_INFO")
python3 scripts/v12-native-smoke.py "${extra[@]}" \
  --binary "$root/target/debug/saya-studio" \
  --script "$root/scripts/casting-portraits-native-smoke.js" \
  --root "$root/test-results/casting-portraits" \
  --ui-workflow "$root/presets/passes/ILUSTCOUPLE_00_MAITRE_CONFIG.json" \
  --mock-comfy --mock-accept --timeout 600
