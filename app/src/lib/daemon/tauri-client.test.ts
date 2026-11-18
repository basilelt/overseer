import { afterEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({
  invoke,
  Channel: class {
    onmessage: ((m: unknown) => void) | null = null;
  },
}));

import { EVENT_BATCH_MS } from './event-batch';
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
    expect(invoke).toHaveBeenCalledWith('conn_disconnect', undefined);
    expect(invoke).toHaveBeenCalledWith('conn_wake', undefined);
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

  it('hands the events of a burst over together and a lone event at once', async () => {
    vi.useFakeTimers();
    try {
      invoke.mockResolvedValue(7);
      const d = new TauriDaemon();
      const batches: string[][] = [];
      const single: string[] = [];
      const off = d.subscribe(
        (e) => single.push(e.id),
        (events) => batches.push(events.map((e) => e.id)),
      );
      const channel = invoke.mock.calls.find((c) => c[0] === 'daemon_events')![1].onEvent as { onmessage: (m: unknown) => void };
      const ev = (id: string) => ({ id, type: 'line.updated', at: '', data: {} });
      channel.onmessage(ev('1'));
      for (const id of ['2', '3', '4']) channel.onmessage(ev(id));
      expect(batches).toEqual([['1']]);
      await vi.advanceTimersByTimeAsync(EVENT_BATCH_MS);
      expect(batches).toEqual([['1'], ['2', '3', '4']]);
      channel.onmessage(ev('5'));
      off();
      await vi.advanceTimersByTimeAsync(EVENT_BATCH_MS);
      channel.onmessage(ev('6'));
      expect(batches).toHaveLength(2);
      expect(single).toEqual([]);
      await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith('daemon_events_stop', { sub: 7 }));
    } finally {
      vi.useRealTimers();
    }
  });

  it('calls a subscriber without a batch handler once per event, in order', async () => {
    vi.useFakeTimers();
    try {
      invoke.mockResolvedValue(7);
      const seen: string[] = [];
      new TauriDaemon().subscribe((e) => seen.push(e.id));
      const channel = invoke.mock.calls.find((c) => c[0] === 'daemon_events')![1].onEvent as { onmessage: (m: unknown) => void };
      for (const id of ['a', 'b', 'c']) channel.onmessage({ id, type: 'line.updated', at: '', data: {} });
      expect(seen).toEqual(['a']);
      await vi.advanceTimersByTimeAsync(EVENT_BATCH_MS);
      expect(seen).toEqual(['a', 'b', 'c']);
    } finally {
      vi.useRealTimers();
    }
  });

  it('turns an error status into a DaemonError', async () => {
    invoke.mockResolvedValue({ status: 404, body: { error: 'gone' } });
    await expect(new TauriDaemon('h2').request('GET', '/v1/x')).rejects.toMatchObject({ status: 404, message: 'gone' });
  });
});
