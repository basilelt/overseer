import { describe, expect, it } from 'vitest';
import { byRisk, foldOf, lowRiskKinds } from './triage';
import type { ReviewFile, ReviewHunk } from './types';

const hunk = (tags: ReviewHunk['tags']): ReviewHunk => ({ header: '', oldStart: 1, oldLines: 1, newStart: 1, newLines: 1, lines: [], tags });
const f = (path: string, risk?: number, residual?: number) => ({ path, risk, residual }) as ReviewFile;

describe('triage helpers', () => {
  it('folds only hunks whose every run is low-risk', () => {
    expect(foldOf(hunk([{ kind: 'format', from: 0, to: 2 }, { kind: 'comment', from: 3, to: 4 }]))).toEqual({ kind: 'mixed', ref: undefined, count: 3 });
    expect(foldOf(hunk([{ kind: 'move', from: 0, to: 4, ref: { path: 'a', hunk: 1 } }]))?.ref).toEqual({ path: 'a', hunk: 1 });
    expect(foldOf(hunk([{ kind: 'format', from: 0, to: 2 }, { kind: 'logic', from: 2, to: 3 }]))).toBeNull();
    expect(foldOf(hunk(undefined))).toBeNull();
  });

  it('names the main low-risk kinds and ranks files by risk', () => {
    expect(lowRiskKinds({ changed: 10, byKind: { logic: 5, format: 1, move: 3 }, lowRiskPct: 40 })).toEqual(['move', 'format']);
    expect(lowRiskKinds(undefined)).toEqual([]);
    expect(byRisk([f('b', 10, 1), f('a', 90, 3), f('c'), f('d', 10, 4)]).map((x) => x.path)).toEqual(['a', 'd', 'b', 'c']);
  });
});
