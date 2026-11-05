<script lang="ts">
  import { untrack } from 'svelte';
  import { t } from '../../i18n.svelte';
  import { app, NO_WORKSPACE } from '../../state.svelte';
  import { activeKey, addTabs, allTabs, dropKey, EMPTY_LAYOUT, layoutFromTabs, openInFocused, renameTab, setRatio, split, switchLayout, update, workspace, type Layout } from '../../workspace.svelte';
  import { isKnown, loadKnown, mergeKnown, saveKnown, startKnown } from '../../known-tabs';
  import { hostKey } from '../../window';
  import { pages } from '../../session-pages.svelte';
  import type { SidebarItem } from '../../grouping';
  import type { Workspace } from '../../types';
  import { aggregate, resolveTab, workspaceTabs } from '../../workspaces';
  import { tabInfo } from '../../tabs';
  import PaneView from './PaneView.svelte';

  let root: HTMLDivElement;
  let dragging = $state(false);

  const layoutKey = $derived(app.sidebarMode === 'status' ? '#status' : app.workspaceId === NO_WORKSPACE ? '#other' : app.workspaceId);
  const members = $derived(
    app.sidebarMode === 'status' ? [] : app.workspaceId.startsWith('group:') ? app.groupItems(app.workspaceId.slice(6)) : app.membersOf(app.workspaceId),
  );
  const loaded = $derived(app.connected && app.synced && app.data.lines.length + app.data.terms.length > 0);
  let current = '#status';
  const stale = new Set<string>();
  const localTabs = new Map<string, string>();
  const knownBy = new Map<string, Set<string> | undefined>();
  let persistTimer: ReturnType<typeof setTimeout> | undefined;
  let pending: { id: string; tabs: string[] } | null = null;

  const host = () => hostKey(app.conn.target);
  const localOnly = (key: string) => key === '#other' || key.startsWith('group:');
  const savedTabs = (key: string, ws: Workspace | null) => (localOnly(key) ? (loadKnown(host(), key)?.tabs ?? []) : (ws?.tabs ?? []));

  function store(key: string, known: Set<string>) {
    const tabs = localOnly(key) ? allTabs(workspace.layout) : undefined;
    if (tabs) localTabs.set(`${host()}|${key}`, tabs.join('\n'));
    saveKnown(host(), key, { known: [...known], tabs });
  }

  function build(key: string, ws: Workspace | null, mem: SidebarItem[], items: SidebarItem[], ready: boolean): Layout {
    if (key === '#status') return EMPTY_LAYOUT;
    if (ready) stale.delete(key);
    else stale.add(key);
    const saved = savedTabs(key, ws);
    const id = `${host()}|${key}`;
    if (ready && !knownBy.has(id)) knownBy.set(id, startKnown(loadKnown(host(), key), saved, mem));
    return layoutFromTabs(workspaceTabs(saved, mem, items, knownBy.get(id)), aggregate(mem).statusKey ?? null);
  }

  $effect(() => {
    const key = layoutKey;
    const mem = members;
    const items = app.items;
    const ws = app.currentWorkspace;
    const ready = loaded;
    untrack(() => {
      if (key !== current) {
        switchLayout(current, key, () => build(key, ws, mem, items, ready));
        current = key;
        const want = app.selectedKey ? resolveTab(app.selectedKey, items) : null;
        if (key !== '#status' && want && mem.some((m) => m.key === want)) {
          update((l) => openInFocused(l, want));
          app.selectedKey = want;
        }
        else if (key !== '#status') app.selectedKey = activeKey(workspace.layout);
        else if (app.selectedKey) {
          const sel = app.selectedKey;
          update((l) => openInFocused(l, sel));
        }
      }
      if (key === '#status' || !ready) return;
      if (stale.has(key)) {
        const sel = app.selectedKey;
        workspace.layout = build(key, ws, mem, items, ready);
        if (sel && allTabs(workspace.layout).includes(sel)) update((l) => openInFocused(l, sel));
        else app.selectedKey = activeKey(workspace.layout);
      }
      const id = `${host()}|${key}`;
      if (!knownBy.has(id)) knownBy.set(id, startKnown(loadKnown(host(), key), savedTabs(key, ws), mem));
      const prev = knownBy.get(id);
      const fresh = mem.filter((m) => m.group !== 'held' && !(prev && isKnown(m, prev))).map((m) => m.key);
      const next = mergeKnown(prev ?? new Set(), mem, items);
      knownBy.set(id, next);
      for (const k of allTabs(workspace.layout)) {
        if (!k.startsWith('term:') && !k.startsWith('line:')) continue;
        const r = resolveTab(k, items);
        if (r && r !== k) update((l) => renameTab(l, k, r));
        else if (!r && !tabInfo(k, items)) update((l) => dropKey(l, k));
      }
      if (fresh.length) update((l) => addTabs(l, fresh));
      if (!prev || prev.size !== next.size || [...next].some((k) => !prev.has(k))) store(key, next);
    });
  });

  $effect(() => {
    const key = app.selectedKey;
    if (key) untrack(() => update((l) => openInFocused(l, key)));
  });

  $effect(() => {
    const tabs = allTabs(workspace.layout);
    const key = layoutKey;
    if (!localOnly(key) || current !== key || stale.has(key)) return;
    untrack(() => {
      const id = `${host()}|${key}`;
      const known = knownBy.get(id);
      if (!localTabs.has(id)) localTabs.set(id, (loadKnown(host(), key)?.tabs ?? []).join('\n'));
      if (known && localTabs.get(id) !== tabs.join('\n')) store(key, known);
    });
  });

  function flush() {
    clearTimeout(persistTimer);
    const p = pending;
    pending = null;
    if (p) app.patchWorkspace(p.id, { tabs: p.tabs });
    pages.flush();
  }

  $effect(() => {
    const h = host();
    untrack(() => pages.useHost(h));
  });
  $effect(() => {
    const tabs = allTabs(workspace.layout);
    const ws = app.currentWorkspace;
    if (!ws || !app.wsPersistent || layoutKey !== ws.id || current !== ws.id || stale.has(ws.id)) return;
    if (tabs.length === ws.tabs.length && tabs.every((k, i) => k === ws.tabs[i])) return;
    clearTimeout(persistTimer);
    pending = { id: ws.id, tabs };
    persistTimer = setTimeout(flush, 800);
  });

  $effect(() => {
    const hidden = () => {
      if (document.visibilityState === 'hidden') flush();
    };
    window.addEventListener('pagehide', flush);
    document.addEventListener('visibilitychange', hidden);
    return () => {
      window.removeEventListener('pagehide', flush);
      document.removeEventListener('visibilitychange', hidden);
      flush();
    };
  });

  const layout = $derived(workspace.layout);
  const row = $derived(layout.split !== 'vertical');

  function startDrag(e: PointerEvent) {
    dragging = true;
    (e.currentTarget as HTMLElement).setPointerCapture(e.pointerId);
  }

  function drag(e: PointerEvent) {
    if (!dragging) return;
    const r = root.getBoundingClientRect();
    update((l) => setRatio(l, row ? (e.clientX - r.left) / r.width : (e.clientY - r.top) / r.height));
  }

  function onKey(e: KeyboardEvent) {
    const step = e.key === 'ArrowLeft' || e.key === 'ArrowUp' ? -0.05 : e.key === 'ArrowRight' || e.key === 'ArrowDown' ? 0.05 : 0;
    if (step) update((l) => setRatio(l, l.ratio + step));
  }

  function onSplit(e: Event) {
    const dir = (e as CustomEvent).detail;
    if (dir === 'horizontal' || dir === 'vertical') update((l) => split(l, dir));
  }

  $effect(() => {
    window.addEventListener('sb:split', onSplit);
    return () => window.removeEventListener('sb:split', onSplit);
  });
