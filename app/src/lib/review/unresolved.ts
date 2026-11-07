import type { Thread, ThreadStatus } from './types';

export const OPEN: ThreadStatus[] = ['pending', 'sent', 'addressed'];

export function unresolvedByWorktree(threads: Iterable<Pick<Thread, 'worktree' | 'status'>>): Record<string, number> {
  const out: Record<string, number> = {};
  for (const t of threads) if (OPEN.includes(t.status)) out[t.worktree] = (out[t.worktree] ?? 0) + 1;
  return out;
}
