import { describe, expect, it } from 'vitest';
import { band, layout, ranking, score } from '../review/map.svelte';
import type { MapFile, ReviewMap } from '../review/types';
import { counted, expectLinear } from './work';

function file(path: string, i: number): MapFile {
  return { path, status: 'modified', size: 400 + (i % 97), add: 3 + (i % 9), del: i % 4, logic: 2 + (i % 7), noise: i % 3, sig: i % 5 === 0 ? 1 : 0, edits: i % 6, fails: i % 11 === 0 ? 1 : 0, viewed: i % 2 === 0, threads: i % 13 === 0 ? 1 : 0, test: i % 8 === 0 };
}

function review(scale: number): ReviewMap {
  const regions = Array.from({ length: 3 }, (_, w) => ({
    worktree: `/w/${w}`,
    name: `w${w}`,
    branch: 'main',
    parent: null,
    error: null,
    truncated: false,
    changed: Array.from({ length: 30 * scale }, (_, i) => file(`src/d${i % 25}/c${i}.rs`, i)),
    tree: Array.from({ length: 300 * scale }, (_, i) => [`src/d${i % 60}/u${i}.rs`, 100 + i] as [string, number]),
  }));
  const links = regions.flatMap((r, w) => r.changed.slice(1).map((f, i) => ({ from: { w, path: f.path }, to: { w: (w + (i % 7 === 0 ? 1 : 0)) % 3, path: r.changed[i].path }, kind: 'uses' as const, count: 1, cross: i % 7 === 0 })));
  return { regions, links };
}

describe('review map at scale', () => {
  it('scores and ranks changed files in work linear to the review', () => {
    expect(score(review(1))).toHaveLength(90);
    const make = (n: number) => () => {
      const m = counted(review(n));
      return () => void ranking(score(m), 'priority');
    };
    expectLinear(make(1), make(10));
  });

  it('lays out the map in work linear to the repository size', () => {
    const make = (n: number) => () => {
      const m = counted(review(n));
      return () => void layout(m, { x: 0, y: 0, w: 1200, h: 700 });
    };
    expectLinear(make(1), make(10));
  });

  it('keeps the number of cells bounded however many unchanged files a folder holds', () => {
    const m = review(1);
    m.regions[0].tree = Array.from({ length: 20000 }, (_, i) => [`src/d0/u${i}.rs`, 10] as [string, number]);
    const g = layout(m, { x: 0, y: 0, w: 1200, h: 700 });
    expect(g.cells.length).toBeLessThan(1000);
  });

  it('builds the list band in work linear to the files shown', () => {
    const make = (n: number) => () => {
      const m = review(n);
      const s = score(m);
      const order = counted(s.map((x) => x.id));
      return () => void band(s, order, 'priority');
    };
    expectLinear(make(1), make(10));
  });
});
