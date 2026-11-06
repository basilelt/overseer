<script lang="ts">
  import type { SidebarItem } from '../../grouping';
  import { t } from '../../i18n.svelte';
  import { age } from '../../time';
  import { connLabel } from '../../conn-label';
  import type { ConnState } from '../../types';

  // host is set for SSH sessions only; `now` comes from the shared clock so rows own no timer.
  let {
    item,
    now,
    selected = false,
    host,
    onselect,
  }: { item: SidebarItem; now: number; selected?: boolean; host?: { name: string } & Pick<ConnState, 'phase' | 'attempt' | 'version'>; onselect: (e?: MouseEvent) => void } = $props();

  const label = $derived(t(`session.state.${item.state}`));
</script>

<button class="row" type="button" data-key={item.key} aria-current={selected ? 'true' : undefined} onclick={(e) => onselect(e)}>
  <span class="sr-only">{label}</span>
  <span class="dot" data-state={item.state} data-hollow={item.state === 'closed' ? '' : undefined} style:--c="var(--state-{item.state})" aria-hidden="true">{item.state === 'paused' ? '❙❙' : ''}</span>
  <span class="title">{item.title}</span>
  {#if item.status}<span class="action">{item.status}</span>{/if}
  <span class="age">{age(item.at, now)}</span>
  {#if host}<span class="host" title={connLabel(host)}><span class="hname">{host.name}</span><span class="hst">{connLabel(host)}</span></span>{/if}
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
    background: none;
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
  .host {
    flex: 0 1 auto;
    min-width: 0;
    max-width: 40%;
    overflow: hidden;
    display: inline-flex;
    gap: 4px;
    padding: 0 6px;
    border-radius: 4px;
    background: var(--surface-2);
    font-size: 10px;
    color: var(--text-2);
  }
  .hname {
    flex: 0 4 auto;
    min-width: 3ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hst {
    flex: 0 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chev {
    flex: none;
    color: var(--text-2);
  }
</style>
