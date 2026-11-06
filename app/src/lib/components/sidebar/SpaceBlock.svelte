<script lang="ts">
  import { t } from '../../i18n.svelte';
  import type { SpaceRow } from '../../spaces';
  import { workspaceSessions } from '../../workspace-sessions';
  import type { ConnState } from '../../types';
  import SessionRow from './SessionRow.svelte';
  import StatePills from './StatePills.svelte';
  import WorkspaceFrame from './WorkspaceFrame.svelte';

  // Purely presentational: the parent owns the expand state, the clock and the open action.
  let {
    row,
    now,
    open,
    closedOpen,
    selectedKey,
    host,
    ontoggle,
    onclosed,
    onselect,
    onmenu,
  }: {
    row: SpaceRow;
    now: number;
    open: boolean;
    closedOpen: boolean;
    selectedKey: string | null;
    host?: { name: string } & Pick<ConnState, 'phase' | 'attempt' | 'version'>;
    ontoggle: () => void;
    onclosed: () => void;
    onselect: (key: string) => void;
    onmenu?: (e: MouseEvent) => void;
  } = $props();

  const sessions = $derived(open ? workspaceSessions(row.items, now) : null);
  const name = $derived(row.name ?? t('ws.other'));
</script>

{#snippet line()}
  <button class="line" type="button" aria-expanded={open} onclick={ontoggle} oncontextmenu={(e) => onmenu && (e.preventDefault(), onmenu(e))}>
    <span class="name">{name}</span>
    <StatePills summary={row.summary} />
  </button>
{/snippet}

{#if sessions}
  <WorkspaceFrame header={line}>
    {#each sessions.live as it (it.key)}
      <SessionRow item={it} {now} {host} selected={selectedKey === it.key} onselect={() => onselect(it.key)} />
    {/each}
    {#if sessions.closed.count}
      <button class="more" type="button" aria-expanded={closedOpen} onclick={onclosed}>{sessions.closed.count === 1 ? t('ws.closedOne') : t('ws.closedMore', { n: sessions.closed.count })}</button>
      {#if closedOpen}
        {#each sessions.closed.buckets as b (b.bucket)}
          <div class="bucket">{t(`ws.bucket.${b.bucket}`)}</div>
          {#each b.items as it (it.key)}
            <SessionRow item={it} {now} {host} selected={selectedKey === it.key} onselect={() => onselect(it.key)} />
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
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 28px;
    padding: 0 6px;
    border: 0;
    border-radius: 6px;
    background: none;
    text-align: left;
    min-width: 0;
    font-weight: 600;
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
