import { beforeEach, describe, expect, it, vi } from 'vitest';

const calls: string[] = [];
vi.mock('../browser', () => {
  const rec =
    (op: string) =>
    (id: string): Promise<void> => {
      calls.push(`${op}:${id}`);
      return Promise.resolve();
    };
  return {
    nativePage: {
      open: (id: string) => rec('open')(id),
      show: (id: string) => rec('show')(id),
      bounds: (id: string) => rec('bounds')(id),
      navigate: (id: string) => rec('navigate')(id),
      back: rec('back'),
      forward: rec('forward'),
      reload: rec('reload'),
      hide: rec('hide'),
      close: rec('close'),
      reset: () => rec('reset')('all'),
    },
  };
});

import { attach, closePage, loadPages, MAX_PAGES, MAX_SESSIONS, parsePages, prune, savePages, select, setRatio, touch, type PagesByKey } from './session-pages';
import { SessionPages } from './session-pages.svelte';

const B = { x: 0, y: 0, width: 100, height: 100 };
let n = 0;
const id = () => `p${n++}`;

function memoryStorage() {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k), clear: () => m.clear() };
}

beforeEach(() => {
  n = 0;
  calls.length = 0;
  vi.stubGlobal('localStorage', memoryStorage());
});

describe('session pages', () => {
  it('attaches pages to a session, reusing the same URL and focusing it', () => {
    let s: PagesByKey = {};
    s = attach(s, 'line:a', 'http://localhost:5173/', id).state;
    s = attach(s, 'line:a', 'https://docs.dev/', id).state;
    const again = attach(s, 'line:a', 'http://localhost:5173/', id);
    expect(again.page.id).toBe('p0');
    expect(again.state['line:a']).toMatchObject({ active: 'p0', open: true });
    expect(again.state['line:a'].pages.map((p) => p.id)).toEqual(['p0', 'p1']);
  });

  it('keeps pages per session and bounds pages and sessions', () => {
    let s: PagesByKey = {};
    for (let i = 0; i < MAX_PAGES + 3; i++) s = attach(s, 'line:a', `https://x.dev/${i}`, id).state;
    expect(s['line:a'].pages).toHaveLength(MAX_PAGES);
    expect(s['line:a'].pages[0].url).toBe('https://x.dev/3');
    for (let i = 0; i < MAX_SESSIONS + 2; i++) s = attach(s, `term:${i}`, 'https://y.dev/', id).state;
    expect(Object.keys(s)).toHaveLength(MAX_SESSIONS);
    expect(s['line:a']).toBeUndefined();
  });

  it('closing the active page focuses its neighbour and drops an empty session', () => {
    let s: PagesByKey = {};
    for (const u of ['https://a.dev/', 'https://b.dev/', 'https://c.dev/']) s = attach(s, 'k', u, id).state;
    s = select(s, 'k', 'p1');
    s = closePage(s, 'k', 'p1');
    expect(s.k.active).toBe('p2');
    s = closePage(closePage(s, 'k', 'p2'), 'k', 'p0');
    expect(s.k).toBeUndefined();
  });

  it('clamps the split ratio and prunes sessions that are gone', () => {
    let s = attach({}, 'k', 'https://a.dev/', id).state;
    s = attach(s, 'gone', 'https://b.dev/', id).state;
    expect(setRatio(s, 'k', 2).k.ratio).toBe(0.8);
    expect(Object.keys(prune(s, new Set(['k'])))).toEqual(['k']);
  });

  it('evicts the least recently shown live page', () => {
    let live: string[] = [];
    const evicted: string[] = [];
    for (const p of ['a', 'b', 'c', 'd', 'b', 'e']) {
      const r = touch(live, p, 4);
      live = r.live;
      evicted.push(...r.evicted);
    }
    expect(live).toEqual(['c', 'd', 'b', 'e']);
    expect(evicted).toEqual(['a']);
  });

  it('persists per host and ignores invalid entries', () => {
    savePages('ssh:dev', attach({}, 'k', 'https://a.dev/', id).state);
    savePages('local', attach({}, 'k', 'https://b.dev/', id).state);
    expect(loadPages('ssh:dev').k.pages[0].url).toBe('https://a.dev/');
    expect(loadPages('local').k.pages[0].url).toBe('https://b.dev/');
    expect(parsePages({ k: { pages: [{ id: 'x', url: 'file:///etc/passwd' }, { id: 'bad id', url: 'https://a.dev/' }] }, j: 'nope' })).toEqual({});
    expect(parsePages({ k: { pages: [{ id: 'p1', url: 'https://a.dev/' }], active: 'zz', ratio: 9 } }).k).toEqual({ pages: [{ id: 'p1', url: 'https://a.dev/' }], active: 'p1', open: true, ratio: 0.8 });
  });
});

