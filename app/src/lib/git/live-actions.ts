import { app } from '../state.svelte';
import { beside } from '../review/beside.svelte';
import { closeSideReview, itemTarget, setReviewMode } from '../review/open';
import { allTabs, workspace } from '../workspace.svelte';
import { live, liveOn } from './live.svelte';

const STORE = 'overseer.liveReview';

function remember(key: string | null) {
  try {
    if (key) localStorage.setItem(STORE, key);
    else localStorage.removeItem(STORE);
  } catch {}
}

export function restoreLiveReview(): boolean {
  let key: string | null = null;
  try {
    key = localStorage.getItem(STORE);
  } catch {}
  if (!key || !allTabs(workspace.layout).includes(key)) return false;
  beside.key = key;
  app.liveReview = true;
  return true;
}

export function toggleLiveReview(fromKey: string | null = app.selectedKey): boolean {
  if (liveOn()) {
    app.liveReview = false;
    live.worktree = null;
    closeSideReview();
    remember(null);
    return false;
  }
  const target = itemTarget(fromKey);
  if (!target) return false;
  live.worktree = null;
  setReviewMode(target, 'side');
  app.liveReview = true;
  remember(beside.key);
  return true;
}
