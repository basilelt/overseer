import { describe, expect, it, vi } from 'vitest';
import { MockDaemon } from '../daemon/mock';
import { closedForwards, forwardNotices, parseIgnore, parsePort, portChip, portCount, portRows, portSettings } from './ports';
import { announce } from './ports.svelte';
import { addEntry, type Entry, type EntryInput } from '../notify/notifications';
import type { ConnState, Forward, Port, PortsState } from '../types';

const port = (n: number, extra: Partial<Port> = {}): Port => ({
  port: n,
  addr: '127.0.0.1',
  pid: 100 + n,
  process: 'node',
  termId: 't1',
  project: '/src/web',
  since: '2026-10-29T09:00:00Z',
  ...extra,
});
const fwd = (remotePort: number, extra: Partial<Forward> = {}): Forward => ({
  remotePort,
  localPort: remotePort,
  localAddr: '127.0.0.1',
  url: `http://localhost:${remotePort}`,
  source: 'auto',
  ...extra,
});

describe('portRows', () => {
  it('gives local ports a direct localhost url', () => {
    const rows = portRows({ remote: false, detected: [port(8000), port(5173)], forwards: [] });
    expect(rows.map((r) => [r.port, r.url, r.forwarded])).toEqual([
      [5173, 'http://localhost:5173', false],
      [8000, 'http://localhost:8000', false],
    ]);
  });

  it('merges remote detections with forwards, keeping manual forwards of undetected ports', () => {
    const s: PortsState = {
      remote: true,
      detected: [port(5173), port(8000, { process: 'python3' })],
      forwards: [fwd(5173, { localPort: 5174, url: 'http://localhost:5174' }), fwd(9229, { source: 'manual', process: 'node' }), fwd(8000, { error: 'bind: address already in use' })],
    };
    const rows = portRows(s);
    expect(rows.map((r) => r.port)).toEqual([5173, 8000, 9229]);
    expect(rows[0]).toMatchObject({ local: '127.0.0.1:5174', url: 'http://localhost:5174', forwarded: true, detected: true, source: 'auto', process: 'node', pid: 5273 });
    expect(rows[1]).toMatchObject({ url: undefined, forwarded: false, error: 'bind: address already in use', process: 'python3' });
    expect(rows[2]).toMatchObject({ detected: false, forwarded: true, source: 'manual', process: 'node' });
  });

  it('leaves unforwarded remote ports without url', () => {
    const rows = portRows({ remote: true, detected: [port(3000)], forwards: [] });
    expect(rows[0]).toMatchObject({ url: undefined, local: undefined, forwarded: false });
  });

  it('counts forwards remotely and detections locally', () => {
    expect(portCount({ remote: false, detected: [port(1), port(2)], forwards: [] })).toBe(2);
    expect(portCount({ remote: true, detected: [port(1), port(2)], forwards: [fwd(1), fwd(2, { error: 'x' })] })).toBe(1);
  });

  it('keeps the chip visible with detected ports when auto-forward is off', () => {
    expect(portChip({ remote: true, detected: [port(3000), port(5173)], forwards: [] })).toEqual({ n: 2, forwarded: false });
  });

  it('shows the forwarded count on the chip once something is forwarded', () => {
    expect(portChip({ remote: true, detected: [port(1), port(2)], forwards: [fwd(1)] })).toEqual({ n: 1, forwarded: true });
  });

  it('hides the chip when nothing is detected or forwarded', () => {
    expect(portChip({ remote: true, detected: [], forwards: [fwd(1, { error: 'x' })] })).toEqual({ n: 0, forwarded: false });
    expect(portChip({ remote: false, detected: [port(1)], forwards: [] })).toEqual({ n: 1, forwarded: false });
  });
});

