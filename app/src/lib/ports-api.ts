import type { DaemonClient } from './client';

export function portsApi(c: Pick<DaemonClient, 'request'>) {
  return {
    killPort: (port: number, pid: number) => c.request('POST', `/v1/ports/${port}/kill`, { pid, confirm: true }),
  };
}
