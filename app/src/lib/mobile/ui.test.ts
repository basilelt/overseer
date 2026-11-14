import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { isPhone, isPhoneOr, onResume, startLayout, ui } from './ui.svelte';

type Handlers = Map<string, Set<() => void>>;

function target() {
  const handlers: Handlers = new Map();
  return {
    handlers,
    addEventListener: (t: string, fn: () => void) => {
      if (!handlers.has(t)) handlers.set(t, new Set());
      handlers.get(t)!.add(fn);
    },
    removeEventListener: (t: string, fn: () => void) => void handlers.get(t)?.delete(fn),
    fire: (t: string) => [...(handlers.get(t) ?? [])].forEach((f) => f()),
    count: () => [...handlers.values()].reduce((n, s) => n + s.size, 0),
  };
}

function root() {
  const classes = new Set<string>();
  const props = new Map<string, string>();
  return {
    classes,
    props,
    el: {
      classList: {
        toggle: (c: string, on: boolean) => (on ? classes.add(c) : classes.delete(c)),
        remove: (c: string) => classes.delete(c),
      },
      style: { setProperty: (k: string, v: string) => props.set(k, v), removeProperty: (k: string) => props.delete(k) },
    },
  };
}

const el = (sel: string) => ({ matches: (q: string) => q.split(',').includes(sel) });

describe('mobile layout', () => {
  let win: ReturnType<typeof target>;
  let doc: ReturnType<typeof root>;
  let frames: Map<number, () => void>;
  let nextFrame: number;

  const flushFrames = () => {
    const due = [...frames.values()];
    frames.clear();
    due.forEach((f) => f());
  };

  beforeEach(() => {
    win = target();
    doc = root();
    frames = new Map();
    nextFrame = 1;
    Object.assign(ui, { layout: 'desktop', mobileOs: false, forcePhone: false, drawer: false, tab: 'agents', panel: false });
    vi.stubGlobal('addEventListener', win.addEventListener);
    vi.stubGlobal('removeEventListener', win.removeEventListener);
    vi.stubGlobal('document', { documentElement: doc.el });
    vi.stubGlobal('location', { search: '' });
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (X11; Linux x86_64)' });
    vi.stubGlobal('innerWidth', 1440);
    vi.stubGlobal('innerHeight', 900);
    vi.stubGlobal('visualViewport', undefined);
    vi.stubGlobal('requestAnimationFrame', (cb: () => void) => {
      const id = nextFrame++;
      frames.set(id, cb);
      return id;
    });
    vi.stubGlobal('cancelAnimationFrame', (id: number) => frames.delete(id));
    vi.stubGlobal('scrollTo', vi.fn());
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    Object.assign(ui, { layout: 'desktop', mobileOs: false, forcePhone: false });
  });

  it('switches between phone and desktop as the window is resized', () => {
    vi.stubGlobal('innerWidth', 500);
    const stop = startLayout();
    expect(isPhone()).toBe(true);
    expect(doc.classes.has('phone')).toBe(true);
    vi.stubGlobal('innerWidth', 1300);
    win.fire('resize');
    expect(ui.layout).toBe('desktop');
    expect(doc.classes.has('phone')).toBe(false);
    stop();
    expect(win.count()).toBe(0);
  });

  it('forces the phone layout with ?phone=1', () => {
    vi.stubGlobal('location', { search: '?phone=1' });
    startLayout()();
    expect(ui.forcePhone).toBe(true);
    expect(isPhone()).toBe(true);
  });

  it('keeps tablets on the desktop layout and phones on the phone layout', () => {
    vi.stubGlobal('innerWidth', 1100);
    startLayout(true)();
    expect(ui.mobileOs).toBe(true);
    expect(ui.layout).toBe('desktop');
    vi.stubGlobal('innerWidth', 900);
    vi.stubGlobal('navigator', { userAgent: 'Mozilla/5.0 (Linux; Android 14)' });
    startLayout()();
    expect(ui.layout).toBe('phone');
  });

  it('works with no window size or document', () => {
    vi.stubGlobal('innerWidth', undefined);
    vi.stubGlobal('document', undefined);
    vi.stubGlobal('location', undefined);
    vi.stubGlobal('navigator', undefined);
    startLayout()();
    expect(ui.layout).toBe('desktop');
  });

  describe('on-screen keyboard', () => {
    let vv: ReturnType<typeof target> & { height: number; scale: number; offsetTop: number };
    let page: ReturnType<typeof target> & { documentElement: typeof doc.el; activeElement: unknown; visibilityState: string };

    beforeEach(() => {
      vv = Object.assign(target(), { height: 900, scale: 1, offsetTop: 0 });
      vi.stubGlobal('visualViewport', vv);
      page = Object.assign(target(), { documentElement: doc.el, activeElement: el('textarea'), visibilityState: 'visible' });
      vi.stubGlobal('document', page);
    });

    it('reserves no keyboard space at launch when nothing has focus', () => {
      page.activeElement = el('body');
      vv.height = 500;
      const stop = startLayout();
      expect(doc.props.get('--kb')).toBe('0px');
      expect(doc.classes.has('kb')).toBe(false);
      stop();
    });

    it('drops a stale keyboard inset on resume once the text box lost focus', () => {
      const stop = startLayout();
      vv.height = 500;
      vv.fire('resize');
      flushFrames();
      expect(doc.props.get('--kb')).toBe('400px');
      page.activeElement = null;
      page.fire('visibilitychange');
      flushFrames();
      expect(doc.props.get('--kb')).toBe('0px');
      expect(doc.classes.has('kb')).toBe(false);
      stop();
    });

    it('re-measures when the app comes back to the foreground', () => {
      const stop = startLayout();
      vv.height = 500;
      vv.fire('resize');
      flushFrames();
      vv.height = 900;
      page.fire('visibilitychange');
      flushFrames();
      expect(doc.props.get('--kb')).toBe('0px');
      stop();
    });

    it('counts the keyboard for text areas and inputs', () => {
      page.activeElement = el('textarea');
      vv.height = 600;
      const stop = startLayout();
      expect(doc.props.get('--kb')).toBe('300px');
      page.activeElement = el('input');
      vv.height = 500;
      vv.fire('resize');
      flushFrames();
      expect(doc.props.get('--kb')).toBe('400px');
      stop();
    });

    it('exposes the keyboard height to CSS once per frame', () => {
      const stop = startLayout();
      expect(doc.props.get('--kb')).toBe('0px');
      expect(doc.classes.has('kb')).toBe(false);
      vv.height = 600;
      vv.fire('resize');
      vv.fire('resize');
      win.fire('resize');
      expect(frames.size).toBe(1);
      flushFrames();
      expect(doc.props.get('--kb')).toBe('300px');
      expect(doc.classes.has('kb')).toBe(true);
      stop();
    });

    it('ignores small viewport changes like a hiding URL bar', () => {
      startLayout();
      vv.height = 850;
      vv.fire('resize');
      flushFrames();
      expect(doc.props.get('--kb')).toBe('0px');
    });

    it('scrolls the page back when the keyboard pushed it up', () => {
      startLayout();
      vv.height = 500;
      vv.offsetTop = 120;
      vv.fire('resize');
      flushFrames();
      expect(globalThis.scrollTo).toHaveBeenCalledWith(0, 0);
    });

    it('accounts for pinch zoom', () => {
      startLayout();
      vv.height = 300;
      vv.scale = 2;
      vv.fire('resize');
      flushFrames();
      expect(doc.props.get('--kb')).toBe('300px');
    });

    it('cleans up listeners, pending frames and CSS on stop', () => {
      const stop = startLayout();
      vv.height = 500;
      vv.fire('resize');
      flushFrames();
      vv.height = 400;
      vv.fire('resize');
      expect(frames.size).toBe(1);
      stop();
      expect(frames.size).toBe(0);
      expect(vv.count()).toBe(0);
      expect(win.count()).toBe(0);
      expect(page.count()).toBe(0);
      expect(doc.props.has('--kb')).toBe(false);
      expect(doc.classes.has('kb')).toBe(false);
    });
  });
});

