import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { dropPoint, pickZone, type DropZone } from './drop';

type DragEvent = { payload: { type: 'enter' | 'over' | 'drop' | 'leave'; position: { x: number; y: number }; paths: string[] } };
const webview = vi.hoisted(() => ({ handler: null as ((e: DragEvent) => void) | null, calls: 0, fail: false }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onDragDropEvent: async (fn: (e: DragEvent) => void) => {
      webview.calls++;
      if (webview.fail) throw new Error('no webview');
      webview.handler = fn;
      return () => {};
    },
  }),
}));
const child = vi.hoisted(() => ({ calls: 0 }));
vi.mock('@tauri-apps/api/webview', () => ({
  getCurrentWebview: () => ({
    onDragDropEvent: async () => {
      child.calls++;
      return () => {};
    },
  }),
}));

describe('dropPoint', () => {
  it('keeps the logical points macOS and Linux report', () => {
    expect(dropPoint({ x: 400, y: 800 }, false, 2)).toEqual({ x: 400, y: 800 });
  });

  it('scales the physical pixels Windows reports', () => {
    expect(dropPoint({ x: 400, y: 800 }, true, 2)).toEqual({ x: 200, y: 400 });
    expect(dropPoint({ x: 30, y: 60 }, true, 0)).toEqual({ x: 30, y: 60 });
  });
});

type Fake = { parent: Fake | null; focused?: boolean; pane?: boolean; shown?: boolean };

function el(f: Fake): Element {
  const self = {
    f,
    isConnected: true,
    getClientRects: () => (f.shown === false ? [] : [{}]),
    contains: (o: Element) => {
      let n: Fake | null = (o as unknown as { f: Fake }).f;
      while (n) {
        if (n === f) return true;
        n = n.parent;
      }
      return false;
    },
    closest: (sel: string) => {
      let n: Fake | null = f;
      while (n) {
        if (sel === '.pane' && n.pane) return el(n);
        if (sel === '.pane.focused' && n.pane && n.focused) return el(n);
        n = n.parent;
      }
      return null;
    },
  };
  return self as unknown as Element;
}

type Named = DropZone & { name: string };
const zone = (f: Fake, rank: number, name: string): Named => ({ name, el: () => el(f), rank, hover: () => {}, drop: () => {} });

describe('pickZone', () => {
  const left: Fake = { parent: null, pane: true, focused: false };
  const right: Fake = { parent: null, pane: true, focused: true };
  const chat: Fake = { parent: right };
  const term: Fake = { parent: left };
  const zones = [zone(chat, 2, 'chat'), zone(term, 1, 'term')];

  it('picks the zone under the pointer', () => {
    expect((pickZone(zones, [el({ parent: term })]) as Named | null)?.name).toBe('term');
  });

  it('tries the second reading of the point when the first hits nothing', () => {
    expect((pickZone(zones, [null, el({ parent: chat })]) as Named | null)?.name).toBe('chat');
  });

  it('falls back to the zone in the focused pane when the point hits no zone', () => {
    expect((pickZone(zones, [el({ parent: null })]) as Named | null)?.name).toBe('chat');
  });

  it('prefers the higher rank, like an open feedback sheet over the chat', () => {
    const modal: Fake = { parent: null };
    const all = [...zones, zone(modal, 3, 'feedback')];
    expect((pickZone(all, [el({ parent: null })]) as Named | null)?.name).toBe('feedback');
  });

  it('ignores hidden zones', () => {
    const hidden = zone({ parent: right, shown: false }, 5, 'hidden');
    expect((pickZone([hidden, ...zones], [null]) as Named | null)?.name).toBe('chat');
  });
});

