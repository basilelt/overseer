<script lang="ts">
  import type { SidebarItem } from '../../grouping';
  import { t } from '../../i18n.svelte';
  import { age } from '../../time';
  // `now` comes from the shared clock so rows own no timer.
  let {
    item,
    now,
    selected = false,
    multi = false,
    onselect,
    onmenu,
  }: { item: SidebarItem; now: number; selected?: boolean; multi?: boolean; onselect: (e?: MouseEvent) => void; onmenu?: (e: MouseEvent) => void } = $props();

  const label = $derived(t(`session.state.${item.state}`));
</script>

<button class="row" type="button" data-key={item.key} class:multi aria-current={selected ? 'true' : undefined} onclick={(e) => onselect(e)} oncontextmenu={(e) => onmenu && (e.preventDefault(), onmenu(e))}>
  <span class="sr-only">{label}</span>
  <span class="dot" data-state={item.state} data-hollow={item.state === 'closed' ? '' : undefined} style:--c="var(--state-{item.state})" aria-hidden="true">{item.state === 'paused' ? '❙❙' : ''}</span>
  <span class="title">{item.title}</span>
  {#if item.status}<span class="action">{item.status}</span>{/if}
  <span class="age">{age(item.at, now)}</span>
  <span class="chev" aria-hidden="true">›</span>
</button>

<style>
  .row {
    display: flex;
    align-items: center;
    gap: 8px;
    height: 28px;
    margin-left: 20px;
    width: calc(100% - 20px);
    padding: 0 8px;
    border: 0;
    border-radius: 6px;
    background: var(--session-row-bg);
    text-align: left;
    min-width: 0;
    overflow: hidden;
  }
  .row:hover {
    background: var(--surface-hover);
  }
  .row[aria-current='true'] {
    background: var(--session-row-selected);
  }
  .row.multi {
    background: color-mix(in srgb, var(--accent) 25%, var(--session-row-bg));
    outline: 1px solid var(--accent);
    outline-offset: -1px;
  }
  .dot {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--c);
    color: var(--c);
    font-size: 7px;
    letter-spacing: -2px;
  }
  .dot[data-state='paused'] {
    width: auto;
    height: 10px;
    background: none;
    font-size: 10px;
    font-weight: 700;
    letter-spacing: -1px;
  }
  .dot[data-hollow] {
    background: none;
    box-shadow: inset 0 0 0 1.5px var(--c);
  }
  .title {
    flex: 0 1 auto;
    min-width: 80px;
    max-width: 100%;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .action {
    flex: 1 1 0;
    min-width: 0;
    color: var(--text-2);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .title:not(:has(+ .action)) {
    flex: 1;
  }
  .age {
    flex: none;
    margin-left: auto;
    font-size: 11px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .chev {
    flex: none;
    color: var(--text-2);
  }
</style>
