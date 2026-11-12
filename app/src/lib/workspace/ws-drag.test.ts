import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Folder, Workspace } from '../types';
import { WsDrag, type DragDeps } from './ws-drag.svelte';

const w = (id: string, rank: number, extra: Partial<Workspace> = {}) => ({ id, name: id, rank, pinned: false, archived: false, folderId: null, ...extra }) as Workspace;
const f = (id: string, rank: number, extra: Partial<Folder> = {}) => ({ id, name: id, rank, parentId: null, collapsed: false, ...extra }) as Folder;

const at = (dataset: Record<string, string>) => {
  const el = { dataset, getBoundingClientRect: () => ({ top: 0, height: 100, left: 0, width: 100 }), closest: () => el };
  return el as unknown as Element;
};
const ev = (x: number, y: number, button = 0) => ({ button, clientX: x, clientY: y }) as PointerEvent;

function setup(under: Element | null, over: Partial<DragDeps> = {}) {
  const deps = {
    persistent: () => true,
    data: () => ({ workspaces: [w('a', 1), w('b', 2)], folders: [f('x', 1), f('y', 2)], groups: [] }),
    elementAt: () => under,
    patchWorkspace: vi.fn(async () => {}),
    patchFolder: vi.fn(async () => {}),
    moveSession: vi.fn(async () => {}),
    refuse: vi.fn(),
    canMove: () => true,
    reorder: vi.fn(),
    ...over,
  } satisfies DragDeps;
  return { deps, drag: new WsDrag(deps) };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('WsDrag', () => {
  it('moves a workspace below another one and swallows the click that ends the drag', async () => {
    const { deps, drag } = setup(at({ drop: 'workspace', id: 'b' }));
    drag.start(ev(0, 0), 'workspace', 'a');
    drag.move(ev(0, 20));
    expect(drag.dragging).toBe(true);
    expect(drag.posOf('workspace', 'b')).toBe('before');
    drag.move(ev(0, 80));
    expect(drag.posOf('workspace', 'b')).toBe('after');
    await drag.end();
    expect(deps.patchWorkspace).toHaveBeenCalledWith('a', expect.objectContaining({ pinned: false, folderId: null, rank: expect.any(Number) }));
    expect(drag.suppressed).toBe(true);
    vi.runAllTimers();
    expect(drag.suppressed).toBe(false);
    expect(drag.dragging).toBe(false);
  });

  it('puts a workspace into a folder and a folder into another', async () => {
    const { deps, drag } = setup(at({ drop: 'folder', id: 'x' }));
    drag.start(ev(0, 0), 'workspace', 'a');
    drag.move(ev(0, 50));
    await drag.end();
    drag.start(ev(0, 0), 'folder', 'y');
    drag.move(ev(0, 50));
    await drag.end();
    expect(deps.patchWorkspace).toHaveBeenCalledWith('a', expect.objectContaining({ folderId: 'x' }));
    expect(deps.patchFolder).toHaveBeenCalledWith('y', expect.objectContaining({ parentId: 'x' }));
  });

  it('pins a workspace dropped on the pinned section and unpins it on the root', async () => {
    const pinned = setup(at({ drop: 'pinned' }));
    pinned.drag.start(ev(0, 0), 'workspace', 'a');
    pinned.drag.move(ev(0, 50));
    await pinned.drag.end();
    const root = setup(at({ drop: 'root' }));
    root.drag.start(ev(0, 0), 'workspace', 'a');
    root.drag.move(ev(0, 50));
    await root.drag.end();
    expect(pinned.deps.patchWorkspace).toHaveBeenCalledWith('a', expect.objectContaining({ pinned: true }));
    expect(root.deps.patchWorkspace).toHaveBeenCalledWith('a', expect.objectContaining({ pinned: false, folderId: null }));
  });

  it('reads the horizontal position on a chip so wrapped chips reorder left to right', () => {
    const { drag } = setup(at({ drop: 'workspace', id: 'b', axis: 'x' }));
    drag.start(ev(0, 0), 'workspace', 'a');
    drag.move(ev(20, 90));
    const left = drag.posOf('workspace', 'b');
    drag.move(ev(90, 90));
    expect(left).toBe('before');
    expect(drag.posOf('workspace', 'b')).toBe('after');
  });

  it('never drags without persistence, with another button, or under 5px, and drops nowhere do nothing', async () => {
    const under = at({ drop: 'workspace', id: 'b' });
    const a = setup(under, { persistent: () => false });
    a.drag.start(ev(0, 0), 'workspace', 'a');
    a.drag.move(ev(0, 50));
    const b = setup(under);
    b.drag.start(ev(0, 0, 2), 'workspace', 'a');
    b.drag.move(ev(0, 50));
    const c = setup(under);
    c.drag.start(ev(0, 0), 'workspace', 'a');
    c.drag.move(ev(2, 2));
    await c.drag.end();
    const d = setup(null);
    d.drag.start(ev(0, 0), 'workspace', 'a');
    d.drag.move(ev(0, 50));
    await d.drag.end();
    expect([a.drag.dragging, b.drag.dragging, c.drag.dragging]).toEqual([false, false, false]);
    expect(c.drag.suppressed).toBe(false);
    expect(d.drag.suppressed).toBe(true);
    for (const x of [a, b, c, d]) expect(x.deps.patchWorkspace).not.toHaveBeenCalled();
  });

  it('skips a drop the planner refuses (a folder onto itself)', async () => {
    const { deps, drag } = setup(at({ drop: 'folder', id: 'x' }));
    drag.start(ev(0, 0), 'folder', 'x');
    drag.move(ev(0, 50));
    await drag.end();
    expect(deps.patchFolder).not.toHaveBeenCalled();
  });

  it('tells why a folder dropped onto its own sub-folder is refused', async () => {
    const data = () => ({ workspaces: [], folders: [f('x', 1), f('y', 2, { parentId: 'x' })], groups: [] });
    const { deps, drag } = setup(at({ drop: 'folder', id: 'y' }), { data });
    drag.start(ev(0, 0), 'folder', 'x');
    drag.move(ev(0, 50));
    await drag.end();
    expect(deps.patchFolder).not.toHaveBeenCalled();
    expect(deps.refuse).toHaveBeenCalledWith('ws.folderDepth');
  });

  it('tells why a folder dropped onto a workspace is refused, and stays silent for a drop on itself', async () => {
    const onWs = setup(at({ drop: 'workspace', id: 'a' }));
    onWs.drag.start(ev(0, 0), 'folder', 'x');
    onWs.drag.move(ev(0, 50));
    await onWs.drag.end();
    expect(onWs.deps.refuse).toHaveBeenCalledWith('ws.folderDropTarget');
    const onSelf = setup(at({ drop: 'folder', id: 'x' }));
    onSelf.drag.start(ev(0, 0), 'folder', 'x');
    onSelf.drag.move(ev(0, 50));
    await onSelf.drag.end();
    expect(onSelf.deps.refuse).not.toHaveBeenCalled();
  });

  it('abandons a drag on cancel without patching, and without swallowing the next click', async () => {
    const a = setup(at({ drop: 'workspace', id: 'b' }));
    const b = setup(at({ drop: 'folder', id: 'x' }));
    a.drag.start(ev(0, 0), 'workspace', 'a');
    a.drag.move(ev(0, 80));
    b.drag.start(ev(0, 0), 'folder', 'y');
    b.drag.move(ev(0, 50));
    a.drag.cancel();
    b.drag.cancel();
    await a.drag.end();
    expect(a.deps.patchWorkspace).not.toHaveBeenCalled();
    expect(b.deps.patchFolder).not.toHaveBeenCalled();
    expect([a.drag.dragging, a.drag.over, a.drag.suppressed]).toEqual([false, null, false]);
  });

  it('moves a dragged session onto a nested workspace line', async () => {
    const { deps, drag } = setup(at({ drop: 'child', id: 'sub' }));
    drag.start(ev(0, 0), 'session', 'line:l1');
    expect(drag.kind).toBeNull();
    drag.move(ev(0, 30));
    expect(drag.kind).toBe('session');
    expect(drag.posOf('workspace', 'sub')).toBe('inside');
    await drag.end();
    expect(drag.kind).toBeNull();
    expect(deps.moveSession).toHaveBeenCalledWith('line:l1', 'sub');
    expect(deps.patchWorkspace).not.toHaveBeenCalled();
  });

  it('moves a dragged session onto a top-level workspace but not onto a folder', async () => {
    const onWs = setup(at({ drop: 'workspace', id: 'b' }));
    onWs.drag.start(ev(0, 0), 'session', 'term:t1');
    onWs.drag.move(ev(0, 30));
    await onWs.drag.end();
    expect(onWs.deps.moveSession).toHaveBeenCalledWith('term:t1', 'b');
    const onFolder = setup(at({ drop: 'folder', id: 'x' }));
    onFolder.drag.start(ev(0, 0), 'session', 'term:t1');
    onFolder.drag.move(ev(0, 30));
    await onFolder.drag.end();
    expect(onFolder.deps.moveSession).not.toHaveBeenCalled();
    expect(onFolder.deps.patchFolder).not.toHaveBeenCalled();
  });

  it('ignores a nested workspace line as a target for workspace drags', async () => {
    const { deps, drag } = setup(at({ drop: 'child', id: 'sub' }));
    drag.start(ev(0, 0), 'workspace', 'a');
    drag.move(ev(0, 30));
    expect(drag.over).toBeNull();
    await drag.end();
    expect(deps.patchWorkspace).not.toHaveBeenCalled();
  });

  it('reorders the tabs when a session is dropped on the lower half of another row of its workspace', async () => {
    const { deps, drag } = setup(at({ drop: 'session', id: 'b', zone: 'w' }));
    drag.start(ev(0, 0), 'session', 'a', 'w');
    drag.move(ev(0, 20));
    expect(drag.posOf('session', 'b')).toBe('before');
    drag.move(ev(0, 80));
    expect(drag.posOf('session', 'b')).toBe('after');
    await drag.end();
    expect(deps.reorder).toHaveBeenCalledWith('w', 'a', 'b', 'after');
    expect(deps.moveSession).not.toHaveBeenCalled();
  });

  it('ignores a session row of another workspace and the dragged row itself', async () => {
    const other = setup(at({ drop: 'session', id: 'b', zone: 'v' }));
    other.drag.start(ev(0, 0), 'session', 'a', 'w');
    other.drag.move(ev(0, 20));
    expect(other.drag.over).toBeNull();
    await other.drag.end();
    const self = setup(at({ drop: 'session', id: 'a', zone: 'w' }));
    self.drag.start(ev(0, 0), 'session', 'a', 'w');
    self.drag.move(ev(0, 20));
    expect(self.drag.over).toBeNull();
    await self.drag.end();
    expect(other.deps.reorder).not.toHaveBeenCalled();
    expect(self.deps.reorder).not.toHaveBeenCalled();
  });

  it('reorders even when workspaces are not saved, but moves a session to another workspace only when it can', async () => {
    const rows = setup(at({ drop: 'session', id: 'b', zone: 'w' }), { persistent: () => false });
    rows.drag.start(ev(0, 0), 'session', 'a', 'w');
    rows.drag.move(ev(0, 20));
    await rows.drag.end();
    const head = setup(at({ drop: 'workspace', id: 'b' }), { canMove: () => false });
    head.drag.start(ev(0, 0), 'session', 'a', 'w');
    head.drag.move(ev(0, 20));
    expect(head.drag.over).toBeNull();
    await head.drag.end();
    expect(rows.deps.reorder).toHaveBeenCalledWith('w', 'a', 'b', 'before');
    expect(head.deps.moveSession).not.toHaveBeenCalled();
  });

  it('never drops a workspace or a folder on a session row', async () => {
    const { deps, drag } = setup(at({ drop: 'session', id: 'b', zone: 'w' }));
    drag.start(ev(0, 0), 'workspace', 'a');
    drag.move(ev(0, 20));
    expect(drag.over).toBeNull();
    await drag.end();
    expect(deps.patchWorkspace).not.toHaveBeenCalled();
  });
});
