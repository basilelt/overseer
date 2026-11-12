import { afterEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    onmessage: ((m: unknown) => void) | null = null;
  },
}));

import { TauriDaemon } from './tauri-client';

afterEach(() => invoke.mockReset());

describe('TauriDaemon slots', () => {
  it('sends no slot for the window connection, as before', async () => {
    invoke.mockResolvedValue({ status: 200, body: [] });
    const d = new TauriDaemon();
    await d.request('GET', '/v1/lines');
    await d.disconnect();
    await d.wake();
    expect(invoke).toHaveBeenCalledWith('daemon_request', { method: 'GET', path: '/v1/lines', body: null });
    expect(invoke).toHaveBeenCalledWith('conn_disconnect');
    expect(invoke).toHaveBeenCalledWith('conn_wake');
  });

  it('routes every connection call of an extra host through its slot', async () => {
    invoke.mockImplementation(async (cmd: string) => (cmd === 'daemon_request' ? { status: 200, body: [] } : cmd === 'term_attach' || cmd === 'daemon_events' || cmd === 'ports_watch' ? 7 : undefined));
    const d = new TauriDaemon('h2');
    await d.request('GET', '/v1/lines');
    await d.connect({ kind: 'ssh', host: 'prod', label: 'prod' }, () => {});
    const io = await d.attachTerm('t1', () => {});
    io.write('x');
    io.resize(80, 24);
    io.close();
    const off = d.subscribe(() => {});
    off();
    const unwatch = d.watchPorts(() => {});
    unwatch();
    await d.forwardPort(3000);
    await d.unforwardPort(3000);
    await d.configurePorts({ autoForward: true, ignore: '', localhostOnly: true } as never);
    await d.answerPrompt(1, 'yes');
    await d.disconnect();
    await d.wake();
    await d.restartServer();
    await d.sshHosts();
    await d.dropSlots();
    await Promise.resolve();
    expect(invoke).toHaveBeenCalledWith('conn_drop_slots');
    const scoped = invoke.mock.calls.filter(([cmd]) => cmd !== 'ssh_hosts' && cmd !== 'conn_drop_slots');
    expect(scoped.length).toBeGreaterThan(10);
    for (const [cmd, args] of scoped) expect([cmd, (args as { slot?: string })?.slot]).toEqual([cmd, 'h2']);
    expect(invoke.mock.calls.find(([cmd]) => cmd === 'ssh_hosts')).toEqual(['ssh_hosts']);
  });

  it('turns an error status into a DaemonError', async () => {
    invoke.mockResolvedValue({ status: 404, body: { error: 'gone' } });
    await expect(new TauriDaemon('h2').request('GET', '/v1/x')).rejects.toMatchObject({ status: 404, message: 'gone' });
  });
});
