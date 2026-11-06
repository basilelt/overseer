<script lang="ts">
  import { t } from '../../i18n.svelte';
  import type { SpaceRow } from '../../spaces';
  import { workspaceSessions } from '../../workspace-sessions';
  import type { ConnState } from '../../types';
  import SessionRow from './SessionRow.svelte';
  import StatePills from './StatePills.svelte';
  import WorkspaceFrame from './WorkspaceFrame.svelte';

  let {
    row,
    now,
    open,
    closedOpen,
    selectedKey,
    host,
    selected = false,
    tabDrop = false,
    dropPos = null,
    ontoggle,
    onpick,
    onclosed,
    onselect,
    onmenu,
    onitemmenu,
    onpointerdown,
  }: {
    row: SpaceRow;
    now: number;
    open: boolean;
    closedOpen: boolean;
    selectedKey: string | null;
    host?: { name: string } & Pick<ConnState, 'phase' | 'attempt' | 'version'>;
    /** This workspace is the active one. */
    selected?: boolean;
    /** A dragged tab hovers the header. */
    tabDrop?: boolean;
    dropPos?: 'before' | 'after' | 'inside' | null;
    /** The chevron only folds; the name picks the workspace. */
    ontoggle: () => void;
    onpick: () => void;
    onclosed: () => void;
    onselect: (key: string, e?: MouseEvent) => void;
    onmenu?: (e: MouseEvent) => void;
    onitemmenu?: (key: string, title: string, e: MouseEvent) => void;
    onpointerdown?: (e: PointerEvent) => void;
  } = $props();

  const sessions = $derived(open ? workspaceSessions(row.items, now) : null);
  const name = $derived(row.name ?? t('ws.other'));
  const archived = $derived(!!row.workspace?.archived);
  const missing = $derived(!!row.workspace?.missing);
  // "Other" has no id: it cannot receive a tab or be dragged.
  const id = $derived(row.workspace ? row.id : undefined);
</script>

{#snippet line()}
  <div
    class="line"
    class:selected
    class:archived
    class:missing
    class:tab-drop={tabDrop}
    class:drop-before={dropPos === 'before'}
    class:drop-after={dropPos === 'after'}
    data-workspace-id={id}
    data-drop={id ? 'workspace' : undefined}
    data-id={id}
    role="presentation"
    oncontextmenu={(e) => onmenu && (e.preventDefault(), onmenu(e))}
    {onpointerdown}
  >
    <button class="chev" type="button" aria-expanded={open} aria-label={t(open ? 'ws.collapse' : 'ws.expand', { name })} onclick={ontoggle}><span class:open>›</span></button>
    <button class="pick" type="button" aria-current={selected ? 'true' : undefined} onclick={onpick}>
      <span class="name">{name}</span>
      {#if missing}<span class="gone" title={row.workspace?.path ?? undefined}>{t('ws.missing')}</span>{/if}
      <StatePills summary={row.summary} />
    </button>
  </div>
{/snippet}

{#if sessions}
  <WorkspaceFrame header={line}>
    {#each sessions.live as it (it.key)}
      <SessionRow item={it} {now} {host} selected={selectedKey === it.key} onselect={(e) => onselect(it.key, e)} onmenu={onitemmenu && ((e) => onitemmenu(it.key, it.title, e))} />
    {/each}
    {#if sessions.closed.count}
      <button class="more" type="button" aria-expanded={closedOpen} onclick={onclosed}>{sessions.closed.count === 1 ? t('ws.closedOne') : t('ws.closedMore', { n: sessions.closed.count })}</button>
      {#if closedOpen}
        {#each sessions.closed.buckets as b (b.bucket)}
          <div class="bucket">{t(`ws.bucket.${b.bucket}`)}</div>
          {#each b.items as it (it.key)}
            <SessionRow item={it} {now} {host} selected={selectedKey === it.key} onselect={(e) => onselect(it.key, e)} onmenu={onitemmenu && ((e) => onitemmenu(it.key, it.title, e))} />
          {/each}
        {/each}
      {/if}
    {/if}
  </WorkspaceFrame>
{:else}
  <div class="solo">{@render line()}</div>
{/if}

<style>
  .solo {
    margin: 0 6px;
  }
  .line {
    position: relative;
    display: flex;
    align-items: center;
    gap: 2px;
    width: 100%;
    height: 28px;
    border-radius: 6px;
    min-width: 0;
    font-weight: 600;
  }
  .line.selected {
    background: var(--accent-soft);
  }
  .line.tab-drop {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .line.missing .name {
    color: var(--text-3);
  }
  .gone {
    flex: none;
    font-size: 11px;
    font-weight: 500;
    color: var(--text-3);
  }
  .line.archived {
    opacity: 0.55;
  }
  .drop-before::before,
  .drop-after::after {
    content: '';
    position: absolute;
    left: 8px;
    right: 8px;
    height: 2px;
    background: var(--accent);
  }
  .drop-before::before {
    top: -1px;
  }
  .drop-after::after {
    bottom: -1px;
  }
  .chev,
  .pick {
    display: flex;
    align-items: center;
    height: 100%;
    border: 0;
    background: none;
    font-weight: inherit;
    text-align: left;
  }
  .chev {
    flex: none;
    width: 20px;
    justify-content: center;
    padding: 0;
    color: var(--text-2);
    border-radius: 6px;
  }
  .chev span {
    display: inline-block;
  }
  .chev .open {
    transform: rotate(90deg);
  }
  .pick {
    flex: 1;
    gap: 6px;
    min-width: 0;
    padding: 0 6px 0 0;
    border-radius: 6px;
  }
  .solo .line:hover {
    background: var(--surface-hover);
  }
  .name {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .more {
    margin-left: 20px;
    height: 24px;
    padding: 0 8px;
    border: 0;
    background: none;
    color: var(--text-2);
    font-size: 12px;
    text-align: left;
    border-radius: 6px;
  }
  .more:hover {
    background: var(--surface-hover);
  }
  .bucket {
    margin-left: 20px;
    padding: 2px 8px 0;
    font-size: 11px;
    color: var(--text-2);
  }
</style>
