import { t, type Key } from '../i18n.svelte';
import { app } from '../state.svelte';
import { layoutKeyOf, moveSavedTab, moveTab } from '../tabs/tab-order';
import type { Folder, Workspace } from '../types';
import { savedLayouts, update } from './workspace.svelte';
import { folderDropRefusal, planFolderDrop, planWorkspaceDrop, resolveTab, type DropPos, type DropTarget, type WsData } from './workspaces';

export type SessionTarget = { type: 'session'; id: string; pos: 'before' | 'after' };

export interface DragDeps {
  persistent: () => boolean;
  data: () => WsData;
  elementAt: (x: number, y: number) => Element | null;
  patchWorkspace: (id: string, patch: Partial<Workspace>) => Promise<unknown>;
  patchFolder: (id: string, patch: Partial<Folder>) => Promise<unknown>;
  moveSession: (key: string, workspaceId: string) => Promise<unknown>;
  canMove: (key: string) => boolean;
  reorder: (zone: string, key: string, anchor: string, pos: 'before' | 'after') => void;
  refuse: (reason: Key) => void;
}

async function patchFolder(id: string, patch: Partial<Folder>) {
  const prev = app.ws;
  app.ws = { ...prev, folders: prev.folders.map((f) => (f.id === id ? { ...f, ...patch } : f)) };
  try {
    const next = await app.api.patchFolder(id, patch);
    app.ws = { ...app.ws, folders: app.ws.folders.map((f) => (f.id === next.id ? next : f)) };
  } catch (err) {
    app.ws = prev;
    app.notify(String((err as Error)?.message ?? err), 'error');
  }
}

function reorder(zone: string, key: string, anchor: string, pos: 'before' | 'after') {
  if (app.sidebarMode === 'workspaces' && zone === app.workspaceId) return update((l) => moveTab(l, key, anchor, pos));
  const lk = layoutKeyOf(zone);
  const cached = savedLayouts.get(lk);
  if (cached) savedLayouts.set(lk, moveTab(cached, key, anchor, pos));
  const ws = app.ws.workspaces.find((w) => w.id === zone);
  if (!ws) return;
  const tabs = moveSavedTab(ws.tabs.map((k) => resolveTab(k, app.items)).filter((k): k is string => !!k), key, anchor, pos);
  if (tabs) void app.patchWorkspace(zone, { tabs });
}

const appDeps = (): DragDeps => ({
  persistent: () => app.wsPersistent,
  data: () => app.ws,
  elementAt: (x, y) => document.elementFromPoint(x, y),
  patchWorkspace: (id, patch) => app.patchWorkspace(id, patch),
  patchFolder,
  moveSession: (key, workspaceId) => app.moveToWorkspace(key, workspaceId),
  canMove: (key) => app.canMoveToWorkspace(key),
  reorder,
  refuse: (reason) => app.notify(t(reason), 'error'),
});

// Pointer drag of workspaces, folders and sessions in the sidebar; drop zones carry data-drop="workspace|child|folder|pinned|root|session"
// and data-id. A nested workspace line ("child") only takes sessions; a session row (data-zone = its workspace) only takes a session of the same workspace.
export class WsDrag {
  over = $state<DropTarget | SessionTarget | null>(null);
  dragging = $state(false);
  kind = $state<'workspace' | 'folder' | 'session' | null>(null);
  /** True for the click that follows a drag, so releasing over a header does not select or fold it. */
  suppressed = false;
  #deps: DragDeps;
  #drag: { kind: 'workspace' | 'folder' | 'session'; id: string; zone?: string; x: number; y: number; active: boolean } | null = null;

  constructor(deps: DragDeps = appDeps()) {
    this.#deps = deps;
  }

  start = (e: PointerEvent, kind: 'workspace' | 'folder' | 'session', id: string, zone?: string) => {
    if (e.button !== 0 || (kind !== 'session' && !this.#deps.persistent())) return;
    this.#drag = { kind, id, zone, x: e.clientX, y: e.clientY, active: false };
  };

  #targetAt(x: number, y: number): DropTarget | SessionTarget | null {
    const el = this.#deps.elementAt(x, y)?.closest<HTMLElement>('[data-drop]');
    if (!el) return null;
    const type = el.dataset.drop;
    const d = this.#drag;
    if (d?.kind === 'session') {
      if (type === 'session') {
        if (el.dataset.zone !== d.zone || el.dataset.id === d.id) return null;
        const r = el.getBoundingClientRect();
        return { type: 'session', id: el.dataset.id!, pos: y - r.top < r.height / 2 ? 'before' : 'after' };
      }
      return (type === 'workspace' || type === 'child') && this.#deps.canMove(d.id) ? { type: 'workspace', id: el.dataset.id!, pos: 'inside' } : null;
    }
    if (type === 'session') return null;
    if (type === 'child') return null;
    if (type === 'pinned') return { type: 'pinned' };
    if (type === 'root') return { type: 'root' };
    const r = el.getBoundingClientRect();
    const f = el.dataset.axis === 'x' ? (x - r.left) / r.width : (y - r.top) / r.height;
    const id = el.dataset.id!;
    if (type === 'folder') {
      const pos: DropPos = f < 0.25 ? 'before' : f > 0.75 && this.#drag?.kind === 'folder' ? 'after' : 'inside';
      return { type: 'folder', id, pos };
    }
    return { type: 'workspace', id, pos: f < 0.5 ? 'before' : 'after' };
  }

  move = (e: PointerEvent) => {
    const d = this.#drag;
    if (!d) return;
    if (!d.active && Math.hypot(e.clientX - d.x, e.clientY - d.y) < 5) return;
    d.active = true;
    this.dragging = true;
    this.kind = d.kind;
    this.over = this.#targetAt(e.clientX, e.clientY);
  };

  end = async () => {
    const d = this.#drag;
    const target = this.over;
    this.#drag = null;
    this.dragging = false;
    this.kind = null;
    this.over = null;
    if (!d?.active) return;
    this.suppressed = true;
    setTimeout(() => (this.suppressed = false), 0);
    if (!target) return;
    if (target.type === 'session') {
      if (d.zone !== undefined) this.#deps.reorder(d.zone, d.id, target.id, target.pos);
    } else if (d.kind === 'session') {
      if (target.type === 'workspace') await this.#deps.moveSession(d.id, target.id);
    } else if (d.kind === 'workspace') {
      const patch = planWorkspaceDrop(this.#deps.data(), d.id, target);
      if (patch) await this.#deps.patchWorkspace(d.id, patch);
    } else {
      const patch = planFolderDrop(this.#deps.data(), d.id, target);
      if (patch) await this.#deps.patchFolder(d.id, patch);
      else {
        const reason = folderDropRefusal(this.#deps.data(), d.id, target);
        if (reason) this.#deps.refuse(reason);
      }
    }
  };

  cancel = () => {
    this.#drag = null;
    this.dragging = false;
    this.kind = null;
    this.over = null;
  };

  posOf = (type: 'workspace' | 'folder' | 'session', id: string): DropPos | null => (this.over && this.over.type === type && this.over.id === id ? this.over.pos : null);
}
