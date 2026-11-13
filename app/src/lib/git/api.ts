import type { DaemonClient } from '../daemon/client';
import type { CleanupPlan, CommitDetail, CommitOptions, GraphPage, NewWorktree, RefsFilter, ScmResult, ScmStatus, ScmTarget, Worktree, WorktreeList } from './types';

const enc = encodeURIComponent;

export function scmQuery(t: ScmTarget): string {
  if (t.worktree) return `worktree=${enc(t.worktree)}`;
  if (t.lineId) return `line=${enc(t.lineId)}`;
  return '';
}

export function scmBody(t: ScmTarget): ScmTarget {
  if (t.worktree) return { worktree: t.worktree };
  if (t.lineId) return { lineId: t.lineId };
  return {};
}

export interface GraphQuery {
  limit?: number;
  before?: string;
  refs?: RefsFilter | string;
}

export function gitApi(c: DaemonClient) {
  return {
    graph: (project: string, q: GraphQuery = {}) => {
      const params = new URLSearchParams();
      if (q.limit) params.set('limit', String(q.limit));
      if (q.before) params.set('before', q.before);
      if (q.refs && q.refs !== 'all') params.set('refs', q.refs);
      const qs = params.toString();
      return c.request<GraphPage>('GET', `/v1/projects/${enc(project)}/graph${qs ? `?${qs}` : ''}`);
    },
    commit: (project: string, sha: string) => c.request<CommitDetail>('GET', `/v1/projects/${enc(project)}/commits/${enc(sha)}`),
    createBranch: (project: string, name: string, sha: string) => c.request<{ name: string; sha: string }>('POST', `/v1/projects/${enc(project)}/branches`, { name, sha }),
    worktrees: (project: string) => c.request<WorktreeList>('GET', `/v1/projects/${enc(project)}/worktrees`),
    createWorktree: (w: NewWorktree) => c.request<Worktree>('POST', '/v1/worktrees', w),
    cleanup: (id: string, dryRun: boolean, project?: string) => c.request<CleanupPlan>('POST', `/v1/worktrees/${enc(id)}/cleanup`, { dryRun, project }),
    mergeBase: (id: string) => c.request<Worktree>('POST', `/v1/worktrees/${enc(id)}/merge-base`, {}),
    cherryPick: (id: string, sha: string) => c.request<{ commit: string; worktree: Worktree }>('POST', `/v1/worktrees/${enc(id)}/cherry-pick`, { sha }),
    status: (t: ScmTarget) => c.request<ScmStatus>('GET', `/v1/git/status?${scmQuery(t)}`),
    stage: (t: ScmTarget, files: string[] | 'all') => c.request<ScmResult>('POST', '/v1/git/stage', { ...scmBody(t), files }),
    unstage: (t: ScmTarget, files: string[] | 'all') => c.request<ScmResult>('POST', '/v1/git/unstage', { ...scmBody(t), files }),
    discard: (t: ScmTarget, files: string[] | 'all') => c.request<ScmResult>('POST', '/v1/git/discard', { ...scmBody(t), files }),
    commitChanges: (t: ScmTarget, o: CommitOptions) => c.request<ScmResult>('POST', '/v1/git/commit', { ...scmBody(t), ...o }),
    fetch: (t: ScmTarget) => c.request<ScmResult>('POST', '/v1/git/fetch', scmBody(t)),
    pull: (t: ScmTarget) => c.request<ScmResult>('POST', '/v1/git/pull', scmBody(t)),
    push: (t: ScmTarget) => c.request<ScmResult>('POST', '/v1/git/push', scmBody(t)),
    switchBranch: (t: ScmTarget, branch: string, create = false) => c.request<ScmResult>('POST', '/v1/git/switch', { ...scmBody(t), branch, create }),
    merge: (t: ScmTarget, rev: string) => c.request<ScmResult>('POST', '/v1/git/merge', { ...scmBody(t), rev }),
    stash: (t: ScmTarget, pop = false) => c.request<ScmResult>('POST', '/v1/git/stash', { ...scmBody(t), pop }),
    revert: (t: ScmTarget, sha: string) => c.request<ScmResult>('POST', '/v1/git/revert', { ...scmBody(t), sha }),
  };
}

export type GitApi = ReturnType<typeof gitApi>;