describe('parseIgnore', () => {
  it('parses lists and ranges', () => {
    expect(parseIgnore('22, 5432 9000-9003')).toEqual({ ports: [22, 5432, 9000, 9001, 9002, 9003], invalid: [] });
    expect(parseIgnore(' 80,80;443 ')).toEqual({ ports: [80, 443], invalid: [] });
    expect(parseIgnore('')).toEqual({ ports: [], invalid: [] });
  });

  it('reports invalid entries', () => {
    expect(parseIgnore('22 abc 0 70000 10-5 1-5000').invalid).toEqual(['abc', '0', '70000', '10-5', '1-5000']);
    expect(parseIgnore('22 abc').ports).toEqual([22]);
  });

  it('builds daemon settings from app settings', () => {
    expect(portSettings({ portsAutoForward: false, portsIgnore: '22 3000-3001', portsLocalhostOnly: true })).toEqual({ autoForward: false, ignore: [22, 3000, 3001], localhostOnly: true });
  });
});

describe('parsePort', () => {
  it('accepts empty as auto and rejects junk', () => {
    expect(parsePort(' ')).toBeNull();
    expect(parsePort('5173')).toBe(5173);
    expect(parsePort('0')).toBeUndefined();
    expect(parsePort('80a')).toBeUndefined();
    expect(parsePort('65536')).toBeUndefined();
  });
});

describe('forwardNotices', () => {
  it('notifies successful auto-forwards only', () => {
    expect(forwardNotices({ remote: true, detected: [], forwards: [] })).toEqual([]);
    const ok = fwd(5173);
    expect(forwardNotices({ remote: true, detected: [], forwards: [ok], added: [ok, fwd(8000, { error: 'in use' })] })).toEqual([ok]);
  });
});

describe('closedForwards', () => {
  it('lists forwards that disappeared on the same remote', () => {
    const a = fwd(3000);
    const b = fwd(5173);
    expect(closedForwards({ remote: true, detected: [], forwards: [a, b] }, { remote: true, detected: [], forwards: [b] })).toEqual([a]);
    expect(closedForwards({ remote: true, detected: [], forwards: [a] }, { remote: false, detected: [], forwards: [] })).toEqual([]);
    expect(closedForwards({ remote: true, detected: [], forwards: [fwd(8000, { error: 'in use' })] }, { remote: true, detected: [], forwards: [] })).toEqual([]);
  });
});

describe('announce', () => {
  it('records quiet, coalesced center entries instead of toasting a restarting dev server', () => {
    let list: Entry[] = [];
    const app = { notice: vi.fn((e: EntryInput, _toast?: { run: () => void }) => void (list = addEntry(list, e, 1) ?? list)), openUrl: vi.fn() };
    for (let i = 0; i < 4; i++) {
      announce(app, 'ssh:dev', [fwd(3000)], []);
      announce(app, 'ssh:dev', [], [fwd(3000)]);
    }
    announce(app, 'ssh:other', [fwd(3000)], []);
    expect(list).toHaveLength(2);
    expect(list.every((e) => e.read)).toBe(true);
    expect(list.find((e) => e.id === 'port:ssh:dev:3000')).toMatchObject({ kind: 'portClosed', read: true });
    expect(app.notice.mock.calls[0][0]).toMatchObject({ kind: 'portForwarded', quiet: true, action: { kind: 'url', url: 'http://localhost:3000' } });
    app.notice.mock.calls[0][1]!.run();
    expect(app.openUrl).toHaveBeenCalledWith('http://localhost:3000', { termId: undefined });
  });

  it('opens a forwarded port beside the session whose process listens on it', () => {
    const app = { notice: vi.fn(), openUrl: vi.fn() };
    announce(app, 'ssh:dev', [fwd(5173, { termId: 't1' })], []);
    (app.notice.mock.calls[0] as unknown as [EntryInput, { run: () => void }])[1].run();
    expect(app.openUrl).toHaveBeenCalledWith('http://localhost:5173', { termId: 't1' });
  });
});

