import type { DaemonClient } from '../daemon/client';
import type { UsageBy, UsageRange, UsageResponse } from './types';

export function usageApi(c: Pick<DaemonClient, 'request'>) {
  return {
    usage: (range: UsageRange, by: UsageBy, tz: number, line?: string, claudeLimits = false) => {
      const q = new URLSearchParams({ range, by, tzOffset: String(tz) });
      if (line) q.set('line', line);
      if (claudeLimits) q.set('claudeLimits', '1');
      return c.request<UsageResponse>('GET', `/v1/usage?${q}`);
    },
  };
}
