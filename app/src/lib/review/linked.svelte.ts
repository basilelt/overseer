import { ReviewStore } from './store.svelte';
import { batch, type Batch } from './threads';
import type { ReviewDiff, ReviewFile, SubmitKind, SubmitResult } from './types';

export class LinkedReviews {
  stores = $state.raw<ReviewStore[]>([]);

  sync(linked: ReviewDiff['linked']) {
    const want = (linked ?? []).map((l) => l.worktree);
    const keep = this.stores.filter((s) => want.includes(s.target.worktree ?? ''));
    for (const s of this.stores) if (!keep.includes(s)) s.stop();
    const added = want
      .filter((w) => !keep.some((s) => s.target.worktree === w))
      .map((w) => {
        const s = new ReviewStore({ worktree: w });
        s.start();
        return s;
      });
    if (added.length || keep.length !== this.stores.length) this.stores = [...keep, ...added];
  }

  get visible(): ReviewStore[] {
    return this.stores.filter((s) => !!s.diff && (s.diff.files.length > 0 || (s.diff.commits?.length ?? 0) > 0));
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
