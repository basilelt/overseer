<script lang="ts">
  import ChevronRight from '@lucide/svelte/icons/chevron-right';
  import Users from '@lucide/svelte/icons/users';
  import Plus from '@lucide/svelte/icons/plus';
  import { t } from '../../i18n.svelte';
  import { app, NO_WORKSPACE } from '../../state.svelte';
  import type { Workspace } from '../../types';
  import { aggregate, buildTree, byRank, planFolderDrop, planWorkspaceDrop, type DropPos, type DropTarget } from '../../workspaces';
  import { createGroup, folderMenu, groupMenu, otherMenu, workspaceMenu } from '../../wsactions';
  import WorkspaceRow, { WS_COLOR } from './WorkspaceRow.svelte';

  let { filter, showArchived }: { filter: string; showArchived: boolean } = $props();

  const q = $derived(filter.trim().toLowerCase());
  const match = (w: Workspace) =>
    w.name.toLowerCase().includes(q) || w.path.toLowerCase().includes(q) || app.membersOf(w.id).some((i) => i.title.toLowerCase().includes(q));
  const tree = $derived(buildTree(app.ws, { showArchived, filter: q ? match : undefined }));
  const others = $derived(app.membersOf(NO_WORKSPACE));
  const groups = $derived(app.ws.groups.slice().sort(byRank));

  let drag: { kind: 'workspace' | 'folder'; id: string; x: number; y: number; active: boolean } | null = null;
  let over = $state<DropTarget | null>(null);
  let dragging = $state(false);
  let suppressClick = false;

  function start(e: PointerEvent, kind: 'workspace' | 'folder', id: string) {
    if (e.button !== 0 || !app.wsPersistent) return;
    drag = { kind, id, x: e.clientX, y: e.clientY, active: false };
  }

  function targetAt(x: number, y: number): DropTarget | null {
    const el = (document.elementFromPoint(x, y) as HTMLElement | null)?.closest<HTMLElement>('[data-drop]');
    if (!el) return null;
    const type = el.dataset.drop;
    if (type === 'pinned') return { type: 'pinned' };
    if (type === 'root') return { type: 'root' };
    const r = el.getBoundingClientRect();
    const f = (y - r.top) / r.height;
    const id = el.dataset.id!;
    if (type === 'folder') {
      const pos: DropPos = f < 0.25 ? 'before' : f > 0.75 && drag?.kind === 'folder' ? 'after' : 'inside';
      return { type: 'folder', id, pos };
    }
    return { type: 'workspace', id, pos: f < 0.5 ? 'before' : 'after' };
  }

  function move(e: PointerEvent) {
    if (!drag) return;
    if (!drag.active && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 5) return;
    drag.active = true;
    dragging = true;
    over = targetAt(e.clientX, e.clientY);
  }

  async function end() {
    const d = drag;
    drag = null;
    dragging = false;
    const target = over;
    over = null;
    if (!d?.active) return;
    suppressClick = true;
    setTimeout(() => (suppressClick = false), 0);
    if (!target) return;
    if (d.kind === 'workspace') {
      const patch = planWorkspaceDrop(app.ws, d.id, target);
      if (patch) await app.patchWorkspace(d.id, patch);
    } else {
      const patch = planFolderDrop(app.ws, d.id, target);
      if (!patch) return;
      const prev = app.ws;
      app.ws = { ...prev, folders: prev.folders.map((f) => (f.id === d.id ? { ...f, ...patch } : f)) };
      try {
        const next = await app.api.patchFolder(d.id, patch);
        app.ws = { ...app.ws, folders: app.ws.folders.map((f) => (f.id === next.id ? next : f)) };
      } catch (err) {
        app.ws = prev;
        app.notify(String((err as Error)?.message ?? err), 'error');
      }
    }
  }

  function pick(id: string) {
    if (suppressClick) return;
    app.selectWorkspace(id);
  }

  async function toggleFolder(id: string) {
    if (suppressClick) return;
    const f = app.ws.folders.find((x) => x.id === id);
    if (!f) return;
    app.ws = { ...app.ws, folders: app.ws.folders.map((x) => (x.id === id ? { ...x, collapsed: !x.collapsed } : x)) };
    await app.api.patchFolder(id, { collapsed: !f.collapsed }).catch(() => {});
  }

  const posOf = (type: 'workspace' | 'folder', id: string) => (over && over.type === type && over.id === id ? over.pos : null);
</script>

<svelte:window onpointermove={move} onpointerup={end} onpointercancel={end} />

