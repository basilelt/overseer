import { app } from '../state.svelte';
import { isMockClient } from '../daemon/client';
import { allTabs, openInFocused, update, workspace } from '../workspace/workspace.svelte';
import { isPhone } from '../mobile/ui.svelte';
import { beside } from './beside.svelte';
import { closeReviews, follow, openBeside, reviewMode, type ReviewMode } from './pane';
import { isReviewKey, parseReviewKey, reviewKey, titleOf, worktreeOf } from './keys';
import type { ScopeOption } from './scope';
import type { ReviewTarget } from './types';
import { isTesting } from '../kanban/task-status';

export { isReviewKey, parseReviewKey, reviewKey };

export function reviewTitle(key: string): string | null {
  const t = parseReviewKey(key);
  return t ? titleOf(t, { lines: app.data.lines, tasks: app.data.tasks, terms: app.data.terms }) : null;
}

export function openReviewTabs(): ScopeOption[] {
  const known = { lines: app.data.lines, tasks: app.data.tasks, terms: app.data.terms };
  return allTabs(workspace.layout).flatMap((key) => {
    const t = parseReviewKey(key);
    return t ? [{ key, name: titleOf(t, known), path: worktreeOf(t, known) ?? '' }] : [];
  });
}

export function testingReviews(tasks = app.data.tasks, lines = app.data.lines, tabs = allTabs(workspace.layout)) {
  const open = new Set(tabs);
  return tasks.filter(
    (x) => isTesting(x.status) && (x.lineId || x.worktreePath) && (open.has(reviewKey({ taskId: x.id })) || lines.some((l) => l.line.id === x.lineId && l.local?.pid)),
  );
}

export function openReview(target: ReviewTarget): string {
  const key = reviewKey(target);
  app.view = 'agents';
  app.open(null);
  update((l) => openInFocused(l, key));
  app.selectedKey = key;
  return key;
}

export function itemTarget(key: string | null | undefined): ReviewTarget | null {
  if (!key || !(key.startsWith('line:') || key.startsWith('term:'))) return null;
  const item = app.items.find((i) => i.key === key);
  if (item?.lineId) return { lineId: item.lineId };
  const term = item?.termId ? app.data.terms.find((x) => x.id === item.termId) : undefined;
  if (term?.lineId) return { lineId: term.lineId };
  return term?.cwd ? { worktree: term.cwd } : null;
}

export function isSideReview(key: string | null | undefined): boolean {
  return !!key && beside.key === key && allTabs(workspace.layout).includes(key);
}

export function openReviewBeside(target: ReviewTarget, from = workspace.layout.focused, pin = false): string {
  if (isPhone()) return openReview(target);
  const key = reviewKey(target);
  app.view = 'agents';
  const side = allTabs(workspace.layout).includes(beside.key ?? '') ? beside.key : null;
  workspace.layout = openBeside(workspace.layout, from, key, side);
  beside.key = key;
  beside.pinned = pin;
  return key;
}

export function openReviewNear(target: ReviewTarget, fromKey: string) {
  if (isSideReview(fromKey)) openReviewBeside(target, workspace.layout.panes.findIndex((p) => !p.tabs.includes(fromKey)), true);
  else openReview(target);
}

export function modeFor(itemKey: string | null | undefined): ReviewMode {
  const t = itemTarget(itemKey);
  return reviewMode(workspace.layout, beside.key, t ? reviewKey(t) : null);
}

export function setReviewMode(target: ReviewTarget, mode: ReviewMode, from = workspace.layout.focused) {
  const key = reviewKey(target);
  if (mode !== 'side') {
    closeSideReview();
    if (mode === 'off') workspace.layout = closeReviews(workspace.layout, [key]);
    else openReview(target);
    return;
  }
  workspace.layout = closeReviews(workspace.layout, [key]);
  openReviewBeside(target, Math.min(from, workspace.layout.panes.length - 1));
}

export function closeSideReview() {
  workspace.layout = closeReviews(workspace.layout, [beside.key]);
  beside.key = null;
  beside.pinned = false;
}

export function followReview(selected: string | null) {
  if (!beside.key || beside.pinned) return;
  const t = itemTarget(selected);
  if (!t) return;
  const want = reviewKey(t);
  const next = follow(workspace.layout, beside.key, false, want);
  if (next === workspace.layout) return;
  workspace.layout = next;
  beside.key = want;
}

export function reviewTargetFor(item: { lineId?: string; taskId?: string } | null, tasks = app.data.tasks): ReviewTarget | null {
  if (item?.lineId) return { lineId: item.lineId };
  if (item?.taskId) return { taskId: item.taskId };
  const testing = tasks.find((t) => isTesting(t.status) && (t.lineId || t.worktreePath));
  return testing ? { taskId: testing.id } : null;
}

export function openReviewFromHash() {
  const hash = globalThis.location?.hash ?? '';
  if (!isMockClient(app.client) || !hash.startsWith('#review')) return;
  const target = hash.startsWith('#review=') ? parseReviewKey(decodeURIComponent(hash.slice(8))) : hash === '#review' ? reviewTargetFor(null) : null;
  if (hash === '#review-beside') {
    app.panelOpen = false;
    setTimeout(() => {
      app.select(app.items.find((i) => i.lineId === 'ln_03')?.key ?? 'line:ln_03');
      setTimeout(() => openReviewBeside({ lineId: 'ln_03' }), 50);
    }, 50);
    return;
  }
  if (!target) return;
  app.panelOpen = false;
  setTimeout(() => openReview(target), 50);
}
