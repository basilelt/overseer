import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { Folder, Workspace } from './types';
import { WsDrag, type DragDeps } from './ws-drag.svelte';

const w = (id: string, rank: number, extra: Partial<Workspace> = {}) => ({ id, name: id, rank, pinned: false, archived: false, folderId: null, ...extra }) as Workspace;
const f = (id: string, rank: number, extra: Partial<Folder> = {}) => ({ id, name: id, rank, parentId: null, collapsed: false, ...extra }) as Folder;

// A fake element under the pointer: data-drop attributes and a 100x100 box at the origin.
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
    ...over,
  } satisfies DragDeps;
  return { deps, drag: new WsDrag(deps) };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('WsDrag', () => {
  it('moves a workspace below another one and swallows the click that ends the drag', async () => {
    // Given a drag of workspace a that starts on a header
    const { deps, drag } = setup(at({ drop: 'workspace', id: 'b' }));
    // When it moves 20px, over the lower half of workspace b, and is released
    drag.start(ev(0, 0), 'workspace', 'a');
    drag.move(ev(0, 20));
    expect(drag.dragging).toBe(true);
    expect(drag.posOf('workspace', 'b')).toBe('before');
    drag.move(ev(0, 80));
    expect(drag.posOf('workspace', 'b')).toBe('after');
    await drag.end();
    // Then the daemon gets the planned patch, and the next click is ignored once
    expect(deps.patchWorkspace).toHaveBeenCalledWith('a', expect.objectContaining({ pinned: false, folderId: null, rank: expect.any(Number) }));
    expect(drag.suppressed).toBe(true);
    vi.runAllTimers();
    expect(drag.suppressed).toBe(false);
    expect(drag.dragging).toBe(false);
  });

  it('puts a workspace into a folder and a folder into another', async () => {
    // Given a drop on the middle of folder x
    const { deps, drag } = setup(at({ drop: 'folder', id: 'x' }));
    // When a workspace is dropped there, then a folder
    drag.start(ev(0, 0), 'workspace', 'a');
    drag.move(ev(0, 50));
    await drag.end();
    drag.start(ev(0, 0), 'folder', 'y');
    drag.move(ev(0, 50));
    await drag.end();
    // Then the workspace gets folderId x and the folder gets parentId x
    expect(deps.patchWorkspace).toHaveBeenCalledWith('a', expect.objectContaining({ folderId: 'x' }));
    expect(deps.patchFolder).toHaveBeenCalledWith('y', expect.objectContaining({ parentId: 'x' }));
  });

  it('pins a workspace dropped on the pinned section and unpins it on the root', async () => {
    // Given the pinned section under the pointer
    const pinned = setup(at({ drop: 'pinned' }));
    // When a workspace is dropped there, then on the root zone
    pinned.drag.start(ev(0, 0), 'workspace', 'a');
    pinned.drag.move(ev(0, 50));
    await pinned.drag.end();
    const root = setup(at({ drop: 'root' }));
    root.drag.start(ev(0, 0), 'workspace', 'a');
    root.drag.move(ev(0, 50));
    await root.drag.end();
    // Then the patches pin, then clear pinned and folder
    expect(pinned.deps.patchWorkspace).toHaveBeenCalledWith('a', expect.objectContaining({ pinned: true }));
    expect(root.deps.patchWorkspace).toHaveBeenCalledWith('a', expect.objectContaining({ pinned: false, folderId: null }));
  });

  it('reads the horizontal position on a chip so wrapped chips reorder left to right', () => {
    // Given a chip under the pointer
    const { drag } = setup(at({ drop: 'workspace', id: 'b', axis: 'x' }));
    // When the pointer is in the left, then the right half, at the same height
    drag.start(ev(0, 0), 'workspace', 'a');
    drag.move(ev(20, 90));
    const left = drag.posOf('workspace', 'b');
    drag.move(ev(90, 90));
    // Then the side decides, not the height
    expect(left).toBe('before');
    expect(drag.posOf('workspace', 'b')).toBe('after');
  });

  it('never drags without persistence, with another button, or under 5px, and drops nowhere do nothing', async () => {
    // Given a workspace under the pointer
    const under = at({ drop: 'workspace', id: 'b' });
    // When the drag is refused three ways, then released over nothing
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
    // Then nothing is dragging, nothing is patched, and only the real drag swallowed the click
    expect([a.drag.dragging, b.drag.dragging, c.drag.dragging]).toEqual([false, false, false]);
    expect(c.drag.suppressed).toBe(false);
    expect(d.drag.suppressed).toBe(true);
    for (const x of [a, b, c, d]) expect(x.deps.patchWorkspace).not.toHaveBeenCalled();
  });

  it('skips a drop the planner refuses (a folder onto itself)', async () => {
    // Given folder x dragged over its own row
    const { deps, drag } = setup(at({ drop: 'folder', id: 'x' }));
    // When it is released there
    drag.start(ev(0, 0), 'folder', 'x');
    drag.move(ev(0, 50));
    await drag.end();
    // Then no patch is sent
    expect(deps.patchFolder).not.toHaveBeenCalled();
  });

  it('abandons a drag on cancel without patching, and without swallowing the next click', async () => {
    // Given a workspace and a folder dragged over valid targets
    const a = setup(at({ drop: 'workspace', id: 'b' }));
    const b = setup(at({ drop: 'folder', id: 'x' }));
    a.drag.start(ev(0, 0), 'workspace', 'a');
    a.drag.move(ev(0, 80));
    b.drag.start(ev(0, 0), 'folder', 'y');
    b.drag.move(ev(0, 50));
    // When the pointer is cancelled (system gesture, scroll takeover)
    a.drag.cancel();
    b.drag.cancel();
    await a.drag.end();
    // Then nothing is patched and the drag state is reset
    expect(a.deps.patchWorkspace).not.toHaveBeenCalled();
    expect(b.deps.patchFolder).not.toHaveBeenCalled();
    expect([a.drag.dragging, a.drag.over, a.drag.suppressed]).toEqual([false, null, false]);
  });
});
