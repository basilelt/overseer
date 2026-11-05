import type { DaemonClient } from '../client';
import type { Bases, Branches, NewThread, Related, ReviewDiff, ReviewGraph, ReviewTarget, SubmitKind, SubmitResult, Thread } from './types';

const enc = encodeURIComponent;

export function targetQuery(t: ReviewTarget): string {
  if (t.branch && t.worktree) return `worktree=${enc(t.worktree)}&branch=${enc(t.branch)}`;
  if (t.worktree) return `worktree=${enc(t.worktree)}`;
  if (t.lineId) return `line=${enc(t.lineId)}`;
  if (t.taskId) return `task=${enc(t.taskId)}`;
  return '';
}

export function targetBody(t: ReviewTarget): ReviewTarget {
  if (t.worktree) return { worktree: t.worktree };
  if (t.lineId) return { lineId: t.lineId };
  if (t.taskId) return { taskId: t.taskId };
  return {};
}

export function reviewApi(c: DaemonClient) {
  return {
    diff: (t: ReviewTarget, base?: string) => c.request<ReviewDiff>('GET', `/v1/review?${targetQuery(t)}${base ? `&base=${enc(base)}` : ''}`),
    graph: (t: ReviewTarget, base?: string) => c.request<ReviewGraph>('GET', `/v1/review/graph?${targetQuery(t)}${base ? `&base=${enc(base)}` : ''}`),
    file: (worktree: string, path: string, rev?: string) => c.request<{ path: string; lines: string[] }>('GET', `/v1/review/file?worktree=${enc(worktree)}&path=${enc(path)}${rev ? `&rev=${enc(rev)}` : ''}`),
    related: (t: ReviewTarget) => c.request<Related>('GET', `/v1/review/worktrees?${targetQuery({ ...t, branch: undefined })}`),
    branches: (t: ReviewTarget) => c.request<Branches>('GET', `/v1/review/branches?${targetQuery({ ...t, branch: undefined })}`),
    bases: (t: ReviewTarget) => c.request<Bases>('GET', `/v1/review/bases?${targetQuery({ ...t, branch: undefined })}`),
    viewed: (worktree: string, path: string, hash: string, viewed: boolean) => c.request<{ path: string; hash: string; viewed: boolean }>('PUT', '/v1/review/viewed', { worktree, path, hash, viewed }),
    threads: (t?: ReviewTarget, status?: string) => {
      const q = [t ? targetQuery(t) : '', status ? `status=${enc(status)}` : ''].filter(Boolean).join('&');
      return c.request<Thread[]>('GET', `/v1/review/comments${q ? `?${q}` : ''}`);
    },
    createThread: (n: NewThread) => c.request<Thread>('POST', '/v1/review/comments', { ...targetBody(n), path: n.path, side: n.side ?? 'new', line: n.line, endLine: n.endLine ?? n.line, body: n.body, base: n.base || undefined }),
    reply: (threadId: string, body: string) => c.request<Thread>('POST', '/v1/review/comments', { threadId, body }),
    setStatus: (threadId: string, status: 'resolved' | 'open') => c.request<Thread>('PATCH', `/v1/review/comments/${enc(threadId)}`, { status }),
    editComment: (commentId: string, body: string) => c.request<Thread>('PATCH', `/v1/review/comments/${enc(commentId)}`, { body }),
    remove: (id: string) => c.request<{ deleted: string }>('DELETE', `/v1/review/comments/${enc(id)}`),
    apply: (threadId: string) => c.request<Thread>('POST', `/v1/review/comments/${enc(threadId)}/apply`),
    submit: (t: ReviewTarget, kind: SubmitKind, body?: string) => c.request<SubmitResult>('POST', '/v1/review/submit', { ...targetBody(t), kind, body: body?.trim() || undefined }),
  };
}

export type ReviewApi = ReturnType<typeof reviewApi>;
