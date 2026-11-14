<script lang="ts">
  import { t } from '../../i18n.svelte';
  import { heat, type Band } from '../../review/map.svelte';

  let { bands, current = null, names, onpick }: { bands: Band[]; current?: string | null; names: (id: string) => string; onpick: (id: string) => void } = $props();

  let tip = $state<{ id: string; heat: number; y: number } | null>(null);

  function show(b: Band, e: PointerEvent) {
    const el = e.currentTarget as HTMLElement;
    tip = { id: b.id, heat: b.heat, y: el.offsetTop + el.offsetHeight / 2 };
  }
</script>

<nav class="band" aria-label={t('map.band')} onpointerleave={() => (tip = null)}>
  {#each bands as b (b.id)}
    <button class:on={b.id === current} style:flex-grow={b.share} style:background={heat(b.heat)} aria-label={names(b.id)} onpointerenter={(e) => show(b, e)} onclick={() => onpick(b.id)}></button>
  {/each}
  {#if tip}<span class="tip mono" style:top="{tip.y}px" aria-hidden="true">{names(tip.id)}<b>{Math.round(tip.heat * 100)}</b></span>{/if}
</nav>

<style>
  .band {
    position: relative;
    flex: none;
    width: 16px;
    display: flex;
    flex-direction: column;
    gap: 1px;
    padding: 6px 3px;
    border-right: 1px solid var(--border);
    background: var(--bg-subtle);
  }
  button {
    flex: 1 1 0;
    min-height: 3px;
    padding: 0;
    border: 0;
    border-radius: 1px;
    cursor: pointer;
  }
  button:hover {
    filter: brightness(1.15);
  }
  button.on {
    outline: 2px solid var(--text);
    outline-offset: 1px;
    z-index: 1;
  }
  .tip {
    position: absolute;
    left: calc(100% + 4px);
    transform: translateY(-50%);
    display: flex;
    gap: 8px;
    max-width: 360px;
    padding: 3px 8px;
    border-radius: var(--radius);
    background: var(--surface);
    color: var(--text);
    box-shadow: var(--shadow);
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
    pointer-events: none;
    z-index: 20;
  }
  .tip b {
    color: var(--text-2);
    font-weight: 500;
  }
</style>
