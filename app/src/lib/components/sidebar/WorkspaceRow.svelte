<script lang="ts" module>
  import type { WsState } from '../../workspaces';

  export const WS_COLOR: Record<WsState, string> = {
    needs_input: 'var(--st-input)',
    running: 'var(--st-working)',
    needs_testing: 'var(--st-testing)',
    ready: 'var(--border-strong)',
    paused: 'var(--st-paused)',
    stopped: 'var(--st-held)',
  };
</script>

<script lang="ts">
  import { shortPath } from '../../dirs';
  import Zap from '@lucide/svelte/icons/zap';
  import Pin from '@lucide/svelte/icons/pin';
  import { summaryParts, summaryText, type Aggregate } from '../../workspaces';
  import { t } from '../../i18n.svelte';
  import { app } from '../../state.svelte';

  let {
    id,
    name,
    path,
    agg,
    selected,
    pinned = false,
    archived = false,
    missing = false,
    depth = 0,
    dropPos = null,
    onselect,
    onmenu,
    onpointerdown,
  }: {
    id?: string;
    name: string;
    path?: string;
    agg?: Aggregate;
    selected: boolean;
    pinned?: boolean;
    archived?: boolean;
    missing?: boolean;
    depth?: number;
    dropPos?: 'before' | 'after' | 'inside' | null;
    onselect: () => void;
    onmenu?: (e: MouseEvent) => void;
    onpointerdown?: (e: PointerEvent) => void;
  } = $props();

  const parts = $derived(summaryParts(agg));
  const state = $derived(parts[0]!.state);
  const summary = $derived(summaryText(parts, (s) => t(`wsstate.${s}`)));
  const host = $derived(
    app.conn.target.kind === 'ssh' ? (app.conn.target.host ?? app.conn.target.label) : app.conn.target.kind === 'direct' ? app.conn.target.label : t('host.local'),
  );
  const online = $derived(app.conn.phase === 'connected' ? 'on' : app.conn.phase === 'error' || app.conn.phase === 'idle' ? 'off' : 'wait');
</script>

<button
  class="ws"
  class:selected
  class:archived
  class:missing
  class:drop-before={dropPos === 'before'}
  class:drop-after={dropPos === 'after'}
  class:tab-drop={!!id && app.tabDrop === id}
  data-workspace-id={id}
  style:--indent="{depth * 12}px"
  role="treeitem"
  aria-selected={selected}
  tabindex="-1"
  onclick={onselect}
  oncontextmenu={(e) => {
    e.preventDefault();
    onmenu?.(e);
  }}
  {onpointerdown}
>
  <div class="top">
    <span class="dot" style:background={WS_COLOR[state]}></span>
    <span class="name">{name}</span>
    {#if pinned}<span class="pin faint"><Pin size={11} /></span>{/if}
    <span class="state" class:hot={state !== 'ready'} style:--c={WS_COLOR[state]} title={summary}>
      {#if state === 'running'}<Zap size={11} />{/if}
      {summary}
    </span>
  </div>
  {#if agg?.status}<div class="line muted">{agg.status}</div>{/if}
  <div class="line meta faint">
    <span class="conn {online}"></span>
    <span class="host">{host}</span>
    {#if agg?.agents}<span>· {t(agg.agents === 1 ? 'ws.agent' : 'ws.agents', { n: agg.agents })}</span>{/if}
    {#if missing}<span class="gone" title={path}>{t('ws.missing')}</span>{:else if path}<span class="path mono">{shortPath(path)}</span>{/if}
  </div>
</button>

<style>
  .ws {
    position: relative;
    display: flex;
    flex-direction: column;
    gap: 1px;
    width: 100%;
    padding: 6px 8px 7px calc(10px + var(--indent));
    border-radius: var(--radius);
    text-align: left;
    transition: background var(--ease);
  }
  .ws:hover {
    background: var(--surface-hover);
  }
  .ws.selected {
    background: var(--accent-soft);
  }
  .ws.tab-drop {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .ws.archived {
    opacity: 0.55;
  }
  .ws.missing .name {
    color: var(--text-3);
  }
  .gone {
    flex: 1;
    min-width: 0;
    text-align: right;
    font-style: italic;
  }
  .drop-before::before,
  .drop-after::after {
    content: '';
    position: absolute;
    left: calc(8px + var(--indent));
    right: 8px;
    height: 2px;
    border-radius: 1px;
    background: var(--accent);
  }
  .drop-before::before {
    top: -1px;
  }
  .drop-after::after {
    bottom: -1px;
  }
  .top {
    display: flex;
    align-items: center;
    gap: 8px;
    min-width: 0;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    flex: none;
  }
  .name {
    flex: 1;
    min-width: 0;
    font-weight: 500;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pin {
    display: inline-grid;
  }
  .state {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 11px;
    color: var(--text-3);
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .state.hot {
    color: var(--c);
    font-weight: 500;
  }
  .line {
    font-size: 12px;
    line-height: 16px;
    padding-left: 15px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meta {
    display: flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
  }
  .conn {
    width: 5px;
    height: 5px;
    border-radius: 50%;
    flex: none;
    background: var(--st-held);
  }
  .conn.on {
    background: var(--st-input);
  }
  .conn.wait {
    background: var(--st-testing);
  }
  .host {
    flex: none;
  }
  .path {
    flex: 1;
    min-width: 0;
    text-align: right;
    font-size: 10.5px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
