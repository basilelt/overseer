<script lang="ts">
  import Plus from '@lucide/svelte/icons/plus';
  import Users from '@lucide/svelte/icons/users';
  import { onMount } from 'svelte';
  import { attention } from '../../attention';
  import { t } from '../../i18n.svelte';
  import { saveSettings, settings } from '../../settings.svelte';
  import { folderSpaces, hostFor, openOnPick, spaceTrail, spaces, toggleOpen } from '../../spaces';
  import { hostKey } from '../../window';
  import { visibleClock } from '../../visible-clock';
  import { app, NO_WORKSPACE } from '../../state.svelte';
  import { aggregate, byRank } from '../../workspaces';
  import { WsDrag } from '../../ws-drag.svelte';
  import { WS_COLOR } from '../../ws-color';
  import { loadKnown } from '../../known-tabs';
  import { tabOrder } from '../../tab-order';
  import { allTabs, savedLayouts, workspace } from '../../workspace.svelte';
  import { createGroup, folderMenu, groupMenu, itemMenuAt, otherMenu, selectSession, startIn, workspaceMenu } from '../../wsactions';
  import type { Workspace } from '../../types';
  import AttentionBanner from './AttentionBanner.svelte';
  import SpaceBlock from './SpaceBlock.svelte';
  import SpaceFolder from './SpaceFolder.svelte';

  let { filter, showArchived }: { filter: string; showArchived: boolean } = $props();

  let now = $state(Date.now());
  let closedOpen = $state(new Set<string>());
  const drag = new WsDrag();

  onMount(() => visibleClock((n) => (now = n)));

  const nodes = $derived(folderSpaces(spaces(app.ws.workspaces, app.items, app.itemWs, filter, showArchived), app.ws.folders, filter));
  const banner = $derived(attention(app.items, app.itemWs));
  const trail = $derived(spaceTrail(app.ws));
  const hk = $derived(hostKey(app.conn.target));
  const saved = $derived(settings.spacesOpen?.[hk]);
  // The parent of the active nested workspace stays open, so the active one is always in view.
  const opened = $derived(new Set([...(saved ?? [app.workspaceId]), ...(app.currentWorkspace?.parentId ? [app.currentWorkspace.parentId] : [])]));
  // Every session of a connection runs on that connection's host.
  const host = $derived(hostFor(app.conn));
  const q = $derived(filter.trim());
  const groups = $derived(app.ws.groups.slice().sort(byRank));
  const pinned = $derived(nodes.some((n) => n.type === 'folder' && !n.folder));
  // Joined so the rows re-sort only when the tab list changes, not on a split resize or a tab switch.
  const liveTabs = $derived(allTabs(workspace.layout).join('\n'));
  const tabsOf = (id: string) =>
    tabOrder(id, app.workspaceId, liveTabs ? liveTabs.split('\n') : [], savedLayouts, id === NO_WORKSPACE ? loadKnown(hk, '#other')?.tabs : app.ws.workspaces.find((w) => w.id === id)?.tabs);
  const rowDrop = (key: string) => drag.posOf('session', key);

  function toggle(id: string) {
    if (drag.suppressed) return;
    saveSettings({ spacesOpen: { ...settings.spacesOpen, [hostKey(app.conn.target)]: toggleOpen(saved, id, new Set(['', ...app.ws.workspaces.map((w) => w.id)]), app.workspaceId) } });
  }

  // Selects the workspace and opens it, so its tab bar and "+" show and its sessions are in view.
  function pick(id: string) {
    if (drag.suppressed) return;
    const next = openOnPick(saved, app.workspaceId, id);
    app.selectWorkspace(id);
    if (next) saveSettings({ spacesOpen: { ...settings.spacesOpen, [hostKey(app.conn.target)]: next } });
  }

  // The daemon keeps the folded state per host. Flipped locally first so the click feels instant.
  async function toggleFolder(id: string) {
    if (drag.suppressed) return;
    if (q) return;
    const f = app.ws.folders.find((x) => x.id === id);
    if (!f) return;
    app.ws = { ...app.ws, folders: app.ws.folders.map((x) => (x.id === id ? { ...x, collapsed: !x.collapsed } : x)) };
    await app.api.patchFolder(id, { collapsed: !f.collapsed }).catch(() => {});
  }

  const kidsFor = (w: Workspace | null) =>
    w && !w.parentId
      ? {
          isOpen: (id: string) => opened.has(id),
          isClosedOpen: (id: string) => closedOpen.has(id),
          selectedId: app.workspaceId,
          tabDrop: app.tabDrop,
          sessionDrop: drag.over?.type === 'workspace' && drag.over.pos === 'inside' ? drag.over.id : null,
          ontoggle: toggle,
          onpick: pick,
          onclosed: toggleClosed,
          onmenu: (e: MouseEvent, c: Workspace) => app.openMenu(e, c.name, workspaceMenu(c)),
          onstart: (e: MouseEvent, c: Workspace) => !drag.suppressed && startIn(e, c),
          onnew: w.path && w.kind !== 'subject' && app.wsPersistent ? () => app.open('workspaceNew', w.id) : undefined,
          tabsOf,
        }
      : undefined;

  const select = (key: string, e?: MouseEvent) => !drag.suppressed && selectSession(key, e);

  function toggleClosed(id: string) {
    const next = new Set(closedOpen);
    if (!next.delete(id)) next.add(id);
    closedOpen = next;
  }
