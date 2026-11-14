import { describe, expect, it } from 'vitest';
import type { DaemonClient } from '../daemon/client';
import { githubApi } from './api';

const capture = () => {
  const calls: unknown[][] = [];
  const c = { request: async (...a: unknown[]) => void calls.push(a) } as unknown as DaemonClient;
  return { calls, a: githubApi(c) };
};

describe('githubApi', () => {
  it('lists review requests, refreshing on demand', async () => {
    const { calls, a } = capture();
    await a.reviewRequests();
    await a.reviewRequests(true);
    expect(calls).toEqual([['GET', '/v1/github/review-requests'], ['GET', '/v1/github/review-requests?refresh=1']]);
  });

  it('starts a review with the owner and name encoded', async () => {
    const { calls, a } = capture();
    await a.startReview('o w', 'r/x', 7, { agent: 'claude' });
    expect(calls).toEqual([['POST', '/v1/github/review-requests/o%20w/r%2Fx/7/review', { agent: 'claude' }]]);
  });
});
