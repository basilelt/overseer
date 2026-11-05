import { afterEach, describe, expect, it, vi } from 'vitest';
import { drawGraph, readTheme, type Theme } from './draw';
import type { Row, Seg } from './layout';
import { laneX, MAX_LANES, ROW_H } from './view';

type Call = [string, ...unknown[]];

function recorder() {
  const calls: Call[] = [];
  const state: Record<string, unknown> = {};
  const ctx = new Proxy(state, {
    get: (t, k: string) => (k in t ? t[k] : (...a: unknown[]) => void calls.push([k, ...a])),
    set: (t, k: string, v) => {
      t[k] = v;
      calls.push([`=${k}`, v]);
      return true;
    },
  }) as unknown as CanvasRenderingContext2D;
  return { ctx, calls, state };
}

const theme: Theme = { lanes: ['c0', 'c1', 'c2', 'c3', 'c4', 'c5'], accent: 'acc', bg: 'bg' };
const row = (lane: number, segs: Seg[] = [], color = lane): Row => ({ lane, color, segs, width: lane + 1 });
const opts = (over: Partial<Parameters<typeof drawGraph>[5]> = {}) => ({ width: 200, head: -1, merges: () => false, ...over });
const named = (calls: Call[], name: string) => calls.filter((c) => c[0] === name);

