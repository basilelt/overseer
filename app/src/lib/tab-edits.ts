// Renaming a tab and moving it to a workspace, loaded on first use to keep them out of the startup bundle.
import FolderInput from '@lucide/svelte/icons/folder-input';
import X from '@lucide/svelte/icons/x';
import { t } from './i18n.svelte';
import type { DaemonClient } from './client';
import type { SidebarItem } from './grouping';
import type { Term } from './types';
import { cleanTitle } from './titles';
import type { MenuItem } from './menu';
import type { Snapshot } from './reduce';
import type { app as appState } from './state.svelte';
import { activeKey, addTabs, allTabs, closeTabAt, dropKey, savedLayouts as saved, update, workspace } from './workspace.svelte';
import { itemMenu } from './wsactions';
import { buildTree, resolveTab } from './workspaces';
import { endTarget, exitedTerm } from './tabs';

type App = typeof appState;

const enc = encodeURIComponent;

/** Daemon calls for custom titles and workspace assignments (PUT /v1/lines|terms/{id}/title|workspace). */
export function tabApi(c: DaemonClient) {
  return {
    forceClose: (id: string) => c.request('POST', `/v1/lines/${enc(id)}/close?dryRun=false&force=true`),
    renameLine: (id: string, title: string | null) => c.request<{ lineId: string; titleOverride: string | null }>('PUT', `/v1/lines/${enc(id)}/title`, { title }),
    renameTerm: (id: string, title: string | null) => c.request<Term>('PUT', `/v1/terms/${enc(id)}/title`, { title }),
    assignLineWorkspace: (id: string, workspaceId: string | null) => c.request<{ lineId: string; workspaceId: string | null }>('PUT', `/v1/lines/${enc(id)}/workspace`, { workspaceId }),
    assignTermWorkspace: (id: string, workspaceId: string | null) => c.request<Term>('PUT', `/v1/terms/${enc(id)}/workspace`, { workspaceId }),
  };
}

function client(app: App): ReturnType<typeof tabApi> {
  if (!app.client) throw new Error('not connected');
  return tabApi(app.client);
}

export function assignWorkspace<S extends Pick<Snapshot, 'lines' | 'terms'>>(snap: S, item: SidebarItem, workspaceId: string | null): S {
  if (item.lineId) return { ...snap, lines: snap.lines.map((l) => (l.line.id === item.lineId ? { ...l, local: { ...l.local, workspaceId } } : l)) };
  if (item.termId) return { ...snap, terms: snap.terms.map((t) => (t.id === item.termId ? { ...t, workspaceId } : t)) };
  return snap;
}

// Moves a tab out of the current layout into the saved layout of `to` (if any); returns the undo.
export function moveTabTo(key: string, to: string): () => void {
  update((l) => dropKey(l, key));
  const target = saved.get(to);
  const had = !!target && allTabs(target).includes(key);
  if (target && !had) saved.set(to, addTabs(target, [key]));
  return () => {
    if (!allTabs(workspace.layout).includes(key)) update((l) => addTabs(l, [key]));
    const t = saved.get(to);
    if (t && !had) saved.set(to, dropKey(t, key));
  };
}

export function moveMenu(app: App, key: string): MenuItem[] {
  const tree = buildTree(app.ws, { filter: () => true });
  const current = app.itemWs.get(key);
  return [...tree.pinned, ...tree.nodes.flatMap((n) => (n.type === 'workspace' ? [n.workspace] : []))].map((w) => ({
    label: w.name,
    icon: FolderInput,
    checked: w.id === current,
    disabled: w.id === current,
    run: () => void app.moveToWorkspace(key, w.id),
  }));
}

