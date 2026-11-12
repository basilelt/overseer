import { afterEach, describe, expect, it, vi } from 'vitest';
import { DaemonError } from '../daemon/client';
import { MockDaemon } from '../daemon/mock';
import { reviewApi, type ReviewApi } from './api';
import { ReviewMapStore, WEIGHTS, band, fid, heat, heatInk, heatLevel, layout, ranking, reasons, saveView, savedView, score, squarify, step } from './map.svelte';
import { MOCK_LINKED, MOCK_MISSING, MOCK_SUBMODULE, MOCK_WORKTREE } from './mock';
import type { MapFile, ReviewMap } from './types';

function memoryStorage() {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v), removeItem: (k: string) => void data.delete(k), clear: () => data.clear() };
}

function f(path: string, extra: Partial<MapFile> = {}): MapFile {
  return { path, status: 'modified', size: 100, add: 4, del: 1, logic: 4, noise: 1, sig: 0, edits: 1, fails: 0, viewed: false, threads: 0, test: false, ...extra };
}

function review(): ReviewMap {
  return {
    regions: [
      { worktree: '/w/web', name: 'web', branch: 'feat/x', parent: null, error: null, truncated: false, changed: [f('src/a.rs', { logic: 20, sig: 1, edits: 6, fails: 2 }), f('src/b.rs', { logic: 2, noise: 30, viewed: true }), f('tests/a.rs', { test: true, logic: 5 })], tree: [['src/c.rs', 300], ['README.md', 50]] },
      { worktree: '/w/api', name: 'api', branch: 'feat/x', parent: null, error: null, truncated: false, changed: [f('src/push.ts', { logic: 9, threads: 2 })], tree: [['src/server.ts', 400]] },
    ],
    links: [
      { from: { w: 0, path: 'src/b.rs' }, to: { w: 0, path: 'src/a.rs' }, kind: 'uses', count: 1, cross: false },
      { from: { w: 0, path: 'tests/a.rs' }, to: { w: 0, path: 'src/b.rs' }, kind: 'uses', count: 1, cross: false },
      { from: { w: 1, path: 'src/push.ts' }, to: { w: 0, path: 'src/a.rs' }, kind: 'uses', count: 1, cross: true, probable: true },
    ],
  };
}

const by = (m: ReviewMap) => new Map(score(m).map((s) => [s.file.path, s]));

describe('map signals', () => {
  it('derives each signal from the file and its links', () => {
    const s = by(review());
    const a = s.get('src/a.rs')!;
    expect(a.raw).toEqual({ logic: 20, reach: 3, contract: 2, struggle: 10, untested: 20, unseen: 20 });
    expect(s.get('src/b.rs')!.raw.untested).toBe(0);
    expect(s.get('src/b.rs')!.raw.unseen).toBe(0);
    expect(s.get('tests/a.rs')!.raw.untested).toBe(0);
    expect(s.get('src/push.ts')!.raw.unseen).toBe(9 + 6);
    expect(a.inbound).toHaveLength(2);
    expect(s.get('src/push.ts')!.outbound).toHaveLength(1);
  });

  it('scales every signal to 0..1 and makes priority the weighted mean, the top file at 1', () => {
    const list = score(review());
    for (const s of list) for (const v of Object.values(s.score)) expect(v).toBeGreaterThanOrEqual(0);
    expect(Math.max(...list.map((s) => s.score.priority))).toBe(1);
    expect(Math.max(...list.map((s) => s.score.logic))).toBe(1);
    expect(ranking(list, 'priority')[0].file.path).toBe('src/a.rs');
    expect(WEIGHTS.logic).toBeGreaterThan(WEIGHTS.unseen);
  });

  it('gives zeros, not NaN, when a signal is absent everywhere', () => {
    const m = review();
    for (const r of m.regions) for (const x of r.changed) Object.assign(x, { edits: 0, fails: 0 });
    for (const s of score(m)) expect(s.score.struggle).toBe(0);
    expect(score({ regions: [], links: [] })).toEqual([]);
  });

  it('ranks by the picked indicator, ties by worktree then path', () => {
    const m = review();
    for (const r of m.regions) for (const x of r.changed) x.sig = 0;
    expect(ranking(score(m), 'contract').map((s) => s.file.path)).toEqual(['src/a.rs', 'src/b.rs', 'tests/a.rs', 'src/push.ts']);
    expect(ranking(score(m), 'struggle')[0].file.path).toBe('src/a.rs');
  });

  it('explains a rank in plain reasons', () => {
    const s = by(review());
    expect(reasons(s.get('src/a.rs')!).map((r) => r.key)).toEqual(['map.why.logicNoise', 'map.why.usedCross', 'map.why.contract', 'map.why.struggleFails', 'map.why.untested', 'map.why.unseen']);
    expect(reasons(s.get('src/push.ts')!).map((r) => r.key)).toContain('map.why.threads');
    const noise = by({ regions: [{ ...review().regions[0], changed: [f('x.rs', { logic: 0, noise: 9, edits: 3 })] }], links: [] }).get('x.rs')!;
    expect(reasons(noise).map((r) => r.key)).toEqual(['map.why.noiseOnly', 'map.why.struggle']);
  });
});

