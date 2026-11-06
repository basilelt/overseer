import { describe, expect, it } from 'vitest';
import { parseDiff, tokenize } from '../diff';
import { layout } from '../git/layout';
import { expectLinear, counted } from './work';
import { graphOrder, nextStep } from '../review/linked.svelte';
import { buildRows, fileTree, keepUnchanged, placeThreads } from '../review/rows';
import type { ReviewStore } from '../review/store.svelte';
import type { ReviewFile, ReviewHunk, ReviewLine, Thread } from '../review/types';

function hunk(k: number): ReviewHunk {
  const start = k * 40 + 1;
  const lines: ReviewLine[] = [];
  for (let i = 0; i < 3; i++) lines.push({ kind: 'ctx', old: start + i, new: start + i, text: `  let a${i} = ${k};` });
  for (let i = 0; i < 4; i++) lines.push({ kind: 'del', old: start + 3 + i, text: `  old(${k}, ${i});` });
  for (let i = 0; i < 5; i++) lines.push({ kind: 'add', new: start + 3 + i, text: `  next(${k}, ${i});` });
  for (let i = 0; i < 3; i++) lines.push({ kind: 'ctx', old: start + 7 + i, new: start + 8 + i, text: '  }' });
  return { header: `@@ -${start},10 +${start},11 @@ fn f${k}()`, oldStart: start, oldLines: 10, newStart: start, newLines: 11, lines };
}

function file(path: string, hunks: number, hash = 'h0'): ReviewFile {
  const hs = Array.from({ length: hunks }, (_, k) => hunk(k));
  return { path, oldPath: path, status: 'modified', binary: false, additions: hunks * 5, deletions: hunks * 4, hash, viewed: false, truncated: false, hunks: hs };
}

function threads(f: ReviewFile, n: number): Thread[] {
  return Array.from({ length: n }, (_, i) => {
    const h = f.hunks[(i * 7) % f.hunks.length];
    const line = h.newStart + 3 + (i % 5);
    return { id: `t${i}`, worktree: '/w', path: f.path, side: 'new', line, endLine: line, originalLine: line, originalEndLine: line, code: '', status: 'pending', outdated: i % 11 === 0, changed: false, createdAt: `2026-10-29T10:${String(i % 60).padStart(2, '0')}:00Z`, updatedAt: '', comments: [] } as Thread;
  });
}

function patch(files: number, hunks: number): string {
  const out: string[] = [];
  for (let f = 0; f < files; f++) {
    out.push(`diff --git a/src/m${f}.rs b/src/m${f}.rs`, `--- a/src/m${f}.rs`, `+++ b/src/m${f}.rs`);
    for (const h of Array.from({ length: hunks }, (_, k) => hunk(k))) {
      out.push(h.header);
      for (const l of h.lines) out.push((l.kind === 'add' ? '+' : l.kind === 'del' ? '-' : ' ') + l.text);
    }
  }
  return out.join('\n');
}

