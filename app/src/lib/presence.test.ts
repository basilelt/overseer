import { beforeEach, describe, expect, it, vi } from 'vitest';

type Mod = typeof import('./presence');

const focusChanged = vi.hoisted(() => ({ handler: null as ((e: { payload: boolean }) => void) | null, off: vi.fn(), fail: false }));
vi.mock('@tauri-apps/api/window', () => ({
  getCurrentWindow: () => ({
    onFocusChanged: async (fn: (e: { payload: boolean }) => void) => {
      if (focusChanged.fail) throw new Error('no window');
      focusChanged.handler = fn;
      return focusChanged.off;
    },
  }),
}));

function fakeDom(visible = true, focused = true) {
  const classes = new Set<string>();
  const doc = new Map<string, () => void>();
  const win = new Map<string, () => void>();
  const document = {
    visibilityState: visible ? 'visible' : 'hidden',
    hasFocus: () => focused,
    documentElement: { classList: { toggle: (c: string, on: boolean) => (on ? classes.add(c) : classes.delete(c)) } },
    addEventListener: (t: string, fn: () => void) => doc.set(t, fn),
    removeEventListener: (t: string, fn: () => void) => doc.get(t) === fn && doc.delete(t),
  };
  const window = {
    addEventListener: (t: string, fn: () => void) => win.set(t, fn),
    removeEventListener: (t: string, fn: () => void) => win.get(t) === fn && win.delete(t),
  };
  return { classes, doc, win, document, window };
}

describe('presence', () => {
  let p: Mod;

  beforeEach(async () => {
    vi.resetModules();
    focusChanged.handler = null;
    focusChanged.off.mockReset();
    focusChanged.fail = false;
    p = await import('./presence');
  });

  it('starts visible and focused', () => {
    expect(p.presence()).toEqual({ visible: true, focused: true });
    expect(p.active()).toBe(true);
  });

  it('notifies listeners only on a real change', () => {
    const seen: string[] = [];
    p.onPresence((s) => seen.push(`${s.visible}/${s.focused}`));
    p.setPresence({ focused: true });
    p.setPresence({});
    p.setPresence({ focused: false });
    p.setPresence({ visible: false });
    expect(seen).toEqual(['true/false', 'false/false']);
    expect(p.active()).toBe(false);
  });

  it('stops notifying after unsubscribe, even from inside a callback', () => {
    const a = vi.fn();
    const b = vi.fn();
    const offA = p.onPresence(() => {
      a();
      offA();
    });
    p.onPresence(b);
    p.setPresence({ focused: false });
    p.setPresence({ focused: true });
    expect(a).toHaveBeenCalledTimes(1);
    expect(b).toHaveBeenCalledTimes(2);
  });

  it('marks the root hidden or inactive for CSS', () => {
    const dom = fakeDom();
    vi.stubGlobal('document', dom.document);
    p.setPresence({ focused: false });
    expect([...dom.classes]).toEqual(['inactive']);
    p.setPresence({ visible: false });
    expect([...dom.classes].sort()).toEqual(['hidden-window', 'inactive']);
    p.setPresence({ visible: true, focused: true });
    expect([...dom.classes]).toEqual([]);
  });

  it('is a no-op without a document', () => {
    vi.stubGlobal('document', undefined);
    p.startPresence()();
    expect(p.presence()).toEqual({ visible: true, focused: true });
  });

  it('reads the initial state and follows visibility, focus and blur until stopped', () => {
    const dom = fakeDom(false, false);
    vi.stubGlobal('document', dom.document);
    vi.stubGlobal('window', dom.window);
    const stop = p.startPresence();
    expect(p.presence()).toEqual({ visible: false, focused: false });
    dom.document.visibilityState = 'visible';
    dom.doc.get('visibilitychange')!();
    expect(p.presence().visible).toBe(true);
    dom.win.get('focus')!();
    expect(p.active()).toBe(true);
    dom.win.get('blur')!();
    expect(p.presence().focused).toBe(false);
    stop();
    expect(dom.doc.size + dom.win.size).toBe(0);
  });

  it('stays inactive while idle whatever the window focus, until idle ends', () => {
    const dom = fakeDom();
    vi.stubGlobal('document', dom.document);
    vi.stubGlobal('window', dom.window);
    const stop = p.startPresence();
    p.setIdle(true);
    expect(p.presence().focused).toBe(false);
    expect([...dom.classes]).toEqual(['inactive']);
    dom.win.get('blur')!();
    dom.win.get('focus')!();
    expect(p.presence().focused).toBe(false);
    p.setIdle(false);
    expect(p.active()).toBe(true);
    dom.win.get('blur')!();
    p.setIdle(true);
    p.setIdle(false);
    expect(p.presence().focused).toBe(false);
    stop();
  });

  it('follows the native window focus in the desktop app and unlistens on stop', async () => {
    const dom = fakeDom();
    vi.stubGlobal('document', dom.document);
    vi.stubGlobal('window', { ...dom.window, __TAURI_INTERNALS__: {} });
    const stop = p.startPresence();
    await vi.waitFor(() => expect(focusChanged.handler).not.toBeNull());
    focusChanged.handler!({ payload: false });
    expect(p.presence().focused).toBe(false);
    focusChanged.handler!({ payload: true });
    expect(p.presence().focused).toBe(true);
    stop();
    await vi.waitFor(() => expect(focusChanged.off).toHaveBeenCalledTimes(1));
  });

  it('keeps working when the native focus listener cannot be set up', async () => {
    focusChanged.fail = true;
    const dom = fakeDom();
    vi.stubGlobal('document', dom.document);
    vi.stubGlobal('window', { ...dom.window, __TAURI_INTERNALS__: {} });
    const stop = p.startPresence();
    await Promise.resolve();
    stop();
    await Promise.resolve();
    expect(focusChanged.off).not.toHaveBeenCalled();
    expect(dom.doc.size + dom.win.size).toBe(0);
  });
});