describe('dropZone', () => {
  const pane: Fake = { parent: null, pane: true, focused: true };
  const leftF: Fake = { parent: pane };
  const rightF: Fake = { parent: pane };
  const points = new Map<string, Element>();
  let picked: { x: number; y: number }[] = [];

  const tracked = (f: Fake, rank: number) => {
    const log: string[] = [];
    const z: DropZone = { el: () => el(f), rank, hover: (on) => log.push(on ? 'on' : 'off'), drop: (paths) => log.push(`drop:${paths.join(',')}`) };
    return { z, log };
  };
  const send = (type: DragEvent['payload']['type'], x = 0, y = 0, paths: string[] = []) => webview.handler!({ payload: { type, position: { x, y }, paths } });
  const load = async () => {
    vi.resetModules();
    return import('./drop');
  };

  beforeEach(() => {
    webview.handler = null;
    webview.calls = 0;
    webview.fail = false;
    picked = [];
    points.clear();
    points.set('10,10', el({ parent: leftF }));
    points.set('50,50', el({ parent: rightF }));
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {}, devicePixelRatio: 2 });
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (X11; Linux x86_64)' });
    vi.stubGlobal('document', {
      elementFromPoint: (x: number, y: number) => {
        picked.push({ x, y });
        return points.get(`${x},${y}`) ?? null;
      },
    });
  });
  afterEach(() => vi.unstubAllGlobals());

  it('highlights the zone under the pointer as it moves and clears on leave', async () => {
    const { dropZone } = await load();
    const a = tracked(leftF, 1);
    const b = tracked(rightF, 1);
    dropZone(a.z);
    dropZone(b.z);
    await vi.waitFor(() => expect(webview.handler).not.toBeNull());
    send('enter', 10, 10);
    send('over', 10, 10);
    send('over', 50, 50);
    send('leave');
    expect(a.log).toEqual(['on', 'off']);
    expect(b.log).toEqual(['on', 'off']);
  });

  it('drops the paths on the zone under the pointer and clears the highlight', async () => {
    const { dropZone } = await load();
    const a = tracked(leftF, 1);
    const b = tracked(rightF, 1);
    dropZone(a.z);
    dropZone(b.z);
    await vi.waitFor(() => expect(webview.handler).not.toBeNull());
    send('over', 50, 50);
    send('drop', 50, 50, ['/tmp/a.png', '/tmp/b.txt']);
    expect(b.log).toEqual(['on', 'off', 'drop:/tmp/a.png,/tmp/b.txt']);
    expect(a.log).toEqual([]);
  });

  it('listens on the window, which still gets drops once a browser child webview is open', async () => {
    const { dropZone } = await load();
    dropZone(tracked(leftF, 1).z);
    await vi.waitFor(() => expect(webview.handler).not.toBeNull());
    expect(child.calls).toBe(0);
  });

  it('subscribes to the window once for many zones', async () => {
    const { dropZone } = await load();
    dropZone(tracked(leftF, 1).z);
    dropZone(tracked(rightF, 1).z);
    await vi.waitFor(() => expect(webview.handler).not.toBeNull());
    dropZone(tracked(leftF, 2).z);
    expect(webview.calls).toBe(1);
  });

  it('clears the highlight when the hovered zone goes away and forgets it', async () => {
    const { dropZone } = await load();
    const a = tracked(leftF, 1);
    const off = dropZone(a.z);
    await vi.waitFor(() => expect(webview.handler).not.toBeNull());
    send('over', 10, 10);
    off();
    send('drop', 10, 10, ['/x']);
    expect(a.log).toEqual(['on', 'off']);
  });

  it('unregistering a zone that is not hovered leaves the highlight alone', async () => {
    const { dropZone } = await load();
    const a = tracked(leftF, 1);
    const b = tracked(rightF, 1);
    dropZone(a.z);
    const offB = dropZone(b.z);
    await vi.waitFor(() => expect(webview.handler).not.toBeNull());
    send('over', 10, 10);
    offB();
    expect(a.log).toEqual(['on']);
  });

  it('reads the Windows physical point first, then the raw one', async () => {
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Windows NT 10.0; Win64; x64)' });
    const { dropZone } = await load();
    const a = tracked(leftF, 1);
    dropZone(a.z);
    await vi.waitFor(() => expect(webview.handler).not.toBeNull());
    send('over', 20, 20);
    expect(picked).toEqual([
      { x: 10, y: 10 },
      { x: 20, y: 20 },
    ]);
    expect(a.log).toEqual(['on']);
  });

  it('never subscribes in a browser', async () => {
    vi.stubGlobal('window', { devicePixelRatio: 1 });
    const { dropZone } = await load();
    dropZone(tracked(leftF, 1).z)();
    await Promise.resolve();
    expect(webview.calls).toBe(0);
  });

  it('retries the subscription after a failure', async () => {
    webview.fail = true;
    const { dropZone } = await load();
    dropZone(tracked(leftF, 1).z);
    await vi.waitFor(() => expect(webview.calls).toBe(1));
    await new Promise((r) => setTimeout(r, 0));
    webview.fail = false;
    dropZone(tracked(rightF, 1).z);
    await vi.waitFor(() => expect(webview.handler).not.toBeNull());
    expect(webview.calls).toBe(2);
  });
});
