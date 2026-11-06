<script lang="ts">
  import { onMount } from 'svelte';
  import { attention } from '../../attention';
  import { t } from '../../i18n.svelte';
  import { saveSettings, settings } from '../../settings.svelte';
  import { folderSpaces, hostFor, spaceTrail, spaces, toggleOpen } from '../../spaces';
  import { hostKey } from '../../window';
  import { visibleClock } from '../../visible-clock';
  import { app } from '../../state.svelte';
  import { workspaceMenu } from '../../wsactions';
  import AttentionBanner from './AttentionBanner.svelte';
  import SpaceBlock from './SpaceBlock.svelte';
  import SpaceFolder from './SpaceFolder.svelte';

  let { filter }: { filter: string } = $props();

  let now = $state(Date.now());
  let closedOpen = $state(new Set<string>());

  // One clock for the whole view, paused while the window is hidden.
  onMount(() => visibleClock((n) => (now = n)));

  const nodes = $derived(folderSpaces(spaces(app.ws.workspaces, app.items, app.itemWs, filter), app.ws.folders, filter));
  const banner = $derived(attention(app.items, app.itemWs));
  const trail = $derived(spaceTrail(app.ws));
  const hk = $derived(hostKey(app.conn.target));
  const saved = $derived(settings.spacesOpen?.[hk]);
  const opened = $derived(new Set(saved ?? [app.workspaceId]));
  // Every session of a connection runs on that connection's host.
  const host = $derived(hostFor(app.conn));
  const q = $derived(filter.trim());

  function toggle(id: string) {
    saveSettings({ spacesOpen: { ...settings.spacesOpen, [hostKey(app.conn.target)]: toggleOpen(saved, id, new Set(['', ...app.ws.workspaces.map((w) => w.id)]), app.workspaceId) } });
  }

  // Same store as the tree view: the daemon keeps it per host. Flipped locally first so the click feels instant.
  async function toggleFolder(id: string) {
    const f = app.ws.folders.find((x) => x.id === id);
    if (!f) return;
    app.ws = { ...app.ws, folders: app.ws.folders.map((x) => (x.id === id ? { ...x, collapsed: !x.collapsed } : x)) };
    await app.api.patchFolder(id, { collapsed: !f.collapsed }).catch(() => {});
  }

  function toggleClosed(id: string) {
    const next = new Set(closedOpen);
    if (!next.delete(id)) next.add(id);
    closedOpen = next;
  }
</script>

<div class="spaces">
  <AttentionBanner entries={banner} pathOf={(e) => trail(e.workspaceId)} onselect={(e) => app.select(e.key)} />
  {#each nodes as n (n.key)}
    <div class="node" class:nested={n.depth > 0} style:--depth={n.depth}>
      {#if n.type === 'folder'}
        <SpaceFolder node={n} ontoggle={() => n.folder && toggleFolder(n.folder.id)} />
      {:else if n.type === 'space'}
        {@const r = n.row}
        <SpaceBlock
          row={r}
          {now}
          {host}
          open={opened.has(r.id)}
          closedOpen={closedOpen.has(r.id)}
          selectedKey={app.selectedKey}
          ontoggle={() => toggle(r.id)}
          onclosed={() => toggleClosed(r.id)}
          onselect={(key) => app.select(key)}
          onmenu={r.workspace ? (e) => app.openMenu(e, r.workspace!.name, workspaceMenu(r.workspace!)) : undefined}
        />
      {:else}
        <div class="empty">
          <span class="label">{t('ws.noSession')}</span>
          <div class="chips">
            {#each n.workspaces as w (w.id)}
              <button class="chip" type="button" onclick={() => app.selectWorkspace(w.id)}>{w.name}</button>
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/each}
  {#if !nodes.length}<p class="none faint">{q ? t('common.none') : t('ws.empty')}</p>{/if}
</div>

<style>
  .spaces {
    flex: 1;
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 0 8px;
  }
  /* 14px per level, on a faint guide line under the parent's chevron. */
  .node {
    padding-left: calc(var(--depth) * 14px);
  }
  .nested {
    background: linear-gradient(var(--ws-frame-border), var(--ws-frame-border)) no-repeat calc((var(--depth) - 1) * 14px + 12px) 0 / 1px 100%;
  }
  .empty {
    margin: 0 6px;
    padding: 0 6px;
  }
  .label {
    font-size: 11px;
    font-weight: 600;
    color: var(--text-2);
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
  }
  .chip {
    max-width: 100%;
    height: 22px;
    padding: 0 8px;
    border: 1px dashed var(--ws-frame-border);
    border-radius: 11px;
    background: none;
    color: var(--text-2);
    font-size: 12px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip:hover {
    background: var(--surface-hover);
  }
  .none {
    padding: 24px 12px;
    text-align: center;
  }
</style>