</script>

<svelte:window onpointermove={drag.move} onpointerup={drag.end} onpointercancel={drag.cancel} />

<div class="spaces" class:dragging={drag.dragging}>
  <AttentionBanner entries={banner} pathOf={(e) => trail(e.workspaceId)} onselect={(e) => app.select(e.key)} />
  {#each nodes as n (n.key)}
    <div class="node" class:nested={n.depth > 0} style:--depth={n.depth}>
      {#if n.type === 'folder'}
        {@const f = n.folder}
        <SpaceFolder
          node={n}
          ontoggle={() => f && toggleFolder(f.id)}
          onmenu={f ? (e) => app.openMenu(e, f.name, folderMenu(f)) : undefined}
          dropPos={f ? drag.posOf('folder', f.id) : null}
          target={drag.over?.type === 'pinned'}
          onpointerdown={f ? (e) => drag.start(e, 'folder', f.id) : undefined}
        />
      {:else if n.type === 'space'}
        {@const r = n.row}
        <SpaceBlock
          row={r}
          {now}
          {host}
          open={opened.has(r.id)}
          closedOpen={closedOpen.has(r.id)}
          selectedKey={app.selectedKey}
          multi={app.multi}
          selected={app.workspaceId === r.id}
          tabDrop={!!r.workspace && app.tabDrop === r.id}
          dropPos={drag.posOf('workspace', r.id)}
          kids={kidsFor(r.workspace)}
          ontoggle={() => toggle(r.id)}
          onpick={() => pick(r.id)}
          onclosed={() => toggleClosed(r.id)}
          onselect={select}
          tabs={tabsOf(r.id)}
          {rowDrop}
          onsessiondown={(e, key, zone) => drag.start(e, 'session', key, zone)}
          onitemmenu={itemMenuAt}
          onmenu={(e) => (r.workspace ? app.openMenu(e, r.workspace.name, workspaceMenu(r.workspace)) : app.openMenu(e, t('ws.other'), otherMenu()))}
          onpointerdown={r.workspace ? (e) => drag.start(e, 'workspace', r.id) : undefined}
        />
      {:else}
        <div class="empty">
          <span class="label">{t('ws.noSession')}</span>
          <div class="chips">
            {#each n.workspaces as w (w.id)}
              <button
                class="chip"
                class:selected={app.workspaceId === w.id}
                class:archived={w.archived}
                class:missing={w.missing}
                title={w.missing ? (w.path ?? undefined) : undefined}
                class:tab-drop={app.tabDrop === w.id}
                class:drop-before={drag.posOf('workspace', w.id) === 'before'}
                class:drop-after={drag.posOf('workspace', w.id) === 'after'}
                type="button"
                data-workspace-id={w.id}
                data-drop="workspace"
                data-id={w.id}
                data-axis="x"
                onclick={() => pick(w.id)}
                oncontextmenu={(e) => (e.preventDefault(), app.openMenu(e, w.name, workspaceMenu(w)))}
                onpointerdown={(e) => drag.start(e, 'workspace', w.id)}>{w.name}</button
              >
            {/each}
          </div>
        </div>
      {/if}
    </div>
  {/each}
  {#if drag.dragging && drag.kind !== 'session' && !pinned}<div class="ghost" data-drop="pinned" class:target={drag.over?.type === 'pinned'}>{t('ws.dropToPin')}</div>{/if}
  {#if !nodes.length}<p class="none faint">{q ? t('common.none') : t('ws.empty')}</p>{/if}
  {#if !q && app.connected}
    <div class="gtitle">
      <span>{t('grp.title')}</span>
      <button class="add" type="button" title={t('grp.new')} aria-label={t('grp.new')} onclick={() => createGroup([...app.multi])}><Plus size={12} /></button>
    </div>
    {#if !groups.length}<p class="hint faint">{t('grp.hint')}</p>{/if}
    {#each groups as g (g.id)}
      {@const agg = aggregate(app.groupItems(g.id))}
      <button
        class="group"
        class:selected={app.workspaceId === `group:${g.id}`}
        type="button"
        onclick={() => app.selectWorkspace(`group:${g.id}`)}
        oncontextmenu={(e) => (e.preventDefault(), app.openMenu(e, g.name, groupMenu(g)))}
      >
        <span class="gicon"><Users size={12} /></span>
        <span class="gname">{g.name}</span>
        <span class="dot" style:background={WS_COLOR[agg.state]}></span>
        <span class="count">{g.members.length}</span>
      </button>
    {/each}
  {/if}
  {#if drag.dragging && drag.kind !== 'session'}<div class="rootdrop" data-drop="root"></div>{/if}
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
  .chip {
    position: relative;
  }
  .chip:hover {
    background: var(--surface-hover);
  }
  .chip.selected {
    background: var(--accent-soft);
    color: var(--text);
  }
  .chip.tab-drop {
    background: var(--accent-soft);
    box-shadow: inset 0 0 0 1px var(--accent);
  }
  .chip.missing {
    color: var(--text-3);
  }
  .chip.archived {
    opacity: 0.55;
  }
  .chip.drop-before::before,
  .chip.drop-after::after {
    content: '';
    position: absolute;
    top: 2px;
    bottom: 2px;
    width: 2px;
    background: var(--accent);
  }
  .chip.drop-before::before {
    left: -3px;
  }
  .chip.drop-after::after {
    right: -3px;
  }
  .dragging {
    cursor: grabbing;
    -webkit-user-select: none;
    user-select: none;
  }
  .ghost {
    margin: 6px 8px;
    padding: 4px 8px;
    border: 1px dashed var(--accent);
    border-radius: var(--radius);
    color: var(--accent);
    font-size: 11px;
    font-weight: 500;
    text-align: center;
  }
  .ghost.target {
    background: var(--accent-soft);
  }
  .rootdrop {
    flex: 1;
    min-height: 40px;
  }
  .gtitle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 12px 2px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-2);
  }
  .add {
    display: inline-grid;
    place-items: center;
    width: 20px;
    height: 20px;
    border-radius: var(--radius);
    color: var(--text-3);
  }
  .add:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .hint {
    margin: 0;
    padding: 2px 12px 6px;
    font-size: 12px;
    line-height: 1.4;
  }
  .group {
    display: flex;
    align-items: center;
    gap: 6px;
    height: 28px;
    margin: 0 6px;
    padding: 0 6px;
    border-radius: 6px;
    color: var(--text-2);
    font-weight: 500;
    text-align: left;
  }
  .group:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .group.selected {
    background: var(--accent-soft);
    color: var(--text);
  }
  .gicon {
    display: inline-grid;
    color: var(--text-3);
  }
  .gname {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .count {
    font-size: 11px;
    color: var(--text-3);
    font-variant-numeric: tabular-nums;
  }
  .dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
  }
  .none {
    padding: 24px 12px;
    text-align: center;
  }
  :global(:root.phone) .empty {
    padding: 4px 6px;
  }
  :global(:root.phone) .label {
    font-size: 12px;
  }
  :global(:root.phone) .chips {
    gap: 6px;
    margin-top: 6px;
  }
  :global(:root.phone) .chip {
    height: 40px;
    padding: 0 14px;
    border-radius: 20px;
    font-size: 14px;
  }
  :global(:root.phone) .gtitle {
    padding: 8px 6px 0 12px;
    font-size: 12px;
  }
  :global(:root.phone) .add {
    width: var(--touch);
    height: var(--touch);
  }
  :global(:root.phone) .group {
    flex: none;
    gap: 10px;
    height: var(--touch);
    padding: 0 10px;
    border-radius: 10px;
  }
</style>