describe('heat colors', () => {
  it('maps a score to one of six theme tokens, and no heat to a neutral fill', () => {
    expect([0, 0.01, 0.2, 0.5, 0.99, 1].map(heatLevel)).toEqual([-1, 0, 1, 3, 5, 5]);
    expect(heat(0.5)).toBe('var(--heat-3)');
    expect(heatInk(0.5)).toBe('var(--heat-ink-3)');
    expect(heat(0)).toBe('var(--surface-active)');
    expect(heatInk(0)).toBe('var(--text-2)');
  });
});

describe('treemap layout', () => {
  it('fills the box with areas proportional to sizes', () => {
    const items = [{ size: 6 }, { size: 3 }, { size: 1 }, { size: 0 }];
    const r = squarify(items, { x: 0, y: 0, w: 100, h: 50 });
    expect(r).toHaveLength(3);
    const area = r.reduce((a, x) => a + x.w * x.h, 0);
    expect(area).toBeCloseTo(5000);
    expect(r[0].w * r[0].h).toBeCloseTo(3000);
    for (const x of r) {
      expect(x.x).toBeGreaterThanOrEqual(-1e-9);
      expect(x.x + x.w).toBeLessThanOrEqual(100 + 1e-9);
      expect(x.y + x.h).toBeLessThanOrEqual(50 + 1e-9);
    }
    expect(squarify([], { x: 0, y: 0, w: 10, h: 10 })).toEqual([]);
    expect(squarify(items, { x: 0, y: 0, w: 0, h: 10 })).toEqual([]);
  });

  it('draws one region per worktree, every changed file as its own cell and unchanged folders as blocks', () => {
    const m = review();
    m.regions[0].tree.push(['docs/a.md', 900], ['docs/b.md', 900]);
    const g = layout(m, { x: 0, y: 0, w: 900, h: 600 });
    expect(g.regions.map((r) => r.region.name).sort()).toEqual(['api', 'web']);
    for (const r of m.regions) for (const c of r.changed) expect(g.cells.some((x) => x.changed && x.path === c.path)).toBe(true);
    const docs = g.cells.find((c) => c.path === 'docs/');
    expect(docs).toMatchObject({ block: true, count: 2, changed: false });
    expect(g.cells.find((c) => c.path === 'README.md')).toMatchObject({ block: false, changed: false });
    expect(g.dirs.some((d) => d.label === 'src')).toBe(true);
  });

  it('keeps a small worktree readable next to a big one', () => {
    const m = review();
    m.regions[0].tree = Array.from({ length: 200 }, (_, i) => [`src/u${i}.rs`, 5000] as [string, number]);
    const g = layout(m, { x: 0, y: 0, w: 1000, h: 600 });
    const api = g.regions.find((r) => r.region.name === 'api')!;
    expect(api.w * api.h).toBeGreaterThan(1000 * 600 * 0.05);
  });

  it('groups a folder of many unchanged files into one block', () => {
    const m = review();
    m.regions[0].tree = Array.from({ length: 300 }, (_, i) => [`src/u${i}.rs`, 10 + i] as [string, number]);
    const g = layout(m, { x: 0, y: 0, w: 1000, h: 600 });
    const rest = g.cells.find((c) => c.label === '…');
    expect(rest?.count).toBe(260);
    expect(g.cells.filter((c) => c.w0 === 0).length).toBeLessThan(60);
  });
});

describe('list band and walk', () => {
  it('sizes each file of the list by its changed lines, in list order', () => {
    const s = score(review());
    const order = [fid(1, 'src/push.ts'), fid(0, 'src/a.rs'), 'missing'];
    const b = band(s, order, 'priority');
    expect(b.map((x) => x.id)).toEqual([fid(1, 'src/push.ts'), fid(0, 'src/a.rs')]);
    expect(b[0].share + b[1].share).toBeCloseTo(1);
    expect(b[1].heat).toBe(1);
  });

  it('walks the ranking forward and back, wrapping at both ends', () => {
    const r = ranking(score(review()), 'priority');
    expect(step(r, null, 1)).toBe(r[0]);
    expect(step(r, null, -1)).toBe(r[r.length - 1]);
    expect(step(r, r[0].id, 1)).toBe(r[1]);
    expect(step(r, r[0].id, -1)).toBe(r[r.length - 1]);
    expect(step([], null, 1)).toBeUndefined();
  });
});

