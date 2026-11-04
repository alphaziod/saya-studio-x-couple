import { call } from './api';

// Display names for gallery images. A casting trial is saved under its job id
// (`<uuid>-<node>-<index>.png`), which tells the user nothing; the casting knows the character
// the image shows, so the gallery displays that name. Other images keep their file name.
const names = $state<{ byAsset: Record<string, string> }>({ byAsset: {} });
let loading: Promise<void> | null = null;

type CastingRow = { name?: string; asset?: string | null; finalAsset?: string | null; phases?: Record<string, { asset?: string }> };

/** Reads the casting history once per call site refresh (`force`) and maps every pass image to its character. */
export function loadAssetNames(force = false): Promise<void> {
  if (loading && !force) return loading;
  loading = call<CastingRow[]>('casting_history', { sync: false })
    .then(rows => {
      const byAsset: Record<string, string> = {};
      for (const row of rows) {
        if (!row?.name) continue;
        for (const id of [row.asset, row.finalAsset, ...Object.values(row.phases ?? {}).map(p => p?.asset)]) {
          if (id) byAsset[id] = row.name;
        }
      }
      names.byAsset = byAsset;
    })
    .catch(() => { /* names are cosmetic: keep file names when the history is unavailable */ });
  return loading;
}

/** File name without folders and image extension. */
export function fileLabel(path: string): string {
  return (path.split(/[\\/]/).pop() ?? path).replace(/\.(png|jpe?g|webp)$/i, '');
}

/** Character name for a casting image, otherwise its file name. Reactive in templates. */
export function assetLabel(asset: { id: string; path: string }): string {
  return names.byAsset[asset.id] ?? fileLabel(asset.path);
}
