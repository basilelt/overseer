import { app } from '../state.svelte';
import { beside } from '../review/beside.svelte';
import { parseReviewKey } from '../review/keys';
import { closeSideReview, itemTarget, setReviewMode } from '../review/open';
import type { ScmTarget } from './types';

export const live = $state<{ worktree: string | null }>({ worktree: null });

export function liveOn(): boolean {
  return app.liveReview && !!beside.key && app.view === 'agents';
}

export function liveTarget(): ScmTarget | null {
  if (live.worktree) return { worktree: live.worktree };
  const t = beside.key ? parseReviewKey(beside.key) : null;
  if (t?.worktree) return { worktree: t.worktree };
  if (t?.lineId) return { lineId: t.lineId };
  return null;
}

export function sessionTarget(): ScmTarget | null {
  const t = beside.key ? parseReviewKey(beside.key) : null;
  if (t?.lineId) return { lineId: t.lineId };
  if (t?.worktree) return { worktree: t.worktree };
  return null;
}

export function toggleLiveReview(fromKey: string | null = app.selectedKey): boolean {
  if (liveOn()) {
    app.liveReview = false;
    live.worktree = null;
    closeSideReview();
    return false;
  }
  const target = itemTarget(fromKey);
  if (!target) return false;
  live.worktree = null;
  setReviewMode(target, 'side');
  app.liveReview = true;
  return true;
}