describe('MockDaemon ports and prompts', () => {
  const ssh = (host: string) => ({ kind: 'ssh' as const, host, label: host });

  it('auto-forwards detected ports on a remote host and reports them as added', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0 });
    await m.connect(ssh('devbox'), () => {});
    const states: PortsState[] = [];
    const stop = m.watchPorts((s) => states.push(s));
    await m.configurePorts({ autoForward: true, ignore: [8000], localhostOnly: true });
    await new Promise((r) => setTimeout(r, 5));
    stop();
    expect(states[0].remote).toBe(true);
    expect(states.flatMap((s) => s.added ?? []).map((f) => f.remotePort)).toEqual([5173]);
    expect(states.at(-1)!.forwards.map((f) => f.remotePort)).toEqual([5173]);
  });

  it('forwards manually, rejects duplicates and does not re-forward a stopped port', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0 });
    await m.connect(ssh('devbox'), () => {});
    const f = await m.forwardPort(8000, 18000);
    expect(f).toMatchObject({ remotePort: 8000, localPort: 18000, source: 'manual', url: 'http://localhost:18000' });
    await expect(m.forwardPort(8000)).rejects.toThrow(/already forwarded/);
    await m.configurePorts({ autoForward: true, ignore: [], localhostOnly: true });
    await m.unforwardPort(5173);
    await m.configurePorts({ autoForward: true, ignore: [], localhostOnly: false });
    expect(m.forwards.map((x) => x.remotePort)).toEqual([8000]);
  });

  it('stops only the pid listed for a port, like the daemon', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0 });
    await m.connect({ kind: 'local', label: 'Local' }, () => {});
    const states: PortsState[] = [];
    const stop = m.watchPorts((s) => states.push(s));
    const p = m.detected.find((x) => x.port === 5173)!;
    await expect(m.request('POST', '/v1/ports/abc/kill', { pid: p.pid, confirm: true })).rejects.toMatchObject({ status: 400 });
    await expect(m.request('POST', '/v1/ports/70000/kill', { pid: p.pid, confirm: true })).rejects.toMatchObject({ status: 400 });
    await expect(m.request('POST', '/v1/ports/5173/kill', { confirm: true })).rejects.toThrow('pid (number) required');
    await expect(m.request('POST', '/v1/ports/5173/kill', { pid: p.pid })).rejects.toThrow('kill needs {"confirm":true}');
    await expect(m.request('POST', '/v1/ports/5173/kill', { pid: p.pid + 1, confirm: true })).rejects.toMatchObject({ status: 404, message: expect.stringContaining('in an Overseer terminal') });
    expect(await m.request('POST', '/v1/ports/5173/kill', { pid: p.pid, confirm: true })).toMatchObject({ port: 5173, pid: p.pid });
    stop();
    expect(states.at(-1)!.detected.map((x) => x.port)).toEqual([8000]);
    await expect(m.request('POST', '/v1/ports/5173/kill', { pid: p.pid, confirm: true })).rejects.toMatchObject({ status: 404 });
  });

  it('keeps local mode without forwards', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0 });
    await m.connect({ kind: 'local', label: 'Local' }, () => {});
    const states: PortsState[] = [];
    m.watchPorts((s) => states.push(s))();
    expect(states[0]).toMatchObject({ remote: false, forwards: [] });
    expect(states[0].detected.map((p) => p.port)).toEqual([5173, 8000]);
    await expect(m.forwardPort(5173)).rejects.toThrow();
  });

  it('suspends on a host key prompt until answered', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0 });
    const seen: ConnState[] = [];
    const done = m.connect(ssh('hostkey-box'), (s) => seen.push(s));
    await new Promise((r) => setTimeout(r, 0));
    const prompt = seen.at(-1)!.prompt!;
    expect(prompt).toMatchObject({ kind: 'hostkey', fingerprint: expect.stringMatching(/^SHA256:/) });
    expect(seen.at(-1)!.resolved).toContain('hostkey-box');
    await m.answerPrompt(prompt.id, 'yes');
    await done;
    expect(seen.at(-1)!.phase).toBe('connected');
    expect(seen.at(-1)!.prompt).toBeUndefined();
  });

  it('fails the connection when a prompt is cancelled', async () => {
    const m = new MockDaemon({ live: false, delayMs: 0 });
    const seen: ConnState[] = [];
    const done = m.connect(ssh('hostkey-box'), (s) => seen.push(s));
    await new Promise((r) => setTimeout(r, 0));
    await m.answerPrompt(seen.at(-1)!.prompt!.id, null);
    await expect(done).rejects.toThrow(/Host key verification failed/);
    expect(seen.at(-1)).toMatchObject({ phase: 'error', error: 'Host key verification failed.' });
  });
});