describe('review diff at scale', () => {
  it('a refreshed diff keeps every unchanged file object', () => {
    const prev = Array.from({ length: 500 }, (_, i) => file(`src/d${i % 25}/f${i}.ts`, 2));
    const same = prev.map((f) => ({ ...f, hunks: f.hunks.map((h) => ({ ...h })) }));
    expect(keepUnchanged(prev, same)).toBe(prev);
    const next = same.map((f, i) => (i === 123 ? { ...f, hash: 'h1' } : f));
    const kept = keepUnchanged(prev, next);
    expect(kept).not.toBe(prev);
    expect(kept.filter((f, i) => f !== prev[i]).map((f) => f.path)).toEqual([prev[123].path]);
  });

  it('unified and split rows scale linearly with hunk count', () => {
    expect(buildRows(file('a.rs', 1000), 'unified').length).toBe(buildRows(file('a.rs', 100), 'unified').length * 10 - 9);
    for (const mode of ['unified', 'split'] as const) {
      const make = (hunks: number) => () => {
        const f = counted(file('a.rs', hunks));
        return () => void buildRows(f, mode);
      };
      expectLinear(make(100), make(1000));
    }
  });

  it('placing review threads scales linearly with rows and threads', () => {
    const input = (hunks: number, n: number) => {
      const f = file('a.rs', hunks);
      return { rows: buildRows(f, 'unified'), ts: threads(f, n) };
    };
    const big = input(2000, 400);
    const placed = placeThreads(big.rows, big.ts);
    expect([...placed.byRow.values()].flat().length + placed.top.length).toBe(400);
    const make = (hunks: number, n: number) => () => {
      const i = input(hunks, n);
      const rows = counted(i.rows);
      const ts = counted(i.ts);
      return () => void placeThreads(rows, ts);
    };
    expectLinear(make(200, 40), make(2000, 400));
  });

  it('the file tree scales linearly for nested paths', () => {
    const files = (n: number) => Array.from({ length: n }, (_, i) => file(`src/pkg${i % 5}/mod${i % 2}/f${i}.ts`, 0));
    const make = (n: number) => () => {
      const fs = counted(files(n));
      return () => void fileTree(fs);
    };
    expectLinear(make(500), make(5000));
  });

  it('the file tree scales linearly for one flat directory', () => {
    const files = (n: number) => Array.from({ length: n }, (_, i) => file(`vendor/f${i}.ts`, 0));
    const make = (n: number) => () => {
      const fs = counted(files(n));
      return () => void fileTree(fs);
    };
    expectLinear(make(200), make(2000));
  });

  it('parsing and tokenizing a unified diff scales linearly', () => {
    expect(parseDiff(patch(200, 20)).length).toBe(200);
    const make = (files: number) => () => {
      const text = patch(files, 20);
      return () => {
        for (const f of parseDiff(text)) for (const h of f.hunks) for (const l of h.lines) tokenize(l.text);
      };
    };
    expectLinear(make(20), make(200));
  });
});

describe('git graph layout at scale', () => {
  function history(n: number) {
    const out: { sha: string; parents: string[] }[] = [];
    for (let i = 0; i < n; i++) {
      const parents = i + 1 < n ? [`c${i + 1}`] : [];
      if (i % 10 === 0 && i + 5 < n) parents.push(`c${i + 5}`);
      out.push({ sha: `c${i}`, parents });
    }
    return out;
  }

  it('lays out commits in time linear to history length', () => {
    expect(layout(history(20000)).rows.length).toBe(20000);
    const make = (n: number) => () => {
      const commits = counted(history(n));
      return () => void layout(commits);
    };
    expectLinear(make(2000), make(20000));
  });
});

describe('review graph order across linked worktrees at scale', () => {
  function stores(perStore: number) {
    return Array.from({ length: 5 }, (_, w) => {
      const nodes = Array.from({ length: perStore }, (_, i) => ({ id: `n${i}`, name: `n${i}`, path: `w${w}/f${i}.ts`, line: 1, hunks: [] }));
      return { graph: { revision: `r${w}`, nodes, edges: [], order: nodes.map((n) => n.id) }, orderAt: -1 } as unknown as ReviewStore;
    });
  }

  function walk(list: ReviewStore[]): number {
    let steps = 0;
    let cur = list[0];
    cur.orderAt = -1;
    for (let st = nextStep(list, cur); st; st = nextStep(list, cur)) {
      st.store.orderAt = st.at;
      cur = st.store;
      steps++;
    }
    for (const s of list) s.orderAt = -1;
    return steps;
  }

  it('walks every node of every linked graph once, linear in nodes, with the order memoized per graph', () => {
    expect(walk(stores(200))).toBe(1000);
    const big = stores(2000);
    expect(walk(big)).toBe(10000);
    expect(graphOrder(big[1])).toBe(graphOrder(big[1]));
    const make = (n: number) => () => {
      const list = counted(stores(n));
      return () => void walk(list);
    };
    expectLinear(make(200), make(2000));
  });
});
