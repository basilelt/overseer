import { mockGraph } from './mocktriage';
import type { MapFile, MapLink, MapRegion, ReviewDiff, ReviewMap, Thread } from './types';

export interface MockRegion {
  diff: ReviewDiff;
  name: string;
  parent: number | null;
  threads: Thread[];
}

const TREES: Record<string, string[]> = {
  overseer: ['Cargo.toml', 'README.md', 'src/main.rs', 'src/lib.rs', 'src/daemon.rs', 'src/board.rs', 'src/tasks.rs', 'src/chat.rs', 'src/mcp.rs', 'src/git.rs', 'src/review/mod.rs', 'src/review/diff.rs', 'src/review/watch.rs', 'src/sync.rs', 'tests/app.rs', 'tests/review.rs', 'tests/board.rs', 'app/src/App.svelte', 'app/src/lib/state.svelte.ts', 'app/src/lib/mock.ts', 'app/src/lib/components/workspace/TerminalPane.svelte', 'app/src/lib/components/review/ReviewView.svelte', 'docs/user-guide.md'],
  mobile: ['build.gradle', 'android/app/src/main/MainActivity.kt', 'android/app/src/main/Session.kt', 'android/app/src/main/Settings.kt', 'android/app/src/test/SessionTest.kt', 'ios/App/AppDelegate.swift', 'ios/App/Terminal.swift', 'README.md'],
  term: ['package.json', 'src/index.ts', 'src/render.ts', 'src/input.ts', 'src/parser.ts', 'test/render.test.ts'],
};

const isTest = (p: string) => /(^|\/)(tests?|__tests__)\/|\.(test|spec)\.[a-z]+$|_test\.(go|py|rs)$/.test(p);

function hash(s: string): number {
  let h = 2166136261;
  for (let i = 0; i < s.length; i++) h = Math.imul(h ^ s.charCodeAt(i), 16777619) >>> 0;
  return h;
}

function treeFor(wt: string): string[] {
  if (/mobile/.test(wt)) return TREES.mobile;
  if (/ghostty|vendor/.test(wt)) return TREES.term;
  return TREES.overseer;
}

export function mockMap(regions: MockRegion[]): ReviewMap {
  const graphs = regions.map((r) => mockGraph(r.diff.files.filter((f) => f.status !== 'submodule'), r.diff.revision));
  const out: MapRegion[] = regions.map((r, w) => {
    const subs = new Set(regions.filter((x) => x.parent === w).map((x) => x.diff.worktree.slice(r.diff.worktree.length + 1)));
    const changed: MapFile[] = r.diff.files
      .filter((f) => f.status !== 'submodule' && !subs.has(f.path))
      .map((f) => {
        const h = hash(`${r.diff.worktree}/${f.path}`);
        const sig = f.hunks.reduce((s, x) => s + (x.tags ?? []).filter((t) => t.sig).length, 0);
        const residual = f.residual ?? f.additions + f.deletions;
        return {
          path: f.path,
          status: f.status,
          size: 800 + (h % 6000),
          add: f.additions,
          del: f.deletions,
          logic: residual,
          noise: Math.max(0, f.additions + f.deletions - residual),
          sig,
          edits: 1 + (h % 6),
          fails: h % 4 === 0 ? 1 + (h % 3) : 0,
          viewed: f.viewed,
          threads: r.threads.filter((t) => t.path === f.path && (t.status === 'pending' || t.status === 'sent')).length,
          test: isTest(f.path),
        };
      });
    const have = new Set(changed.map((f) => f.path));
    const tree = treeFor(r.diff.worktree)
      .filter((p) => !have.has(p) && !subs.has(p))
      .map((p) => [p, 400 + (hash(p) % 9000)] as [string, number]);
    return { worktree: r.diff.worktree, name: r.name, branch: r.diff.branch ?? null, parent: r.parent, error: null, truncated: false, changed, tree };
  });
  const links = new Map<string, MapLink>();
  const add = (l: MapLink) => {
    const k = `${l.from.w}\n${l.from.path}\n${l.to.w}\n${l.to.path}`;
    const cur = links.get(k);
    if (cur) cur.count++;
    else links.set(k, l);
  };
  graphs.forEach((g, w) => {
    const path = new Map(g.nodes.map((n) => [n.id, n.path]));
    for (const e of g.edges) {
      const a = path.get(e.from), b = path.get(e.to);
      if (a && b && a !== b) add({ from: { w, path: a }, to: { w, path: b }, kind: e.kind, count: 1, cross: false });
    }
  });
  const names = new Map<string, { w: number; path: string }[]>();
  graphs.forEach((g, w) => {
    for (const n of g.nodes) {
      if (n.kind === 'file' || n.name.length < 4) continue;
      const list = names.get(n.name) ?? [];
      if (list.filter((x) => x.w === w).length < 2 && !list.some((x) => x.w === w && x.path === n.path)) list.push({ w, path: n.path });
      names.set(n.name, list);
    }
  });
  for (const list of names.values()) {
    list.forEach((a, i) => {
      for (const b of list.slice(i + 1)) {
        if (a.w === b.w) continue;
        const [from, to] = a.w > b.w ? [a, b] : [b, a];
        add({ from, to, kind: 'uses', count: 1, cross: true, probable: true });
      }
    });
  }
  return { regions: out, links: [...links.values()] };
}
