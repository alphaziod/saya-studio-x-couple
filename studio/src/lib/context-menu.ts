/** One entry of a right-click menu (`ContextMenu.svelte`). */
export interface MenuItem {
  label: string;
  icon?: string;
  disabled?: boolean;
  danger?: boolean;
  run: () => unknown;
}

/** Keeps a menu of `width`×`height` inside the window, starting at the pointer. */
export function menuPosition(x: number, y: number, width: number, height: number, viewport: { width: number; height: number }) {
  return { x: Math.max(4, Math.min(x, viewport.width - width - 4)), y: Math.max(4, Math.min(y, viewport.height - height - 4)) };
}

/**
 * The WebView's own menu (Back, Forward, Stop, Reload) makes no sense in the app: it is kept only
 * where it is useful, in editable fields and on a text selection (copy, paste).
 */
export function keepsNativeMenu(target: EventTarget | null, selection: string): boolean {
  const element = target as { closest?: (selector: string) => unknown } | null;
  return selection.trim() !== '' || !!element?.closest?.('input, textarea, [contenteditable="true"]');
}
