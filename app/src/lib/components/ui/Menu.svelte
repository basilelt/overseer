<script lang="ts">
  import Check from '@lucide/svelte/icons/check';
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import { menuRows, type MenuItem } from '../../menu';
  import AgentGlyph from './AgentGlyph.svelte';
  import { ui } from '../../mobile/ui.svelte';
  import { untrack } from 'svelte';

  let { items, x, y, label, onclose }: { items: MenuItem[]; x: number; y: number; label: string; onclose: () => void } = $props();

  let menu: HTMLDivElement | undefined = $state();
  let pos = $state({ left: 0, top: 0 });
  let expanded = $state(new Set<number>());
  const rows = $derived(menuRows(items, expanded));

  $effect(() => {
    if (!menu) return;
    const r = menu.getBoundingClientRect();
    pos = { left: Math.max(4, Math.min(x, innerWidth - r.width - 8)), top: Math.max(4, Math.min(y, innerHeight - r.height - 8)) };
    (menu.querySelector<HTMLElement>('button[data-default]:not([disabled])') ?? menu.querySelector<HTMLElement>('button:not([disabled])'))?.focus();
  });

  $effect(() => {
    if (!menu || !expanded.size) return;
    const r = menu.getBoundingClientRect();
    untrack(() => (pos = { left: pos.left, top: Math.max(4, Math.min(pos.top, innerHeight - r.height - 8)) }));
  });

  function toggle(index: number) {
    const next = new Set(expanded);
    if (!next.delete(index)) next.add(index);
    expanded = next;
  }

  function pick(it: MenuItem) {
    onclose();
    it.run?.();
  }

  function onKey(e: KeyboardEvent) {
    const all = [...(menu?.querySelectorAll<HTMLElement>('button:not([disabled])') ?? [])];
    const i = all.indexOf(document.activeElement as HTMLElement);
    if (e.key === 'ArrowDown') all[(i + 1) % all.length]?.focus();
    else if (e.key === 'ArrowUp') all[(i - 1 + all.length) % all.length]?.focus();
    else if (e.key === 'Escape') onclose();
    else return;
    e.preventDefault();
    e.stopPropagation();
  }
</script>

<svelte:window onpointerdown={(e) => menu && !menu.contains(e.target as Node) && onclose()} onblur={onclose} onkeydown={(e) => e.key === 'Escape' && onclose()} />

{#if ui.layout === 'phone'}<div class="scrim" role="presentation"></div>{/if}
<div class="menu" role="menu" tabindex="-1" aria-label={label} bind:this={menu} style:left="{pos.left}px" style:top="{pos.top}px" onkeydown={onKey}>
  {#if ui.layout === 'phone' && label}<div class="sheet-title">{label}</div>{/if}
  {#each rows as { item: it, index, child }, i (i)}
    {#if it.separator}
      <hr />
    {:else if it.more && !child}
      <button role="menuitem" class="more" aria-expanded={expanded.has(index)} onclick={() => toggle(index)}>
        <span class="ic chev" class:open={expanded.has(index)}><ChevronRight size={13} /></span>
        <span class="label">{it.label}</span>
      </button>
    {:else}
      <button role="menuitem" class:child class:danger={it.danger} disabled={it.disabled} data-default={it.default || undefined} title={it.hint} onclick={() => pick(it)}>
        <span class="ic">
          {#if it.agent}<AgentGlyph agent={it.agent} size={13} />{:else if it.icon}<it.icon size={13} />{:else if it.checked}<Check size={13} />{/if}
        </span>
        <span class="label">{it.label}</span>
        {#if it.hint && it.disabled}<span class="hint mono">{it.hint}</span>{/if}
      </button>
    {/if}
  {/each}
</div>

<style>
  .menu {
    position: fixed;
    min-width: 200px;
    max-width: 320px;
    padding: 4px;
    background: var(--surface);
    border-radius: var(--radius);
    box-shadow: var(--shadow);
    z-index: 60;
    outline: none;
  }
  button {
    display: flex;
    align-items: center;
    gap: 8px;
    width: 100%;
    height: 28px;
    padding: 0 8px;
    border-radius: 4px;
    text-align: left;
    color: var(--text);
  }
  button:hover:not(:disabled),
  button:focus-visible {
    background: var(--accent-soft);
    outline: none;
  }
  button:disabled {
    color: var(--text-3);
    cursor: default;
  }
  .danger {
    color: var(--st-failed);
  }
  .more {
    color: var(--text-2);
  }
  .chev {
    transition: transform 120ms;
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .child {
    padding-left: 20px;
  }
  .ic {
    width: 16px;
    display: grid;
    place-items: center;
    color: var(--text-2);
    flex: none;
  }
  .label {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hint {
    font-size: 10.5px;
    color: var(--text-3);
    max-width: 120px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  hr {
    border: 0;
    border-top: 1px solid var(--border);
    margin: 4px 0;
  }
  .scrim {
    position: fixed;
    inset: 0;
    z-index: 59;
    background: var(--overlay);
  }
  .sheet-title {
    padding: 10px 12px 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-3);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  :global(.phone) .menu {
    left: 0 !important;
    right: 0;
    top: auto !important;
    bottom: 0;
    min-width: 0;
    max-width: none;
    max-height: 80dvh;
    overflow-y: auto;
    padding: 6px 8px calc(8px + var(--safe-bottom));
    border-radius: 14px 14px 0 0;
    animation: menu-up 160ms cubic-bezier(0.2, 0, 0, 1);
  }
  :global(.phone) button {
    height: 48px;
    gap: 12px;
    padding: 0 12px;
    font-size: 15px;
  }
  :global(.phone) .ic {
    width: 20px;
  }
  :global(.phone) .hint {
    max-width: 160px;
  }
  @keyframes menu-up {
    from {
      transform: translateY(100%);
    }
  }
</style>
