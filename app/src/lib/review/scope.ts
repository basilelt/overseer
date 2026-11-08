import { t } from '../i18n.svelte';
import { reviewKey } from './keys';
import type { Related, RelatedWorktree } from './types';

export interface ScopeOption {
  key: string;
  name: string;
  path: string;
  worktree?: RelatedWorktree;
}

export function folder(path: string): string {
  return path.split('/').filter(Boolean).pop() ?? path;
}

export function changed(w: RelatedWorktree): boolean {
  return w.files > 0 || w.ahead > 0;
}

export function changedWorktrees(r: Related | null): RelatedWorktree[] {
  return r ? r.worktrees.filter(changed) : [];
}

export function changedLabel(list: RelatedWorktree[]): string {
  if (!list.some((w) => w.current)) return t('review.otherWorktrees');
  return t(list.length === 1 ? 'review.worktreesChangedOne' : 'review.worktreesChanged', { n: list.length });
}

export function scopeOptions(related: Related | null, selfKey?: string, open: ScopeOption[] = []): ScopeOption[] {
  const self = selfKey && !selfKey.startsWith('review:br:') ? selfKey : null;
  const out: ScopeOption[] = (related?.worktrees ?? []).map((w) => ({ key: w.current && self ? self : reviewKey({ worktree: w.path }), name: w.branch || folder(w.path), path: w.path, worktree: w }));
  for (const o of open) if (!out.some((x) => x.key === o.key || (!!o.path && x.path === o.path && !o.key.startsWith('review:br:')))) out.push(o);
  return out;
}

export function includeCurrent(opts: ScopeOption[], key: string, name: string, path: string): ScopeOption[] {
  if (!key || opts.some((o) => o.key === key)) return opts;
  return [{ key, name, path }, ...opts];
}

export function counts(w: { files: number; ahead: number; agents?: number }): string {
  const parts: string[] = [];
  if (w.files) parts.push(w.files === 1 ? t('review.optFile') : t('review.optFiles', { n: w.files }));
  if (w.ahead) parts.push(w.ahead === 1 ? t('review.optCommit') : t('review.optCommits', { n: w.ahead }));
  if (!parts.length) parts.push(t('review.optClean'));
  if (w.agents) parts.push(w.agents === 1 ? t('review.optAgent') : t('review.optAgents', { n: w.agents }));
  return parts.join(' · ');
}
