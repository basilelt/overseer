import { t, type Key } from './i18n.svelte';
import { app } from './state.svelte';
import type { Folder, Workspace } from './types';
import { folderDropRefusal, planFolderDrop, planWorkspaceDrop, type DropPos, type DropTarget, type WsData } from './workspaces';

export interface DragDeps {
  persistent: () => boolean;
  data: () => WsData;
  elementAt: (x: number, y: number) => Element | null;
  patchWorkspace: (id: string, patch: Partial<Workspace>) => Promise<unknown>;
  patchFolder: (id: string, patch: Partial<Folder>) => Promise<unknown>;
  moveSession: (key: string, workspaceId: string) => Promise<unknown>;
  refuse: (reason: Key) => void;
}

// Folder moves show at once and roll back with a notice when the daemon refuses.
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

const appDeps = (): DragDeps => ({
  persistent: () => app.wsPersistent,
  data: () => app.ws,
  elementAt: (x, y) => document.elementFromPoint(x, y),
  patchWorkspace: (id, patch) => app.patchWorkspace(id, patch),
  patchFolder,
  moveSession: (key, workspaceId) => app.moveToWorkspace(key, workspaceId),
  refuse: (reason) => app.notify(t(reason), 'error'),
});

// Pointer drag of workspaces, folders and sessions in the sidebar; drop zones carry data-drop="workspace|child|folder|pinned|root"
// and data-id. A nested workspace line ("child") only takes sessions.
export class WsDrag {
  over = $state<DropTarget | null>(null);
  dragging = $state(false);
  /** True for the click that follows a drag, so releasing over a header does not select or fold it. */
  suppressed = false;
  #deps: DragDeps;
  #drag: { kind: 'workspace' | 'folder' | 'session'; id: string; x: number; y: number; active: boolean } | null = null;

  constructor(deps: DragDeps = appDeps()) {
    this.#deps = deps;
  }

  start = (e: PointerEvent, kind: 'workspace' | 'folder' | 'session', id: string) => {
    if (e.button !== 0 || !this.#deps.persistent()) return;
    this.#drag = { kind, id, x: e.clientX, y: e.clientY, active: false };
  };

  #targetAt(x: number, y: number): DropTarget | null {
    const el = this.#deps.elementAt(x, y)?.closest<HTMLElement>('[data-drop]');
    if (!el) return null;
    const type = el.dataset.drop;
    if (this.#drag?.kind === 'session') return type === 'workspace' || type === 'child' ? { type: 'workspace', id: el.dataset.id!, pos: 'inside' } : null;
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
    this.over = this.#targetAt(e.clientX, e.clientY);
  };

  end = async () => {
    const d = this.#drag;
    const target = this.over;
    this.#drag = null;
    this.dragging = false;
    this.over = null;
    if (!d?.active) return;
    this.suppressed = true;
    setTimeout(() => (this.suppressed = false), 0);
    if (!target) return;
    if (d.kind === 'session') {
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

  /** The pointer was taken over (system gesture): drop nothing. */
  cancel = () => {
    this.#drag = null;
    this.dragging = false;
    this.over = null;
  };

  /** Where the dragged item would land relative to this workspace or folder, for the drop marker. */
  posOf = (type: 'workspace' | 'folder', id: string): DropPos | null => (this.over && this.over.type === type && this.over.id === id ? this.over.pos : null);
}
