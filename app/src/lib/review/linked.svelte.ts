import { ReviewStore } from './store.svelte';
import { batch, type Batch } from './threads';
import type { GraphNode, ReviewDiff, ReviewFile, ReviewGraph, SubmitKind, SubmitResult } from './types';

export class LinkedReviews {
  stores = $state.raw<ReviewStore[]>([]);
  view: 'list' | 'graph' = 'list';

  sync(linked: ReviewDiff['linked']) {
    const want = (linked ?? []).map((l) => l.worktree);
    const keep = this.stores.filter((s) => want.includes(s.target.worktree ?? ''));
    for (const s of this.stores) if (!keep.includes(s)) s.stop();
    const added = want
      .filter((w) => !keep.some((s) => s.target.worktree === w))
      .map((w) => {
        const s = new ReviewStore({ worktree: w });
        s.setView(this.view);
        s.start();
        return s;
      });
    if (added.length || keep.length !== this.stores.length) this.stores = [...keep, ...added];
  }

  get visible(): ReviewStore[] {
    return this.stores.filter((s) => !!s.diff && (s.diff.files.length > 0 || (s.diff.commits?.length ?? 0) > 0));
  }

  setView(v: 'list' | 'graph') {
    this.view = v;
    for (const s of this.stores) s.setView(v);
  }

  setAutoRefresh(on: boolean) {
    for (const s of this.stores) s.setAutoRefresh(on);
  }

  stop() {
    for (const s of this.stores) s.stop();
    this.stores = [];
  }
}

export function batchAll(primary: ReviewStore, linked: ReviewStore[]): Batch {
  const own = batch(primary.threads);
  const out: Batch = { comments: own.comments, threads: own.threads, files: [...own.files] };
  for (const s of linked) {
    const b = batch(s.threads);
    const name = (s.worktree ?? '').split('/').pop();
    out.comments += b.comments;
    out.threads += b.threads;
    out.files.push(...b.files.map((f) => ({ ...f, path: `${name}/${f.path}` })));
  }
  return out;
}

export async function submitAll(primary: ReviewStore, linked: ReviewStore[], kind: SubmitKind, body: string): Promise<SubmitResult | null> {
  const r = await primary.submit(kind, body);
  if (!r) return null;
  const pending = linked.filter((s) => batch(s.threads).comments > 0);
  await Promise.all(pending.map((s) => s.submit(kind, '')));
  return r;
}

export function fileLabel(f: ReviewFile): string {
  return `${f.path} (+${f.additions} −${f.deletions})${f.viewed ? ' ✓' : ''}`;
}

export function pickerGroups(stores: ReviewStore[]): { worktree: string; label: string; options: { value: string; label: string }[] }[] {
  return stores
    .filter((s) => s.diff?.files.length)
    .map((s) => {
      const wt = s.diff!.worktree;
      return { worktree: wt, label: wt.split('/').pop() ?? wt, options: s.diff!.files.map((f) => ({ value: `${wt}\n${f.path}`, label: fileLabel(f) })) };
    });
}

export function parsePick(v: string): { worktree: string | null; path: string } {
  const i = v.indexOf('\n');
  return i < 0 ? { worktree: null, path: v } : { worktree: v.slice(0, i), path: v.slice(i + 1) };
}

const orders = new WeakMap<ReviewGraph, GraphNode[]>();

export function graphOrder(s: ReviewStore): GraphNode[] {
  const g = s.graph;
  if (!g) return [];
  let o = orders.get(g);
  if (!o) {
    const byId = new Map(g.nodes.map((n) => [n.id, n]));
    o = g.order.map((id) => byId.get(id)).filter((n): n is GraphNode => !!n);
    orders.set(g, o);
  }
  return o;
}

export interface Step {
  store: ReviewStore;
  node: GraphNode;
  at: number;
}

export function nextStep(stores: ReviewStore[], active: ReviewStore): Step | undefined {
  const i = stores.indexOf(active);
  if (i < 0) return undefined;
  const own = graphOrder(active);
  if (active.orderAt + 1 < own.length) return { store: active, node: own[active.orderAt + 1], at: active.orderAt + 1 };
  for (const s of stores.slice(i + 1)) {
    const o = graphOrder(s);
    if (o.length) return { store: s, node: o[0], at: 0 };
  }
  return undefined;
}
