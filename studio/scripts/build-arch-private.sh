#!/usr/bin/env bash
# Local pacman package with your own presets: the COMMITTED sources (HEAD) with presets/ replaced by
# PRESETS_DIR (for example your config folder's workflows/). For your machine only: never publish the result.
#
#   bash scripts/build-arch-private.sh PRESETS_DIR
#
# The version is the PKGBUILD one plus a build stamp (1.1.1.dev.wt<stamp>), so the app's update check
# reports it as a local build and never replaces it without asking. The package is written under
# test-results/arch-package-private-<stamp>-*/stage/.
set -euo pipefail
if [ $# -ne 1 ] || [ ! -d "$1" ]; then
  echo "Usage: $0 PRESETS_DIR" >&2
  exit 1
fi
presets="$(cd -- "$1" && pwd)"
root="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"
stamp="$(date +%Y%m%d%H%M)"
version="$(sed -n 's/^pkgver=//p' packaging/arch/PKGBUILD).dev.wt${stamp}"
mkdir -p "$root/test-results"
work="$(mktemp -d "$root/test-results/arch-package-private-${stamp}-XXXXXX")"
mkdir -p "$work/src/saya-studio" "$work/stage"
# Committed files that exist on disk (an uncommitted deletion would make tar fail).
git ls-files -z | while IFS= read -r -d '' f; do [ -e "$f" ] && printf '%s\0' "$f"; done \
  | tar --null -T - -cf - | tar -xf - -C "$work/src/saya-studio"
rm -rf "$work/src/saya-studio/presets"
cp -a "$presets" "$work/src/saya-studio/presets"
tar -C "$work/src" -cf - saya-studio | gzip -n > "$work/stage/saya-studio-$version.tar.gz"
cp packaging/arch/PKGBUILD packaging/arch/saya-studio.desktop packaging/arch/saya-studio.install "$work/stage/"
sed -i "s/^pkgver=.*/pkgver=$version/" "$work/stage/PKGBUILD"
cp docs/INSTALLATION-ARCH.md docs/SAUVEGARDES.md "$work/stage/"
printf '%s\nprivate presets: %s\n' "$(git rev-parse HEAD)" "$presets" > "$work/stage/SOURCE-COMMIT"
export CARGO_TARGET_DIR="$root/target" PKGEXT='.pkg.tar.zst'
cd "$work/stage"
makepkg --geninteg >> PKGBUILD
makepkg --noconfirm
if diff -rq "$presets" "$work/src/saya-studio/presets" >/dev/null; then
  echo "Presets: identical to $presets"
else
  echo "Presets: DIFFERENT from $presets" >&2
  exit 1
fi
printf '\nPackage: %s\n' "$(ls "$work"/stage/*.pkg.tar.zst)"
