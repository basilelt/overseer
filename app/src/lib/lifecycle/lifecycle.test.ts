import { beforeEach, describe, expect, it } from 'vitest';
import { setClient } from '../daemon/client';
import { LOCAL_TARGET, startupTarget } from '../conn';
import { keepRunningOffer, lingerOff } from './lifecycle';
import { MockDaemon } from '../daemon/mock';
import { saveSettings } from '../settings/settings.svelte';
import { app } from '../state.svelte';
import { EMPTY_LAYOUT, openInFocused, workspace } from '../workspace/workspace.svelte';
import type { ConnState, HostTarget, Lifecycle } from '../types';
import { quietMock } from '../../test/quiet-mock';

const ssh: HostTarget = { kind: 'ssh', host: 'workstation', label: 'workstation' };
const hosts = [
  { id: 'h1', label: 'Atlas' },
  { id: 'h2', label: 'Box' },
];

describe('startupTarget', () => {
  it('reconnects the desktop to the last SSH host unless the user disconnected', () => {
    expect(startupTarget({ mobile: false, lastHost: ssh, savedHosts: [], mock: false })).toEqual(ssh);
    expect(startupTarget({ mobile: false, lastHost: ssh, explicitlyDisconnected: true, savedHosts: [], mock: false })).toEqual(LOCAL_TARGET);
    expect(startupTarget({ mobile: false, savedHosts: [], mock: false })).toEqual(LOCAL_TARGET);
    expect(startupTarget({ mobile: false, lastHost: LOCAL_TARGET, savedHosts: [], mock: false })).toEqual(LOCAL_TARGET);
  });

  it('opens the host picker in a secondary desktop window instead of reusing the last host', () => {
    expect(startupTarget({ mobile: false, lastHost: ssh, savedHosts: [], mock: false, secondary: true })).toBeNull();
    expect(startupTarget({ mobile: false, savedHosts: [], mock: true, secondary: true })).toBeNull();
    expect(startupTarget({ mobile: false, lastHost: ssh, savedHosts: [], mock: false, secondary: false })).toEqual(ssh);
  });

  it('reconnects the phone to the last direct host unless the user disconnected', () => {
    const last: HostTarget = { kind: 'direct', host: 'h2', label: 'Box' };
    expect(startupTarget({ mobile: true, lastHost: last, savedHosts: hosts, mock: false })).toEqual(last);
    expect(startupTarget({ mobile: true, lastDirect: last, savedHosts: hosts, mock: false })).toEqual(last);
    expect(startupTarget({ mobile: true, lastHost: last, explicitlyDisconnected: true, savedHosts: hosts, mock: false })).toBeNull();
    expect(startupTarget({ mobile: true, lastHost: { kind: 'direct', host: 'gone', label: 'x' }, savedHosts: hosts, mock: false })).toBeNull();
    expect(startupTarget({ mobile: true, savedHosts: hosts, mock: true })).toEqual({ kind: 'direct', host: 'h1', label: 'Atlas' });
  });
});

class Recorder extends MockDaemon {
  calls: string[] = [];
  seen: HostTarget | null = null;

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    this.seen = target;
    onState({ phase: 'connected', target });
  }

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    this.calls.push(`${method} ${path.split('?')[0]}`);
    return super.request<T>(method, path, body);
  }
}

describe('app relaunch', () => {
  let m: Recorder;

  beforeEach(() => {
    m = new Recorder({ live: false, delayMs: 0 });
    setClient(m);
  });

  it('reconnects to the remembered host and starts no agent for restored tabs', async () => {
    saveSettings({ lastHost: ssh, explicitlyDisconnected: false });
    const lines = await m.request<{ line: { id: string; state: string } }[]>('GET', '/v1/lines');
    const dead = lines.filter((l) => l.line.state !== 'active').map((l) => `line:${l.line.id}`);
    expect(dead.length).toBeGreaterThan(0);
    workspace.layout = dead.reduce(openInFocused, EMPTY_LAYOUT);
    m.calls = [];

    await app.init();
    await app.refresh();

    expect(m.seen).toEqual(ssh);
    expect(app.overlay).toBeNull();
    const spawned = m.calls.filter((c) => /\/resume$|^POST \/v1\/terms$|\/tasks\/[^/]+\/start$/.test(c));
    expect(spawned).toEqual([]);
  });

  it('only attaches when opening a line, and resumes only on an explicit request', async () => {
    await app.connect(LOCAL_TARGET);
    const dead = app.items.find((i) => i.lineId && !i.termId && app.data.lines.find((l) => l.line.id === i.lineId)?.line.state === 'held');
    expect(dead).toMatchObject({ lineId: expect.any(String) });
    const before = app.data.terms.length;
    await expect(app.openTerm(dead!)).rejects.toThrow(/not running/);
    expect(app.data.terms.length).toBe(before);
    expect(await app.resumeLine(dead!)).toEqual(expect.any(String));
    expect(app.data.terms.length).toBe(before + 1);
  });

  it('remembers an explicit disconnect across launches', async () => {
    saveSettings({ lastHost: ssh, explicitlyDisconnected: false });
    await app.init();
    await app.disconnect();
    m.seen = null;
    await app.init();
    expect(m.seen).toEqual(LOCAL_TARGET);
  });
});

describe('keep the server running', () => {
  const status = (over: Partial<Lifecycle['autostart']> = {}): Lifecycle => ({
    afterReboot: 'agents',
    autostart: { platform: 'systemd', supported: true, installed: false, enabled: false, current: false, supervised: false, path: null, linger: false, lingerCommand: 'loginctl enable-linger u', ...over },
  });
  const conn: ConnState = { phase: 'connected', target: ssh };

  it('offers once per remote host, only when it can and is not set up yet', () => {
    expect(keepRunningOffer(status(), conn, [])).toBe(true);
    expect(keepRunningOffer(status(), conn, ['ssh:workstation'])).toBe(false);
    expect(keepRunningOffer(status({ enabled: true }), conn, [])).toBe(false);
    expect(keepRunningOffer(status({ supported: false }), conn, [])).toBe(false);
    expect(keepRunningOffer(status(), { phase: 'connected', target: LOCAL_TARGET }, [])).toBe(false);
    expect(keepRunningOffer(status(), { phase: 'connecting', target: ssh }, [])).toBe(false);
    expect(keepRunningOffer(null, conn, [])).toBe(false);
  });

  it('flags a systemd service that will not start without a login', () => {
    expect(lingerOff(status({ enabled: true }))).toBe(true);
    expect(lingerOff(status({ enabled: true, linger: true }))).toBe(false);
    expect(lingerOff(status({ enabled: true, linger: null }))).toBe(false);
    expect(lingerOff(status({ enabled: true, platform: 'launchd', linger: null }))).toBe(false);
    expect(lingerOff(status())).toBe(false);
  });

  it('round-trips through the mock daemon', async () => {
    const m = quietMock();
    const on = await m.request<Lifecycle>('PATCH', '/v1/lifecycle', { autostart: true, afterReboot: 'shells' });
    expect(on.autostart.enabled).toBe(true);
    expect(on.afterReboot).toBe('shells');
    await expect(m.request('PATCH', '/v1/lifecycle', { afterReboot: 'all' })).rejects.toThrow();
  });
});