<div class="tree" class:dragging role="tree" aria-label={t('ws.title')}>
  {#if tree.pinned.length || (dragging && over?.type === 'pinned')}
    <div class="section" data-drop="pinned" class:target={over?.type === 'pinned'}>{t('ws.pinned')}</div>
    {#each tree.pinned as w (w.id)}
      <div data-drop="workspace" data-id={w.id}>
        <WorkspaceRow
          id={w.id}
          name={w.name}
          path={w.path}
          agg={app.aggregates.get(w.id)}
          selected={app.workspaceId === w.id}
          pinned
          archived={w.archived}
          missing={w.missing}
          dropPos={posOf('workspace', w.id)}
          onselect={() => pick(w.id)}
          onmenu={(e) => app.openMenu(e, w.name, workspaceMenu(w))}
          onpointerdown={(e) => start(e, 'workspace', w.id)}
        />
      </div>
    {/each}
  {:else if dragging}
    <div class="section ghost" data-drop="pinned">{t('ws.dropToPin')}</div>
  {/if}

  {#if tree.nodes.length}<div class="section" data-drop="root" class:target={over?.type === 'root'}>{t('ws.title')}</div>{/if}
  {#each tree.nodes as node (node.type === 'folder' ? `f:${node.folder.id}` : `w:${node.workspace.id}`)}
    {#if node.type === 'folder'}
      {@const f = node.folder}
      {@const pos = posOf('folder', f.id)}
      <button
        class="folder"
        class:inside={pos === 'inside'}
        class:drop-before={pos === 'before'}
        class:drop-after={pos === 'after'}
        style:--indent="{node.depth * 12}px"
        data-drop="folder"
        data-id={f.id}
        aria-expanded={!f.collapsed}
        tabindex="-1"
        onclick={() => toggleFolder(f.id)}
        oncontextmenu={(e) => (e.preventDefault(), app.openMenu(e, f.name, folderMenu(f)))}
        onpointerdown={(e) => start(e, 'folder', f.id)}
      >
        <span class="chev" class:open={!f.collapsed || !!q}><ChevronRight size={12} /></span>
        <span class="fname">{f.name}</span>
        <span class="count">{node.count}</span>
      </button>
    {:else}
      {@const w = node.workspace}
      <div data-drop="workspace" data-id={w.id}>
        <WorkspaceRow
          id={w.id}
          name={w.name}
          path={w.path}
          agg={app.aggregates.get(w.id)}
          selected={app.workspaceId === w.id}
          archived={w.archived}
          missing={w.missing}
          depth={node.depth}
          dropPos={posOf('workspace', w.id)}
          onselect={() => pick(w.id)}
          onmenu={(e) => app.openMenu(e, w.name, workspaceMenu(w))}
          onpointerdown={(e) => start(e, 'workspace', w.id)}
        />
      </div>
    {/if}
  {/each}

  {#if others.length && !q}
    <WorkspaceRow name={t('ws.other')} agg={aggregate(others)} selected={app.workspaceId === NO_WORKSPACE} onselect={() => pick(NO_WORKSPACE)} onmenu={(e) => app.openMenu(e, t('ws.other'), otherMenu())} />
  {/if}

  {#if !q && app.connected}
    <div class="section head">
      <span>{t('grp.title')}</span>
      <button class="add" title={t('grp.new')} aria-label={t('grp.new')} onclick={() => createGroup([...app.multi])}><Plus size={12} /></button>
    </div>
    {#if !groups.length}<p class="hint faint">{t('grp.hint')}</p>{/if}
    {#each groups as g (g.id)}
      {@const agg = aggregate(app.groupItems(g.id))}
      <button
        class="group"
        class:selected={app.workspaceId === `group:${g.id}`}
        tabindex="-1"
        onclick={() => app.selectWorkspace(`group:${g.id}`)}
        oncontextmenu={(e) => (e.preventDefault(), app.openMenu(e, g.name, groupMenu(g)))}
      >
        <span class="gicon"><Users size={12} /></span>
        <span class="fname">{g.name}</span>
        <span class="dot" style:background={WS_COLOR[agg.state]}></span>
        <span class="count">{g.members.length}</span>
      </button>
    {/each}
  {/if}

  {#if !tree.nodes.length && !tree.pinned.length && !others.length}
    <p class="empty faint">{q ? t('common.none') : t('ws.empty')}</p>
  {/if}
  {#if dragging}<div class="rootdrop" data-drop="root"></div>{/if}
</div>

<style>
  .tree {
    flex: 1;
    overflow-y: auto;
    padding: 0 6px 8px;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .dragging {
    cursor: grabbing;
  }
  .section.head {
    display: flex;
    align-items: center;
    justify-content: space-between;
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
    padding: 2px 8px 6px;
    font-size: 12px;
    line-height: 1.4;
  }
  .section {
    padding: 10px 8px 4px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--text-3);
    border-radius: var(--radius);
  }
  .section.target,
  .section.ghost {
    color: var(--accent);
  }
  .section.ghost {
    border: 1px dashed var(--accent);
    text-transform: none;
    letter-spacing: 0;
    font-weight: 500;
    text-align: center;
    margin: 6px 2px;
  }
  .folder,
  .group {
    position: relative;
    display: flex;
    align-items: center;
    gap: 6px;
    width: 100%;
    height: 28px;
    padding: 0 8px 0 calc(4px + var(--indent, 0px));
    border-radius: var(--radius);
    color: var(--text-2);
    font-weight: 500;
    text-align: left;
  }
  .folder:hover,
  .group:hover {
    background: var(--surface-hover);
    color: var(--text);
  }
  .group.selected {
    background: var(--accent-soft);
    color: var(--text);
  }
  .folder.inside {
    box-shadow: inset 0 0 0 1px var(--accent);
    background: var(--accent-soft);
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
  .chev {
    display: inline-grid;
    transition: transform var(--ease);
    color: var(--text-3);
  }
  .chev.open {
    transform: rotate(90deg);
  }
  .gicon {
    display: inline-grid;
    color: var(--text-3);
    padding-left: 2px;
  }
  .fname {
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
  .empty {
    padding: 24px 12px;
    text-align: center;
  }
  .rootdrop {
    flex: 1;
    min-height: 40px;
  }
</style>