describe('saved view', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('defaults to the list, keeps the map, and opens an old graph choice as the map', () => {
    vi.stubGlobal('localStorage', memoryStorage());
    expect(savedView()).toBe('list');
    saveView('map');
    expect(savedView()).toBe('map');
    localStorage.setItem('overseer.review.view', 'graph');
    expect(savedView()).toBe('map');
    saveView('list');
    expect(savedView()).toBe('list');
  });

  it('survives storage that throws or is missing', () => {
    const boom = () => {
      throw new Error('blocked');
    };
    vi.stubGlobal('localStorage', { getItem: boom, setItem: boom });
    expect(savedView()).toBe('list');
    expect(() => saveView('map')).not.toThrow();
    vi.stubGlobal('localStorage', undefined);
    expect(savedView()).toBe('list');
    expect(() => saveView('map')).not.toThrow();
  });
});

describe('ReviewMapStore', () => {
  afterEach(() => vi.useRealTimers());

  const fake = (impl: (also: string[]) => Promise<ReviewMap>) => {
    const map = vi.fn((_t, also: string[] = []) => impl(also));
    return { api: { map } as unknown as ReviewApi, map };
  };

  it('loads at once the first time, then waits for changes to settle and asks once per key', async () => {
    vi.useFakeTimers();
    const { api, map } = fake(async () => review());
    const s = new ReviewMapStore(() => api);
    s.request('k1', { worktree: '/w/web' }, ['/w/api']);
    await vi.waitFor(() => expect(s.data).not.toBeNull());
    expect(map).toHaveBeenCalledWith({ worktree: '/w/web' }, ['/w/api']);
    expect(s.scored).toHaveLength(4);
    s.request('k1', { worktree: '/w/web' }, []);
    s.request('k2', { worktree: '/w/web' }, []);
    s.request('k3', { worktree: '/w/web' }, []);
    expect(map).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(300);
    expect(map).toHaveBeenCalledTimes(2);
    s.stop();
  });

  it('keeps the newest answer when an older one arrives late', async () => {
    let release!: (m: ReviewMap) => void;
    const slow = new Promise<ReviewMap>((r) => (release = r));
    const fresh = review();
    fresh.regions[0].name = 'fresh';
    const { api } = fake((also) => (also.length ? slow : Promise.resolve(fresh)));
    const s = new ReviewMapStore(() => api);
    s.load({ worktree: '/w/web' }, ['/w/api']);
    await s.load({ worktree: '/w/web' }, []);
    release(review());
    await Promise.resolve();
    expect(s.data?.regions[0].name).toBe('fresh');
    expect(s.loading).toBe(false);
  });

  it('shows the error and asks again for the same key after a failure', async () => {
    let fail = true;
    const { api, map } = fake(async () => {
      if (fail) throw new DaemonError(410, 'the worktree folder /w/gone no longer exists');
      return review();
    });
    const s = new ReviewMapStore(() => api);
    s.request('k', { worktree: '/w/web' }, [], 0);
    await vi.waitFor(() => expect(s.error).toBe('the worktree folder /w/gone no longer exists'));
    fail = false;
    s.request('k', { worktree: '/w/web' }, [], 0);
    await vi.waitFor(() => expect(s.data).not.toBeNull());
    expect(map).toHaveBeenCalledTimes(2);
    expect(s.error).toBe('');
  });

  it('does nothing without a connection', async () => {
    const s = new ReviewMapStore(() => null);
    await s.load({ worktree: '/w/web' }, []);
    expect(s.data).toBeNull();
    expect(s.loading).toBe(false);
  });
});

describe('mock /v1/review/map', () => {
  const api = () => reviewApi(new MockDaemon({ live: false, delayMs: 0 }));

  it('maps the review worktree, its submodule and the linked worktree passed along', async () => {
    const m = await api().map({ worktree: MOCK_WORKTREE }, [MOCK_LINKED]);
    expect(m.regions.map((r) => [r.worktree, r.parent])).toEqual([
      [MOCK_WORKTREE, null],
      [MOCK_SUBMODULE, 0],
      [MOCK_LINKED, null],
    ]);
    const main = m.regions[0];
    expect(main.changed.length).toBeGreaterThan(2);
    expect(main.changed.some((x) => x.path === 'vendor/ghostty-web')).toBe(false);
    expect(main.tree.length).toBeGreaterThan(5);
    expect(main.changed.every((x) => x.logic + x.noise === x.add + x.del)).toBe(true);
    expect(m.links.length).toBeGreaterThan(0);
    const s = score(m);
    expect(s.some((x) => x.score.priority === 1)).toBe(true);
  });

  it('refuses a relative or missing extra worktree with the daemon messages', async () => {
    await expect(api().map({ worktree: MOCK_WORKTREE }, ['rel/dir'])).rejects.toMatchObject({ status: 400, message: 'rel/dir is not an absolute path' });
    await expect(api().map({ worktree: MOCK_WORKTREE }, [MOCK_MISSING])).rejects.toMatchObject({ status: 410, message: `the worktree folder ${MOCK_MISSING} no longer exists` });
  });
});
