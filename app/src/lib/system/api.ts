import type { DaemonClient } from '../daemon/client';
import type { SysResponse } from './types';

export function systemApi(c: Pick<DaemonClient, 'request'>) {
  return {
    system: (since: number | null) => c.request<SysResponse>('GET', since == null ? '/v1/system' : `/v1/system?since=${since}`),
  };
}
