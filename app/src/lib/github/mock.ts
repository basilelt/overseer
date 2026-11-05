import { DaemonError } from '../client';
import type { ReviewRequest, ReviewRequests } from './types';

const ago = (h: number) => new Date(Date.now() - h * 3_600_000).toISOString();

function pr(repo: string, number: number, title: string, author: string, h: number, more: Partial<ReviewRequest> = {}): ReviewRequest {
  return {
    repo,
    number,
    title,
    url: `https://github.com/${repo}/pull/${number}`,
    author,
    headRefName: `feat/${number}`,
    baseRefName: 'main',
    updatedAt: ago(h),
    isDraft: false,
    additions: 42,
    deletions: 7,
    changedFiles: 3,
    reviewDecision: 'REVIEW_REQUIRED',
    reviewed: false,
    myState: null,
    localPath: null,
    ...more,
  };
}

export function mockReviewRequests(): ReviewRequests {
  const mode = globalThis.location?.hash.match(/gh=(\w+)/)?.[1];
  if (mode === 'auth') throw new DaemonError(424, 'gh is not logged in on this host', { error: 'gh is not logged in on this host', code: 'auth' });
  if (mode === 'missing') throw new DaemonError(424, 'gh is not installed on this host', { error: 'gh is not installed on this host', code: 'missing' });
  const prs =
    mode === 'empty'
      ? []
      : [
          pr('acme/overseer', 412, 'Keep terminals alive across daemon upgrades', 'lea', 0.4, { additions: 318, deletions: 96, changedFiles: 9, localPath: '/home/me/src/overseer' }),
          pr('acme/overseer', 409, 'Debounce file watcher events on large repos', 'marc', 5, { additions: 24, deletions: 11, reviewed: true, myState: 'COMMENTED', localPath: '/home/me/src/overseer' }),
          pr('acme/api', 88, 'Add rate limiting to the public endpoints', 'sam', 26, { additions: 1240, deletions: 380, changedFiles: 31, reviewDecision: 'CHANGES_REQUESTED' }),
          pr('acme/api', 91, 'WIP: migrate auth to OIDC', 'nora', 50, { isDraft: true, additions: 6, deletions: 2, changedFiles: 1, reviewDecision: null }),
          pr('acme/site', 15, 'Fix dark mode contrast on the pricing page', 'li', 74, { additions: 8, deletions: 8, changedFiles: 2, reviewDecision: 'APPROVED' }),
        ];
  return { login: 'me', prs, fetchedAt: Date.now() };
}