describe('drawGraph', () => {
  it('clears only the visible window and draws rows relative to its start', () => {
    const { ctx, calls } = recorder();
    const rows = [row(0), row(1), row(2), row(3)];
    drawGraph(ctx, rows, 1, 3, theme, opts());
    expect(calls[0]).toEqual(['clearRect', 0, 0, 200, 2 * ROW_H]);
    expect(named(calls, 'arc').map((c) => [c[1], c[2]])).toEqual([
      [laneX(1), ROW_H / 2],
      [laneX(2), ROW_H + ROW_H / 2],
    ]);
  });

  it('draws pass-through lines full height and straight in/out lines to the dot', () => {
    const { ctx, calls } = recorder();
    drawGraph(ctx, [row(0, [{ kind: 'pass', from: 1, to: 1, color: 1 }, { kind: 'in', from: 0, to: 0, color: 0 }, { kind: 'out', from: 0, to: 0, color: 0 }])], 0, 1, theme, opts());
    const x0 = laneX(0);
    const x1 = laneX(1);
    expect(named(calls, 'moveTo')).toEqual([
      ['moveTo', x1, 0],
      ['moveTo', x0, 0],
      ['moveTo', x0, ROW_H / 2],
    ]);
    expect(named(calls, 'lineTo')).toEqual([
      ['lineTo', x1, ROW_H],
      ['lineTo', x0, ROW_H / 2],
      ['lineTo', x0, ROW_H],
    ]);
    expect(named(calls, 'bezierCurveTo')).toEqual([]);
    expect(named(calls, '=strokeStyle').slice(0, 3).map((c) => c[1])).toEqual(['c1', 'c0', 'c0']);
  });

  it('curves lines that change lanes', () => {
    const { ctx, calls } = recorder();
    drawGraph(ctx, [row(0, [{ kind: 'in', from: 2, to: 0, color: 2 }, { kind: 'out', from: 0, to: 3, color: 3 }])], 0, 1, theme, opts());
    const curves = named(calls, 'bezierCurveTo');
    expect(curves).toHaveLength(2);
    expect(curves[0].slice(-2)).toEqual([laneX(0), ROW_H / 2]);
    expect(curves[1].slice(-2)).toEqual([laneX(3), ROW_H]);
    expect(named(calls, 'lineTo')).toEqual([]);
  });

  it('skips lines entirely past the lane cap and clamps the rest to the last lane', () => {
    const { ctx, calls } = recorder();
    const far = MAX_LANES + 3;
    drawGraph(ctx, [row(far, [{ kind: 'pass', from: far, to: far, color: 0 }, { kind: 'pass', from: MAX_LANES - 1, to: MAX_LANES - 1, color: 0 }, { kind: 'in', from: far, to: 0, color: 0 }])], 0, 1, theme, opts());
    const last = laneX(MAX_LANES - 1);
    expect(named(calls, 'moveTo')).toEqual([
      ['moveTo', last, 0],
      ['moveTo', last, 0],
    ]);
    expect(named(calls, 'arc')[0].slice(1, 3)).toEqual([last, ROW_H / 2]);
  });

  it('wraps colours around the palette, including negative indexes', () => {
    const { ctx, calls } = recorder();
    drawGraph(ctx, [row(0, [], 7), row(0, [], -1)], 0, 2, theme, opts());
    expect(named(calls, '=fillStyle').map((c) => c[1])).toEqual(['c1', 'c5']);
  });

  it('draws the head as an accent ring, merges as hollow dots and commits as filled dots', () => {
    const { ctx, calls, state } = recorder();
    drawGraph(ctx, [row(0), row(1), row(2)], 0, 3, theme, opts({ head: 0, merges: (i) => i === 1 }));
    const arcs = named(calls, 'arc').map((c) => c[3]);
    expect(arcs).toEqual([5, 3.2, 3.6]);
    const after = (name: string, from: number) => calls.slice(calls.findIndex((c, i) => i >= from && c[0] === 'arc')).find((c) => c[0] === name);
    const headAt = calls.findIndex((c) => c[0] === 'arc');
    expect(after('=fillStyle', headAt)?.[1]).toBe('bg');
    expect(after('=strokeStyle', headAt)?.[1]).toBe('acc');
    expect(named(calls, '=lineWidth').map((c) => c[1])).toEqual([1.6, 2.2, 1.6]);
    const mergeAt = calls.findIndex((c, i) => c[0] === 'arc' && c[3] === 3.2 && i > headAt);
    expect(after('=fillStyle', mergeAt)?.[1]).toBe('bg');
    expect(after('=strokeStyle', mergeAt)?.[1]).toBe('c1');
    const plainAt = calls.findIndex((c) => c[0] === 'arc' && c[3] === 3.6);
    expect(after('=fillStyle', plainAt)?.[1]).toBe('c2');
    expect(calls.slice(plainAt).some((c) => c[0] === 'stroke')).toBe(false);
    expect(state.globalAlpha).toBe(1);
  });

  it('dims filtered-out rows and restores full opacity at the end', () => {
    const { ctx, calls, state } = recorder();
    drawGraph(ctx, [row(0), row(1)], 0, 2, theme, opts({ dimmed: (i) => i === 1 }));
    expect(named(calls, '=globalAlpha').map((c) => c[1])).toEqual([1, 0.35, 1, 0.35, 1]);
    expect(state.globalAlpha).toBe(1);
  });

  it('draws nothing but the clear for an empty window', () => {
    const { ctx, calls } = recorder();
    drawGraph(ctx, [row(0)], 0, 0, theme, opts());
    expect(calls.filter((c) => !c[0].startsWith('='))).toEqual([['clearRect', 0, 0, 200, 0]]);
  });
});

describe('readTheme', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('reads lane colours and accents from CSS, with a grey fallback', () => {
    const vars: Record<string, string> = { '--lane-0': ' #f00 ', '--lane-2': '#0f0', '--accent': ' #00f', '--bg': '#111 ' };
    const getComputedStyle = vi.fn(() => ({ getPropertyValue: (n: string) => vars[n] ?? '' }));
    vi.stubGlobal('getComputedStyle', getComputedStyle);
    const root = {} as Element;
    expect(readTheme(root)).toEqual({ lanes: ['#f00', '#888', '#0f0', '#888', '#888', '#888'], accent: '#00f', bg: '#111' });
    expect(getComputedStyle).toHaveBeenCalledWith(root);
  });
});
