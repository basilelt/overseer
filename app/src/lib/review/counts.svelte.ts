import { app } from '../state.svelte';
import { reviewApi } from './api';
import { itemWorktree, reviewKey } from './keys';
import { unresolvedByWorktree } from './unresolved';
import type { ReviewTarget, Thread } from './types';

class ReviewCounts {
  threads = $state.raw<Map<string, Pick<Thread, 'worktree' | 'status'>>>(new Map());
  byWorktree = $derived(unresolvedByWorktree(this.threads.values()));
  any = $derived(Object.keys(this.byWorktree).length > 0);
  resolved = $state.raw<Record<string, string>>({});
  private client: unknown = null;
  private off: (() => void) | null = null;

  ensure() {
    if (!app.connected || !app.client || this.client === app.client) return;
    this.client = app.client;
    this.off?.();
    this.off = app.onChange((_p, _n, e) => {
      if (e.type === 'review.thread' && e.data?.thread?.id) this.put(e.data.thread);
      else if (e.type === 'review.thread_deleted' && e.data?.id) this.drop(e.data.id);
    });
    reviewApi(app.client)
      .threads()
      .then((ts) => (this.threads = new Map(ts.map((t) => [t.id, { worktree: t.worktree, status: t.status }]))))
      .catch(() => {
        if (this.client === app.client) this.client = null;
      });
  }

  put(t: Pick<Thread, 'id' | 'worktree' | 'status'>) {
    const next = new Map(this.threads);
    next.set(t.id, { worktree: t.worktree, status: t.status });
    this.threads = next;
  }

  drop(id: string) {
    if (!this.threads.has(id)) return;
    const next = new Map(this.threads);
    next.delete(id);
    this.threads = next;
  }

  alias(target: ReviewTarget, worktree: string) {
    const key = reviewKey(target);
    if (this.resolved[key] !== worktree) this.resolved = { ...this.resolved, [key]: worktree };
  }

  forKey(key: string): number {
    if (key.startsWith('review:br:') || !this.any) return 0;
    const wt = this.resolved[key] ?? itemWorktree(key, { lines: app.data.lines, tasks: app.data.tasks, terms: app.data.terms });
    return wt ? (this.byWorktree[wt] ?? 0) : 0;
  }
}

export const reviewCounts = new ReviewCounts();
