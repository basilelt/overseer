import { describe, expect, it, vi } from 'vitest';
import type { DaemonClient } from '../client';
import { reviewApi, targetBody, targetQuery } from './api';

function client() {
  const request = vi.fn().mockResolvedValue({});
  return { request, api: reviewApi({ request } as unknown as DaemonClient) };
}

describe('targetQuery', () => {
  it('prefers a branch over its worktree, then worktree, line and task', () => {
    expect(targetQuery({ worktree: '/w/a b', branch: 'feat/x' })).toBe('worktree=%2Fw%2Fa%20b&branch=feat%2Fx');
    expect(targetQuery({ worktree: '/w/a', lineId: 'L', taskId: 'T' })).toBe('worktree=%2Fw%2Fa');
    expect(targetQuery({ lineId: 'L:1', taskId: 'T' })).toBe('line=L%3A1');
    expect(targetQuery({ taskId: 'T&1' })).toBe('task=T%261');
  });

  it('ignores a branch without a worktree and returns nothing for an empty target', () => {
    expect(targetQuery({ branch: 'feat/x', lineId: 'L' })).toBe('line=L');
    expect(targetQuery({})).toBe('');
  });
});

describe('targetBody', () => {
  it('keeps only the most specific field and never sends the branch', () => {
    expect(targetBody({ worktree: '/w', branch: 'b', lineId: 'L' })).toEqual({ worktree: '/w' });
    expect(targetBody({ lineId: 'L', taskId: 'T' })).toEqual({ lineId: 'L' });
    expect(targetBody({ taskId: 'T' })).toEqual({ taskId: 'T' });
    expect(targetBody({ branch: 'b' })).toEqual({});
  });
});

describe('reviewApi requests', () => {
  it('adds the base to diff and graph only when one is picked', () => {
    const { request, api } = client();
    api.diff({ lineId: 'L' });
    api.diff({ lineId: 'L' }, 'origin/main');
    api.graph({ taskId: 'T' }, 'a~1');
    api.graph({ taskId: 'T' });
    expect(request.mock.calls).toEqual([
      ['GET', '/v1/review?line=L'],
      ['GET', '/v1/review?line=L&base=origin%2Fmain'],
      ['GET', '/v1/review/graph?task=T&base=a~1'],
      ['GET', '/v1/review/graph?task=T'],
    ]);
  });

  it('reads a file at the working tree or at a revision', () => {
    const { request, api } = client();
    api.file('/w', 'src/a b.ts');
    api.file('/w', 'x', 'abc');
    expect(request.mock.calls).toEqual([
      ['GET', '/v1/review/file?worktree=%2Fw&path=src%2Fa%20b.ts'],
      ['GET', '/v1/review/file?worktree=%2Fw&path=x&rev=abc'],
    ]);
  });

  it('drops the branch when listing related worktrees, branches and bases', () => {
    const { request, api } = client();
    const t = { worktree: '/w', branch: 'feat/x' };
    api.related(t);
    api.branches(t);
    api.bases(t);
    expect(request.mock.calls.map((c) => c[1])).toEqual(['/v1/review/worktrees?worktree=%2Fw', '/v1/review/branches?worktree=%2Fw', '/v1/review/bases?worktree=%2Fw']);
    expect(t.branch).toBe('feat/x');
  });

  it('builds the comment listing query from an optional target and status', () => {
    const { request, api } = client();
    api.threads();
    api.threads(undefined, 'open');
    api.threads({ lineId: 'L' });
    api.threads({ lineId: 'L' }, 'resolved');
    api.threads({});
    expect(request.mock.calls.map((c) => c[1])).toEqual([
      '/v1/review/comments',
      '/v1/review/comments?status=open',
      '/v1/review/comments?line=L',
      '/v1/review/comments?line=L&status=resolved',
      '/v1/review/comments',
    ]);
  });

  it('creates a thread on the new side of a single line by default and omits an empty base', () => {
    const { request, api } = client();
    api.createThread({ lineId: 'L', branch: 'b', path: 'a.ts', line: 4, body: 'hm', base: '' });
    api.createThread({ worktree: '/w', path: 'a.ts', side: 'old', line: 2, endLine: 5, body: 'x', base: 'main' });
    expect(request.mock.calls[0]).toEqual(['POST', '/v1/review/comments', { lineId: 'L', path: 'a.ts', side: 'new', line: 4, endLine: 4, body: 'hm', base: undefined }]);
    expect(request.mock.calls[1][2]).toEqual({ worktree: '/w', path: 'a.ts', side: 'old', line: 2, endLine: 5, body: 'x', base: 'main' });
  });

  it('encodes ids in comment routes and sends the right verb for each action', () => {
    const { request, api } = client();
    api.reply('t/1', 'ok');
    api.setStatus('t/1', 'resolved');
    api.editComment('c 2', 'new');
    api.remove('c#3');
    api.apply('t/1');
    api.viewed('/w', 'a.ts', 'h', true);
    expect(request.mock.calls).toEqual([
      ['POST', '/v1/review/comments', { threadId: 't/1', body: 'ok' }],
      ['PATCH', '/v1/review/comments/t%2F1', { status: 'resolved' }],
      ['PATCH', '/v1/review/comments/c%202', { body: 'new' }],
      ['DELETE', '/v1/review/comments/c%233'],
      ['POST', '/v1/review/comments/t%2F1/apply'],
      ['PUT', '/v1/review/viewed', { worktree: '/w', path: 'a.ts', hash: 'h', viewed: true }],
    ]);
  });

  it('trims the overall submit message and leaves out a blank one', () => {
    const { request, api } = client();
    api.submit({ worktree: '/w', branch: 'b' }, 'approve', '  lgtm \n');
    api.submit({ taskId: 'T' }, 'comment', '   ');
    api.submit({ lineId: 'L' }, 'request_changes');
    expect(request.mock.calls.map((c) => c[2])).toEqual([
      { worktree: '/w', kind: 'approve', body: 'lgtm' },
      { taskId: 'T', kind: 'comment', body: undefined },
      { lineId: 'L', kind: 'request_changes', body: undefined },
    ]);
  });

  it('passes the daemon reply and errors through untouched', async () => {
    const request = vi.fn().mockResolvedValueOnce({ deleted: 'x' }).mockRejectedValueOnce(new Error('gone'));
    const api = reviewApi({ request } as unknown as DaemonClient);
    await expect(api.remove('x')).resolves.toEqual({ deleted: 'x' });
    await expect(api.remove('y')).rejects.toThrow('gone');
  });
});
