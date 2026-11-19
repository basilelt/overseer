import { app } from '../state.svelte';
import { beside } from '../review/beside.svelte';
import { closeSideReview, itemTarget, setReviewMode } from '../review/open';
import { allTabs, workspace } from '../workspace/workspace.svelte';
import { live, liveOn } from './live.svelte';
import { readItem, removeItem, writeItem, KEY_LIVE_REVIEW } from '../storage';

const STORE = KEY_LIVE_REVIEW;

// While the live review is on, the left bar shows as a rail of sessions; remember
// whether the sidebar was open so it comes back big when the review ends.
let sidebarBeforeLive: boolean | null = null;

function collapseSidebar() {
  sidebarBeforeLive = app.sidebarOpen;
  app.sidebarOpen = false;
}

function restoreSidebar() {
  app.sidebarOpen = sidebarBeforeLive ?? true;
  sidebarBeforeLive = null;
}

function remember(key: string | null) {
  if (key) writeItem(STORE, key);
  else removeItem(STORE);
}

export function restoreLiveReview(): boolean {
  const key = readItem(STORE);
  if (!key || !allTabs(workspace.layout).includes(key)) return false;
  beside.key = key;
  app.liveReview = true;
  collapseSidebar();
  return true;
}

export function toggleLiveReview(fromKey: string | null = app.selectedKey): boolean {
  if (liveOn()) {
    app.liveReview = false;
    live.worktree = null;
    closeSideReview();
    remember(null);
    restoreSidebar();
    return false;
  }
  const target = itemTarget(fromKey);
  if (!target) return false;
  live.worktree = null;
  setReviewMode(target, 'side');
  app.liveReview = true;
  remember(beside.key);
  collapseSidebar();
  return true;
}