describe('session pages store', () => {
  it('shows one session’s page and hides it on switch without reopening', async () => {
    const s = new SessionPages(true, 4);
    s.useHost('local');
    const a = s.attach('line:a', 'https://a.dev/');
    const b = s.attach('line:b', 'https://b.dev/');
    await s.show(a.id, B);
    s.hide(a.id);
    await s.show(b.id, B);
    s.hide(b.id);
    await s.show(a.id, B);
    expect(calls).toEqual(['reset:all', `open:${a.id}`, `hide:${a.id}`, `open:${b.id}`, `hide:${b.id}`, `show:${a.id}`]);
  });

  it('suspends the least recently used webview beyond the live limit', async () => {
    const s = new SessionPages(true, 2);
    s.useHost('local');
    const ids = ['a', 'b', 'c'].map((k) => s.attach(`line:${k}`, `https://${k}.dev/`).id);
    for (const p of ids) {
      await s.show(p, B);
      s.hide(p);
    }
    expect(s.live).toEqual([ids[1], ids[2]]);
    expect(calls).toContain(`close:${ids[0]}`);
    calls.length = 0;
    await s.show(ids[0], B);
    expect(calls).toEqual([`close:${ids[1]}`, `open:${ids[0]}`]);
  });

  it('never suspends a page that is on screen', async () => {
    const s = new SessionPages(true, 1);
    s.useHost('local');
    const a = s.attach('line:a', 'https://a.dev/');
    const b = s.attach('line:b', 'https://b.dev/');
    await s.show(a.id, B);
    await s.show(b.id, B);
    expect(calls).not.toContain(`close:${a.id}`);
    expect(s.live).toEqual([a.id, b.id]);
  });

  it('closes the webview of a closed page and of every page on host switch', async () => {
    const s = new SessionPages(true, 4);
    s.useHost('local');
    const a = s.attach('line:a', 'https://a.dev/');
    const b = s.attach('line:a', 'https://b.dev/');
    await s.show(a.id, B);
    await s.show(b.id, B);
    s.close('line:a', a.id);
    expect(calls).toContain(`close:${a.id}`);
    s.flush();
    s.useHost('ssh:dev');
    expect(calls).toContain(`close:${b.id}`);
    expect(s.byKey).toEqual({});
    s.useHost('local');
    expect(s.byKey['line:a'].pages.map((p) => p.url)).toEqual(['https://b.dev/']);
  });

  it('follows navigation in the page and keeps the latest URL for the next load', () => {
    const s = new SessionPages(true, 4);
    s.useHost('local');
    const a = s.attach('line:a', 'https://a.dev/');
    s.onNav(a.id, { url: 'https://a.dev/next', title: 'Next', loading: true });
    expect(s.page(a.id)).toEqual({ id: a.id, url: 'https://a.dev/next', title: 'Next' });
    expect(s.loading.has(a.id)).toBe(true);
  });

  it('does nothing native in the web build', async () => {
    const s = new SessionPages(false, 4);
    s.useHost('local');
    const a = s.attach('line:a', 'https://a.dev/');
    await s.show(a.id, B);
    expect(calls).toEqual([]);
  });
});
