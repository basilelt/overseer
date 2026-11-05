import { describe, expect, it } from 'vitest';
import { layout, maxWidth, type Row } from './layout';

const c = (sha: string, ...parents: string[]) => ({ sha, parents });

function segs(r: Row, kind: 'pass' | 'in' | 'out') {
  return r.segs
    .filter((s) => s.kind === kind)
    .map((s) => [s.from, s.to])
    .sort((a, b) => a[0] - b[0] || a[1] - b[1]);
}

describe('layout', () => {
  it('keeps a linear history on one lane', () => {
    const { rows, state } = layout([c('d', 'c'), c('c', 'b'), c('b', 'a'), c('a')]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 0, 0]);
    expect(new Set(rows.map((r) => r.color)).size).toBe(1);
    expect(segs(rows[0], 'in')).toEqual([]);
    expect(segs(rows[1], 'in')).toEqual([[0, 0]]);
    expect(segs(rows[3], 'out')).toEqual([]);
    expect(state.lanes).toEqual([]);
  });

  it('opens a lane for the second parent of a merge and closes it at the fork', () => {
    const { rows } = layout([c('m', 'a2', 'b1'), c('a2', 'a1'), c('b1', 'a1'), c('a1', 'r'), c('r')]);
    expect(rows.map((r) => r.lane)).toEqual([0, 0, 1, 0, 0]);
    expect(segs(rows[0], 'out')).toEqual([
      [0, 0],
      [0, 1],
    ]);
    expect(rows[0].color).not.toBe(rows[2].color);
    expect(segs(rows[1], 'pass')).toEqual([[1, 1]]);
    expect(segs(rows[2], 'pass')).toEqual([[0, 0]]);
    expect(segs(rows[2], 'out')).toEqual([[1, 0]]);
    expect(segs(rows[3], 'in')).toEqual([[0, 0]]);
    expect(rows[3].segs.filter((s) => s.kind === 'pass')).toEqual([]);
    expect(maxWidth(rows)).toBe(2);
  });

  it('lays out an octopus merge on one lane per parent', () => {
    const { rows, state } = layout([c('o', 'p1', 'p2', 'p3', 'p4'), c('p1', 'b'), c('p2', 'b'), c('p3', 'b'), c('p4', 'b'), c('b')]);
    expect(rows[0].lane).toBe(0);
    expect(segs(rows[0], 'out')).toEqual([
      [0, 0],
      [0, 1],
      [0, 2],
      [0, 3],
    ]);
    expect(new Set(rows[0].segs.map((s) => s.color)).size).toBe(4);
    expect(rows.slice(1, 5).map((r) => r.lane)).toEqual([0, 1, 2, 3]);
    expect(segs(rows[5], 'in')).toEqual([[0, 0]]);
    expect(rows[4].width).toBe(4);
    expect(state.lanes).toEqual([]);
  });

  it('routes crossing branches through stable lanes and reuses freed slots', () => {
    const history = [c('x2', 'x1'), c('y2', 'y1'), c('x1', 'base'), c('z1', 'y1'), c('y1', 'base'), c('w1', 'base'), c('base')];
    const { rows } = layout(history);
    expect(rows.map((r) => r.lane)).toEqual([0, 1, 0, 2, 1, 1, 0]);
    expect(segs(rows[3], 'out')).toEqual([[2, 1]]);
    expect(segs(rows[4], 'in')).toEqual([[1, 1]]);
    expect(segs(rows[4], 'out')).toEqual([[1, 0]]);
    expect(segs(rows[5], 'pass')).toEqual([[0, 0]]);
    expect(segs(rows[5], 'out')).toEqual([[1, 0]]);
    expect(segs(rows[6], 'in')).toEqual([[0, 0]]);
  });

  it('keeps a lane open for a parent missing from the page', () => {
    const { rows, state } = layout([c('b', 'a'), c('side', 'gone'), c('a')]);
    expect(rows[2].lane).toBe(0);
    expect(segs(rows[2], 'pass')).toEqual([[1, 1]]);
    expect(state.lanes).toEqual([null, 'gone']);
  });

  it('gives the same rows when computed over two pages', () => {
    const history = [c('m', 'a3', 'b2'), c('b2', 'b1'), c('a3', 'a2'), c('b1', 'a1'), c('a2', 'a1'), c('t', 'a1'), c('a1', 'r'), c('r')];
    const whole = layout(history);
    const first = layout(history.slice(0, 3));
    const second = layout(history.slice(3), first.state);
    expect([...first.rows, ...second.rows]).toEqual(whole.rows);
    expect(second.state).toEqual(whole.state);
  });

  it('lays out 10k commits fast', () => {
    const commits: { sha: string; parents: string[] }[] = [];
    for (let i = 0; i < 10_000; i++) {
      const parents = [`c${i + 1}`];
      if (i % 7 === 0 && i + 23 < 10_000) parents.push(`c${i + 23}`);
      commits.push({ sha: `c${i}`, parents: i === 9_999 ? [] : parents });
    }
    const t0 = performance.now();
    const { rows } = layout(commits);
    const ms = performance.now() - t0;
    console.info(`layout 10k commits: ${ms.toFixed(1)} ms, max lanes ${maxWidth(rows)}`);
    expect(rows).toHaveLength(10_000);
    expect(ms).toBeLessThan(500);
  });
});
