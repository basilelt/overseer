<script lang="ts">
  import { onMount } from 'svelte';
  import ChevronDown from '@lucide/svelte/icons/chevron-down';
  import Ellipsis from '@lucide/svelte/icons/ellipsis';
  import Monitor from '@lucide/svelte/icons/monitor';
  import Plus from '@lucide/svelte/icons/plus';
  import Server from '@lucide/svelte/icons/server';
  import { connLabel } from '../../conn-label';
  import { hostMenu, openOnHost, toggleFold } from '../../host/host-actions';
  import type { HostLink } from '../../host/host-link.svelte';
  import { hostColor, hostName } from '../../host/hosts';
  import { itemsOf } from '../../host/host-watch';
  import { t } from '../../i18n.svelte';
  import { settings } from '../../settings/settings.svelte';
  import { spaces } from '../../workspace/spaces';
  import { app } from '../../state.svelte';
  import { visibleClock } from '../../visible-clock';
  import SpaceBlock from './SpaceBlock.svelte';
  import SpacesView from './SpacesView.svelte';
  import StatesView from './StatesView.svelte';
  import { isAgentItem } from '../../agents';

  let { filter, showArchived }: { filter: string; showArchived: boolean } = $props();

  let now = $state(Date.now());
  onMount(() => visibleClock((n) => (now = n)));

  const folded = $derived(new Set(settings.hostsFolded ?? []));

  function stateOf(l: HostLink) {
    return l === app.link ? app.conn : l.conn;
  }

  function count(l: HostLink): number {
    const items = l === app.link ? app.items : itemsOf(l).items;
    return items.filter((i) => isAgentItem(i)).length;
  }

  function dot(l: HostLink): string {
    const p = stateOf(l).phase;
    if (p === 'error') return 'var(--st-failed)';
    if (p === 'connected') return hostColor(l.key);
    return p === 'idle' ? 'var(--st-held)' : 'var(--st-paused)';
  }

  function meta(l: HostLink): string {
    const c = stateOf(l);
    if (c.prompt) return t('hosts.waiting');
    if (c.phase !== 'connected') return connLabel(c);
    if (l.target.kind === 'local') return t('connect.local');
    const addr = c.resolved?.split(' ')[0] ?? l.target.host ?? '';
    return addr === hostName(l.target) ? '' : addr;
  }

  function menu(e: MouseEvent, l: HostLink) {
    e.preventDefault();
    e.stopPropagation();
    app.openMenu(e, hostName(l.target), hostMenu(l.key));
  }
</script>

<div class="hosts" data-tour="host-sections">
  {#each app.hosts as l (l.key)}
    {@const isActive = l === app.link}
    {@const open = !folded.has(l.key)}
    {@const name = hostName(l.target)}
    <section class="host" class:active={isActive} aria-label={name}>
      <div class="head" role="presentation" oncontextmenu={(e) => menu(e, l)}>
        <button class="fold" type="button" aria-expanded={open} aria-label={t(open ? 'ws.collapse' : 'ws.expand', { name })} onclick={() => toggleFold(l.key)}>
          <ChevronDown size={12} class={open ? '' : 'shut'} />
        </button>
        <button class="pick" type="button" aria-current={isActive ? 'true' : undefined} title={isActive ? undefined : t('hosts.makeActive')} onclick={() => !isActive && app.switchHost(l.key)}>
          {#if l.target.kind === 'local'}<Monitor size={13} />{:else}<Server size={13} />{/if}
          <span class="dot" style:background={dot(l)}></span>
          <span class="name">{name}</span>
          <span class="meta mono" class:warn={stateOf(l).phase !== 'connected'}>{meta(l)}</span>
          <span class="n">{count(l)}</span>
        </button>
        <button class="icon-btn more" type="button" aria-label={t('hosts.menu', { name })} aria-haspopup="menu" onclick={(e) => menu(e, l)}><Ellipsis size={13} /></button>
      </div>
      {#if open}
        {#if isActive}
          {#if app.sidebarMode === 'workspaces'}<SpacesView {filter} {showArchived} />{:else}<StatesView {filter} />{/if}
        {:else}
          {@const its = itemsOf(l)}
          {@const view = spaces(l.ws.workspaces, its.items, its.itemWs, filter)}
          <div class="rows">
            {#each view.rows as r (r.id)}
              <SpaceBlock
                row={r}
                {now}
                open
                closedOpen={false}
                selectedKey={null}
                ontoggle={() => {}}
                onpick={() => openOnHost(l.key, () => app.selectWorkspace(r.id))}
                onclosed={() => {}}
                onselect={(key) => openOnHost(l.key, () => app.select(key))}
              />
            {/each}
            {#if !view.rows.length}<p class="none faint">{l.ready ? t('hosts.empty') : connLabel(l.conn)}</p>{/if}
          </div>
        {/if}
      {/if}
    </section>
  {/each}
  <button class="add" type="button" onclick={() => app.open('connect', 'add')}><Plus size={13} />{t('hosts.add')}</button>
</div>

<style>
  .hosts {
    flex: 1;
    min-height: 0;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-bottom: 8px;
  }
  .hosts :global(.spaces),
  .hosts :global(.states) {
    flex: none;
    overflow: visible;
  }
  .host {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .head {
    display: flex;
    align-items: center;
    gap: 2px;
    margin: 4px 8px 2px;
    padding: 2px 2px 2px 4px;
    border: 1px solid var(--border);
    border-radius: var(--radius);
    background: var(--surface-2);
  }
  .active .head {
    border-color: color-mix(in srgb, var(--accent) 55%, var(--border));
  }
  .fold {
    display: flex;
    align-items: center;
    justify-content: center;
    width: 20px;
    height: 24px;
    color: var(--text-2);
  }
  .fold :global(.shut) {
    transform: rotate(-90deg);
  }
  .pick {
    flex: 1;
    min-width: 0;
    display: flex;
    align-items: center;
    gap: 7px;
    height: 26px;
    padding: 0 4px;
    border-radius: 4px;
    color: var(--text-2);
    text-align: left;
  }
  .pick:hover {
    background: var(--surface-hover);
  }
  .dot {
    flex: none;
    width: 7px;
    height: 7px;
    border-radius: 50%;
  }
  .name {
    flex: none;
    max-width: 50%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text);
    font-size: 12.5px;
    font-weight: 600;
  }
  .meta {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
    color: var(--text-2);
  }
  .meta.warn {
    color: var(--st-paused);
  }
  .n {
    font-size: 11px;
    color: var(--text-2);
    font-variant-numeric: tabular-nums;
  }
  .more {
    flex: none;
  }
  .rows {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .none {
    margin: 2px 16px 6px;
    font-size: 12px;
  }
  .add {
    display: flex;
    align-items: center;
    gap: 8px;
    margin: 6px 8px 0;
    padding: 7px 10px;
    border: 1px dashed var(--border-strong);
    border-radius: var(--radius);
    color: var(--text-2);
    font-size: 12.5px;
  }
  .add:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
</style>
