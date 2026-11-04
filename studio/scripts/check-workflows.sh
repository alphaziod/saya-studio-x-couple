#!/usr/bin/env bash
# Report which ComfyUI nodes the workflows of a folder need and which ones are missing.
#
#   scripts/check-workflows.sh WORKFLOW_DIR OBJECT_INFO.json
#
# OBJECT_INFO.json is a saved response of ComfyUI's GET /object_info. Prints one JSON report per
# file. Exit code: 0 all complete, 2 missing nodes (or otherwise incomplete), 1 error.
# Set STUDIO_MAINTENANCE to use an already built studio-maintenance binary.
set -euo pipefail

if [ "$#" -ne 2 ]; then
  echo "Usage: $0 WORKFLOW_DIR OBJECT_INFO.json" >&2
  exit 1
fi
dir=$1
info=$2
[ -d "$dir" ] || { echo "Not a folder: $dir" >&2; exit 1; }
[ -f "$info" ] || { echo "No such file: $info" >&2; exit 1; }

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
if [ -n "${STUDIO_MAINTENANCE:-}" ]; then
  bin=$STUDIO_MAINTENANCE
else
  cargo build -q -p studio-core --bin studio-maintenance --manifest-path "$root/Cargo.toml"
  bin=$root/target/debug/studio-maintenance
fi
exec "$bin" check-workflow "$dir" "$info"