</script>

<div class="ws" class:col={!row} class:dragging bind:this={root}>
  {#each layout.panes as pane, i (i)}
    <div class="slot" style:flex-basis={layout.panes.length > 1 ? `${(i === 0 ? layout.ratio : 1 - layout.ratio) * 100}%` : '100%'}>
      <PaneView {pane} index={i} focused={layout.focused === i} canClose={layout.panes.length > 1} />
    </div>
    {#if i === 0 && layout.panes.length > 1}
      <button
        class="divider"
        aria-label={t('title.layout')}
        onpointerdown={startDrag}
        onpointermove={drag}
        onpointerup={() => (dragging = false)}
        onkeydown={onKey}
      ></button>
    {/if}
  {/each}
</div>

<style>
  .ws {
    flex: 1;
    display: flex;
    min-height: 0;
    min-width: 0;
  }
  .ws.col {
    flex-direction: column;
  }
  .slot {
    display: flex;
    min-width: 0;
    min-height: 0;
    flex-grow: 0;
    flex-shrink: 1;
  }
  .divider {
    flex: none;
    width: 1px;
    background: var(--border);
    cursor: col-resize;
    position: relative;
    transition: background var(--ease);
  }
  .divider::after {
    content: '';
    position: absolute;
    inset: 0 -3px;
  }
  .col .divider {
    width: auto;
    height: 1px;
    cursor: row-resize;
  }
  .col .divider::after {
    inset: -3px 0;
  }
  .divider:hover,
  .dragging .divider {
    background: var(--accent);
  }
  .dragging {
    user-select: none;
    -webkit-user-select: none;
  }
  .dragging :global(.term) {
    pointer-events: none;
  }
</style>
