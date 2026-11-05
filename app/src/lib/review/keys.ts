import type { ReviewTarget } from './types';

export const REVIEW_PREFIX = 'review:';

export function reviewKey(t: ReviewTarget): string {
  if (t.branch && t.worktree) return `${REVIEW_PREFIX}br:${t.branch}:${t.worktree}`;
  if (t.worktree) return `${REVIEW_PREFIX}wt:${t.worktree}`;
  if (t.lineId) return `${REVIEW_PREFIX}line:${t.lineId}`;
  if (t.taskId) return `${REVIEW_PREFIX}task:${t.taskId}`;
  throw new Error('empty review target');
}

export function isReviewKey(key: string | null | undefined): key is string {
  return !!key && key.startsWith(REVIEW_PREFIX);
}

export function parseReviewKey(key: string): ReviewTarget | null {
  if (!isReviewKey(key)) return null;
  const rest = key.slice(REVIEW_PREFIX.length);
  const i = rest.indexOf(':');
  const kind = i < 0 ? '' : rest.slice(0, i);
  const v = rest.slice(i + 1);
  if (kind === 'br') {
    const j = v.indexOf(':');
    return j > 0 && j < v.length - 1 ? { worktree: v.slice(j + 1), branch: v.slice(0, j) } : null;
  }
  if (kind === 'wt') return v ? { worktree: v } : null;
  if (kind === 'line') return v ? { lineId: v } : null;
  if (kind === 'task') return v ? { taskId: v } : null;
  return rest ? { lineId: rest } : null;
}

export interface Known {
  lines: { line: { id: string; worktreePath?: string; branch?: string }; local?: { title?: string; titleOverride?: string | null } }[];
  tasks: { id: string; number: number; title: string; worktreePath?: string | null; lineId?: string | null; branch?: string | null }[];
  terms?: { id: string; cwd: string; lineId?: string | null }[];
}

export function worktreeOf(t: ReviewTarget, k: Known): string | null {
  if (t.worktree) return t.worktree;
  if (t.lineId) return k.lines.find((l) => l.line.id === t.lineId)?.line.worktreePath ?? null;
  if (t.taskId) {
    const task = k.tasks.find((x) => x.id === t.taskId);
    if (task?.worktreePath) return task.worktreePath;
    return task?.lineId ? (k.lines.find((l) => l.line.id === task.lineId)?.line.worktreePath ?? null) : null;
  }
  return null;
}

export function titleOf(t: ReviewTarget, k: Known): string {
  if (t.branch) return t.branch;
  if (t.taskId) {
    const task = k.tasks.find((x) => x.id === t.taskId);
    if (task) return `#${task.number} ${task.title}`;
  }
  if (t.lineId) {
    const l = k.lines.find((x) => x.line.id === t.lineId);
    const task = k.tasks.find((x) => x.lineId === t.lineId);
    if (task) return `#${task.number} ${task.title}`;
    if (l) return l.local?.titleOverride || l.local?.title || l.line.branch || l.line.worktreePath?.split('/').pop() || l.line.id;
  }
  const wt = worktreeOf(t, k);
  return wt?.split('/').filter(Boolean).pop() ?? t.lineId ?? t.taskId ?? '';
}

export function itemWorktree(key: string, k: Known): string | null {
  const target = parseReviewKey(key);
  if (target) return worktreeOf(target, k);
  if (key.startsWith('line:')) return worktreeOf({ lineId: key.slice(5) }, k);
  if (key.startsWith('task:')) return worktreeOf({ taskId: key.slice(5) }, k);
  if (key.startsWith('term:')) {
    const term = k.terms?.find((x) => x.id === key.slice(5));
    if (!term) return null;
    return (term.lineId && worktreeOf({ lineId: term.lineId }, k)) || term.cwd;
  }
  return null;
}