/** Set (or clear with null/blank) the custom title of a session or plain terminal; optimistic, reverted on failure. */
export async function renameItem(app: App, key: string, raw: string | null) {
  const item = app.items.find((i) => i.key === key);
  const title = raw === null ? null : cleanTitle(raw);
  const setLine = (id: string, v: string | null | undefined) =>
    (app.data = { ...app.data, lines: app.data.lines.map((l) => (l.line.id === id ? { ...l, local: { ...l.local, titleOverride: v } } : l)) });
  const setTerm = (id: string, v: string | null | undefined) => (app.data = { ...app.data, terms: app.data.terms.map((t) => (t.id === id ? { ...t, titleOverride: v } : t)) });
  try {
    if (item?.lineId) {
      const prev = app.data.lines.find((l) => l.line.id === item.lineId)?.local?.titleOverride;
      setLine(item.lineId, title);
      await client(app).renameLine(item.lineId, title).catch((e) => {
        setLine(item.lineId!, prev);
        throw e;
      });
    } else if (item?.termId) {
      const prev = app.data.terms.find((t) => t.id === item.termId)?.titleOverride;
      setTerm(item.termId, title);
      await client(app).renameTerm(item.termId, title).catch((e) => {
        setTerm(item.termId!, prev);
        throw e;
      });
    }
  } catch (e) {
    app.notify(String((e as Error)?.message ?? e), 'error');
  }
}

export async function moveToWorkspace(app: App, key: string, workspaceId: string): Promise<boolean> {
  const k = resolveTab(key, app.items) ?? key;
  const it = app.items.find((i) => i.key === k);
  if (!it || !app.canMoveToWorkspace(key) || !app.ws.workspaces.some((w) => w.id === workspaceId) || app.itemWs.get(it.key) === workspaceId) return false;
  const prev = it.lineId ? (app.data.lines.find((l) => l.line.id === it.lineId)?.local?.workspaceId ?? null) : (app.data.terms.find((t) => t.id === it.termId)?.workspaceId ?? null);
  const from = app.workspaceId;
  app.data = assignWorkspace(app.data, it, workspaceId);
  const undo = from.startsWith('group:') || app.sidebarMode === 'status' ? null : moveTabTo(it.key, workspaceId);
  if (undo && app.selectedKey === it.key) app.selectedKey = activeKey(workspace.layout);
  try {
    if (it.lineId) await client(app).assignLineWorkspace(it.lineId, workspaceId);
    else await client(app).assignTermWorkspace(it.termId!, workspaceId);
    return true;
  } catch (e) {
    app.data = assignWorkspace(app.data, it, prev);
    if (app.workspaceId === from) undo?.();
    app.notify(String((e as Error)?.message ?? e), 'error');
    return false;
  }
}

export function tabClosed(app: App, key: string, end: boolean) {
  const id = exitedTerm(key, app.data.terms);
  if (id) return dropTerm(app, id);
  const target = end ? endTarget(key, app.items) : null;
  if (!target) return;
  if ('term' in target) {
    const term = target.term;
    ending.add(term);
    return app.api
      .killTerm(term)
      .then(() => reapEnded(app))
      .catch((e) => {
        ending.delete(term);
        app.notify(String((e as Error)?.message ?? e), 'error');
      });
  }
  return client(app)
    .forceClose(target.line)
    .then(() => app.refresh())
    .catch((e) => {
      app.notify(String((e as Error)?.message ?? e), 'error');
      return app.refresh();
    });
}

const ending = new Set<string>();

function dropTerm(app: App, id: string) {
  return app.api
    .deleteTerm(id)
    .then(() => {
      app.data = { ...app.data, terms: app.data.terms.filter((x) => x.id !== id) };
    })
    .catch(() => {});
}

export function reapEnded(app: App, terms = app.data.terms) {
  for (const t of terms) {
    if (!t.exited || !ending.delete(t.id)) continue;
    void dropTerm(app, t.id);
  }
}

export function tabMenu(app: App, e: MouseEvent, index: number, key: string, item: boolean): MenuItem[] {
  const items: MenuItem[] = item ? itemMenu(key) : [];
  if (app.canMoveToWorkspace(key)) items.push({ label: t('ws.moveSession'), icon: FolderInput, run: () => app.openMenu(e, t('ws.moveSession'), moveMenu(app, key)) });
  items.push({ label: '', separator: true });
  if (item) items.push({ label: t('tab.hide'), run: () => closeTabAt(index, key, false) });
  items.push({ label: t('common.close'), icon: X, run: () => closeTabAt(index, key) });
  return items;
}