describe('onResume', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('runs when the app becomes visible again or comes back online', () => {
    const doc = Object.assign(target(), { visibilityState: 'hidden' });
    const win = target();
    vi.stubGlobal('document', doc);
    vi.stubGlobal('addEventListener', win.addEventListener);
    vi.stubGlobal('removeEventListener', win.removeEventListener);
    const fn = vi.fn();
    const stop = onResume(fn);
    doc.fire('visibilitychange');
    expect(fn).not.toHaveBeenCalled();
    doc.visibilityState = 'visible';
    doc.fire('visibilitychange');
    win.fire('online');
    expect(fn).toHaveBeenCalledTimes(2);
    stop();
    expect(doc.count() + win.count()).toBe(0);
  });

  it('is a no-op without a document', () => {
    vi.stubGlobal('document', undefined);
    expect(() => onResume(vi.fn())()).not.toThrow();
  });
});

describe('isPhoneOr', () => {
  afterEach(() => {
    ui.layout = 'desktop';
  });

  it('is true for a mobile shell on any layout', () => {
    ui.layout = 'desktop';
    expect(isPhoneOr(true)).toBe(true);
  });

  it('is true on the phone layout without a mobile shell', () => {
    ui.layout = 'phone';
    expect(isPhoneOr(false)).toBe(true);
  });

  it('is false on desktop without a mobile shell', () => {
    ui.layout = 'desktop';
    expect(isPhoneOr(false)).toBe(false);
  });
});
