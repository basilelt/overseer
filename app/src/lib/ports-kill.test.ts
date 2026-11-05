import { describe, expect, it, vi } from 'vitest';
import { MockDaemon } from './mock';
import { stopProcess } from './ports-kill';
import type { DaemonClient } from './client';

describe('stopProcess', () => {
  const row = { port: 5173, pid: 4242, process: 'node', detected: true, forwarded: false };
  const client = (request: DaemonClient['request']) => ({ request }) as DaemonClient;

  it('sends the port, pid and confirmation the daemon requires', async () => {
    const request = vi.fn().mockResolvedValue({});
    const notify = vi.fn();
    await stopProcess({ client: client(request), notify }, row);
    expect(request).toHaveBeenCalledWith('POST', '/v1/ports/5173/kill', { pid: 4242, confirm: true });
    expect(notify).not.toHaveBeenCalled();
  });

  it('stops a detected port on the mock daemon without an error', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0 });
    await m.connect({ kind: 'local', label: 'Local' }, () => {});
    const p = m.detected[0];
    const notify = vi.fn();
    await stopProcess({ client: m, notify }, { ...row, port: p.port, pid: p.pid });
    expect(notify).not.toHaveBeenCalled();
    expect(m.detected.some((x) => x.port === p.port)).toBe(false);
  });

  it('reports a daemon refusal as an error', async () => {
    const request = vi.fn().mockRejectedValue(new Error('process 4242 has already exited'));
    const notify = vi.fn();
    await stopProcess({ client: client(request), notify }, row);
    expect(notify).toHaveBeenCalledWith('process 4242 has already exited', 'error');
  });

  it('does nothing without a pid or a connection', async () => {
    const request = vi.fn();
    await stopProcess({ client: client(request), notify: vi.fn() }, { ...row, pid: undefined });
    await stopProcess({ client: null, notify: vi.fn() }, row);
    expect(request).not.toHaveBeenCalled();
  });
});
