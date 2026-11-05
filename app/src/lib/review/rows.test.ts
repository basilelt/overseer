import { describe, expect, it } from 'vitest';
import { lineDiff } from './linediff';
import { buildRows, fileTree, gaps, keepUnchanged, linesFor, openHunks, placeThreads, rowAnchors, totals, type FoldRow, type GapRow, type LineRow, type PairRow } from './rows';
import type { ReviewFile, Thread } from './types';

const base = Array.from({ length: 40 }, (_, i) => `line ${i + 1}`);
const next = base.slice();
next[4] = 'changed 5';
next.splice(20, 1);
next.splice(30, 0, 'new A', 'new B');

function file(over: Partial<ReviewFile> = {}): ReviewFile {
  return { path: 'src/a.rs', oldPath: 'src/a.rs', status: 'modified', binary: false, additions: 3, deletions: 2, hash: 'h', viewed: false, truncated: false, hunks: lineDiff(base, next), ...over };
}

function thread(over: Partial<Thread>): Thread {
  return { id: 't', worktree: '/w', path: 'src/a.rs', side: 'new', line: 1, endLine: 1, originalLine: 1, originalEndLine: 1, code: '', status: 'sent', outdated: false, changed: false, createdAt: '2026-10-29T09:00:00Z', updatedAt: '', comments: [], ...over };
}

describe('lineDiff', () => {
  it('produces hunks with three lines of context', () => {
    const h = lineDiff(base, next);
    expect(h.map((x) => x.header)).toEqual(['@@ -2,7 +2,7 @@', '@@ -18,7 +18,6 @@', '@@ -29,6 +28,8 @@']);
    expect(h[0].lines.filter((l) => l.kind === 'add')).toEqual([{ kind: 'add', new: 5, text: 'changed 5' }]);
  });
});

describe('rows', () => {
  it('collapses unchanged lines into gaps between hunks', () => {
    const g = gaps(file(), next.length);
    expect(g.map((x) => [x.newStart, x.count])).toEqual([
      [1, 1],
      [9, 9],
      [24, 4],
      [36, 6],
    ]);
    const rows = buildRows(file(), 'unified');
    const gapRows = rows.filter((r): r is GapRow => r.type === 'gap');
    expect(gapRows.at(-1)!.count).toBeNull();
  });

  it('expands a gap from the new file with offset old numbers', () => {
    const rows = buildRows(file(), 'unified', new Set(['g2']), next);
    const expanded = rows.filter((r): r is LineRow => r.type === 'line' && r.new! >= 24 && r.new! <= 26);
    expect(expanded.map((r) => [r.old, r.new, r.text])).toEqual([
      [25, 24, 'line 25'],
      [26, 25, 'line 26'],
      [27, 26, 'line 27'],
    ]);
    const end = buildRows(file(), 'unified', new Set(['end']), next).filter((r) => r.type === 'line').at(-1) as LineRow;
    expect([end.old, end.new, end.text]).toEqual([40, 41, 'line 40']);
  });

  it('pairs deletions and additions side by side in split mode', () => {
    const rows = buildRows(file(), 'split').filter((r): r is PairRow => r.type === 'pair');
    const changed = rows.find((r) => r.left?.kind === 'del' && r.right?.kind === 'add')!;
    expect(changed.left).toEqual({ kind: 'del', no: 5, text: 'line 5' });
    expect(changed.right).toEqual({ kind: 'add', no: 5, text: 'changed 5' });
    const onlyAdd = rows.filter((r) => !r.left && r.right?.kind === 'add');
    expect(onlyAdd.map((r) => r.right!.text)).toEqual(['new A', 'new B']);
    const onlyDel = rows.filter((r) => r.left?.kind === 'del' && !r.right);
    expect(onlyDel.map((r) => r.left!.text)).toEqual(['line 21']);
  });

  it('has no gaps for added files', () => {
    const f = file({ status: 'untracked', hunks: lineDiff([], ['a', 'b']) });
    expect(gaps(f, 2)).toEqual([{ id: 'g0', header: '@@ -0,0 +1,2 @@', oldStart: 1, newStart: 1, count: 0 }]);
  });
});

describe('thread placement', () => {
  it('places threads under their last line and lists unplaced or outdated ones on top', () => {
    const rows = buildRows(file(), 'unified');
    const p = placeThreads(rows, [
      thread({ id: 'a', line: 4, endLine: 5 }),
      thread({ id: 'b', side: 'old', line: 21, endLine: 21 }),
      thread({ id: 'c', line: 15, endLine: 15 }),
      thread({ id: 'd', line: 5, endLine: 5, outdated: true }),
    ]);
    const placed = [...p.byRow.entries()].map(([i, ts]) => [rowAnchors(rows[i]), ts.map((t) => t.id)]);
    expect(placed).toEqual([
      [['new:5'], ['a']],
      [['old:21'], ['b']],
    ]);
    expect(p.top.map((t) => t.id)).toEqual(['d', 'c']);
  });

  it('finds the selected lines on either side', () => {
    expect(linesFor(file(), 'new', 4, 6)).toEqual(['line 4', 'changed 5', 'line 6']);
    expect(linesFor(file(), 'old', 21, 21)).toEqual(['line 21']);
    expect(linesFor(file(), 'new', 12, 12, next)).toEqual(['line 12']);
  });
});

