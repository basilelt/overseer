import type { DaemonClient } from '../client';
import type { ReviewRequests, StartedReview } from './types';

const enc = encodeURIComponent;

export function githubApi(c: Pick<DaemonClient, 'request'>) {
  return {
    reviewRequests: (refresh = false) => c.request<ReviewRequests>('GET', `/v1/github/review-requests${refresh ? '?refresh=1' : ''}`),
    startReview: (owner: string, name: string, number: number, body: object) =>
      c.request<StartedReview | { pending: true; title: string }>('POST', `/v1/github/review-requests/${enc(owner)}/${enc(name)}/${number}/review`, body),
  };
}
