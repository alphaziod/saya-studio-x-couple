<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import { menuPosition, type MenuItem } from '../lib/context-menu';
  // Right-click menu shared by the pages: opens at the pointer, closes on Escape, a click outside,
  // scrolling, resizing or after an item ran.
  let { x, y, label, items, onclose }: { x: number; y: number; label: string; items: MenuItem[]; onclose: () => void } = $props();
  let element = $state<HTMLElement | null>(null);
  let position = $state({ x: 0, y: 0 });
  $effect(() => {
    const box = element?.getBoundingClientRect();
    position = menuPosition(x, y, box?.width ?? 240, box?.height ?? 240, { width: window.innerWidth, height: window.innerHeight });
  });
  onMount(() => element?.querySelector<HTMLButtonElement>('button:not(:disabled)')?.focus());
  function choose(item: MenuItem) { if (item.disabled) return; onclose(); void item.run(); }
  function outside(event: PointerEvent) { if (element && !element.contains(event.target as Node)) onclose(); }
  function key(event: KeyboardEvent) { if (event.key === 'Escape') { event.preventDefault(); onclose(); } }
</script>
<svelte:window onpointerdown={outside} onkeydown={key} onscrollcapture={onclose} onresize={onclose} onblur={onclose}/>
<div class="ctx-menu" role="menu" tabindex="-1" aria-label={label} data-context-menu bind:this={element} style:left={`${position.x}px`} style:top={`${position.y}px`} oncontextmenu={event => event.preventDefault()}>
  {#each items as item (item.label)}
    <button role="menuitem" class:danger={item.danger} disabled={item.disabled} onclick={() => choose(item)}>{#if item.icon}<Icon name={item.icon}/>{/if}{item.label}</button>
  {/each}
</div>
<style>
  .ctx-menu{position:fixed;z-index:1000;min-width:220px;max-width:min(320px,calc(100vw - 8px));max-height:calc(100vh - 8px);overflow:auto;display:grid;gap:2px;padding:4px;background:var(--panel);border:1px solid var(--line-strong);border-radius:var(--radius);box-shadow:0 8px 24px #0008}
  .ctx-menu button{display:flex;align-items:center;gap:8px;background:transparent;border-color:transparent;color:var(--text);text-align:start;white-space:nowrap}
  .ctx-menu button:hover:not(:disabled),.ctx-menu button:focus-visible{background:var(--hover)}
  .ctx-menu button.danger{color:var(--danger)}
</style>
