import type { DaemonClient } from './client';
import type { Term } from './types';

export function feedbackApi(c: Pick<DaemonClient, 'request'>) {
  return {
    startFeedback: (body: object) => c.request<{ term: Term }>('POST', '/v1/feedback/start', body),
    directFeedback: (body: object) => c.request<{ url: string; attachWarning?: string }>('POST', '/v1/feedback/direct', body),
  };
}
