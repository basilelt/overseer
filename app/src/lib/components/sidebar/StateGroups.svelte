<script lang="ts">
  import type { WorkspaceGroup } from '../../by-state';
  import { t } from '../../i18n.svelte';
  import type { SpaceHost } from '../../spaces';
  import SessionRow from './SessionRow.svelte';
  import WorkspaceFrame from './WorkspaceFrame.svelte';

  // One frame per workspace, its sessions in the order given; the unassigned group is named "Other sessions".
  let {
    groups,
    names,
    now,
    selectedKey,
    host,
    onselect,
    onmenu,
  }: {
    groups: WorkspaceGroup[];
    names: ReadonlyMap<string, string>;
    now: number;
    selectedKey: string | null;
    host?: SpaceHost;
    onselect: (key: string, e?: MouseEvent) => void;
    onmenu?: (key: string, title: string, e: MouseEvent) => void;
  } = $props();
</script>

{#each groups as g (g.workspaceId ?? '')}
  {#snippet head()}<span class="gname">{(g.workspaceId && names.get(g.workspaceId)) || t('ws.other')}</span>{/snippet}
  <WorkspaceFrame header={head}>
    {#each g.items as it (it.key)}
      <SessionRow item={it} {now} {host} selected={selectedKey === it.key} onselect={(e) => onselect(it.key, e)} onmenu={onmenu && ((e) => onmenu(it.key, it.title, e))} />
    {/each}
  </WorkspaceFrame>
{/each}

<style>
  .gname {
    display: block;
    padding: 0;
  }
</style>
