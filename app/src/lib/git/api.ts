import type { DaemonClient } from '../client';
import type { CleanupPlan, CommitDetail, GraphPage, NewWorktree, RefsFilter, Worktree, WorktreeList } from './types';

const enc = encodeURIComponent;

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
  };
}

export type GitApi = ReturnType<typeof gitApi>;