describe('tree and totals', () => {
  it('builds a compact folder tree', () => {
    const files = ['src/a.rs', 'src/b/c.rs', 'app/src/lib/x.ts', 'README.md'].map((path) => file({ path }));
    const tree = fileTree(files);
    expect(tree.map((n) => [n.name, n.dir])).toEqual([
      ['app/src/lib', true],
      ['src', true],
      ['README.md', false],
    ]);
    expect(tree[1].children.map((n) => n.name)).toEqual(['b', 'a.rs']);
    expect(totals([file({ viewed: true }), file()])).toEqual({ additions: 6, deletions: 4, viewed: 1 });
  });
});

describe('keepUnchanged', () => {
  it('keeps the previous object of files whose hash and flags did not change', () => {
    const a = file({ path: 'a', hash: '1' });
    const b = file({ path: 'b', hash: '2' });
    const next = [file({ path: 'a', hash: '1' }), file({ path: 'b', hash: '3' })];
    const out = keepUnchanged([a, b], next);
    expect(out[0]).toBe(a);
    expect(out[1]).toBe(next[1]);
  });

  it('returns the previous array when nothing changed', () => {
    const prev = [file({ path: 'a', hash: '1' }), file({ path: 'b', hash: '2' })];
    expect(keepUnchanged(prev, [file({ path: 'a', hash: '1' }), file({ path: 'b', hash: '2' })])).toBe(prev);
  });

  it('replaces files whose viewed or truncated flag changed, and handles added files', () => {
    const a = file({ path: 'a', hash: '1' });
    const next = [file({ path: 'a', hash: '1', viewed: true }), file({ path: 'c', hash: '9' })];
    const out = keepUnchanged([a], next);
    expect(out[0]).toBe(next[0]);
    expect(out[1]).toBe(next[1]);
    expect(keepUnchanged(undefined, next)).toBe(next);
  });
});

describe('triage folds', () => {
  const tagged = () => {
    const f = file();
    f.hunks[0].tags = [{ kind: 'format', from: 3, to: 5 }];
    f.hunks[1].tags = [{ kind: 'move', from: 3, to: 4, ref: { path: 'src/b.rs', hunk: 2 } }];
    f.hunks[2].tags = [{ kind: 'logic', from: 3, to: 5, sig: true }];
    return f;
  };

  it('collapses low-risk hunks into one fold row each', () => {
    const rows = buildRows(tagged(), 'unified');
    const folds = rows.filter((r): r is FoldRow => r.type === 'fold');
    expect(folds.map((r) => [r.hunk, r.kind, r.count])).toEqual([
      [0, 'format', 2],
      [1, 'move', 1],
    ]);
    expect(folds[1].ref).toEqual({ path: 'src/b.rs', hunk: 2 });
    const lines = rows.filter((r): r is LineRow => r.type === 'line');
    expect(lines.every((r) => r.new! > 25 || r.old! > 25)).toBe(true);
    expect(lines.filter((r) => r.tag === 'logic' && r.sig).map((r) => r.text)).toEqual(['new A', 'new B']);
  });

  it('expands a fold on demand and keeps the tags on the lines', () => {
    const rows = buildRows(tagged(), 'split', undefined, undefined, new Set([0]));
    expect(rows.filter((r) => r.type === 'fold')).toHaveLength(1);
    const pair = rows.find((r): r is PairRow => r.type === 'pair' && r.right?.text === 'changed 5')!;
    expect(pair.right?.tag).toBe('format');
    expect(pair.left?.tag).toBe('format');
  });

  it('opens hunks holding a thread and never folds untagged diffs', () => {
    expect([...openHunks(tagged(), [1], [{ side: 'new', line: 5 }, { side: 'old', line: 99 }])].sort()).toEqual([0, 1]);
    expect(buildRows(file(), 'unified').some((r) => r.type === 'fold')).toBe(false);
    expect(openHunks(file(), undefined, [{ side: 'new', line: 5 }]).size).toBe(0);
  });

  it('refreshes a file whose risk or tags changed with the same hash', () => {
    const a = tagged();
    const b = { ...tagged(), risk: 40 };
    expect(keepUnchanged([a], [b])[0]).toBe(b);
    const c = tagged();
    expect(keepUnchanged([a], [c])[0]).toBe(a);
    c.hunks[0].tags = [{ kind: 'logic', from: 3, to: 5 }];
    expect(keepUnchanged([a], [c])[0]).toBe(c);
  });
});
