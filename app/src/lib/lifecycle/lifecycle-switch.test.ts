import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DaemonError, setClient } from '../daemon/client';
import { lifecycle } from './lifecycle.svelte';
import { toggleAutostart } from './lifecycle-switch';
import { MockDaemon } from '../daemon/mock';
import { app } from '../state.svelte';
import type { ConnState, HostTarget, Lifecycle } from '../types';

class Daemon extends MockDaemon {
  override: Record<string, (body: unknown) => unknown> = {};
  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    onState({ phase: 'connected', target });
  }
  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const fn = this.override[`${method} ${path.split('?')[0]}`];
    if (fn) return fn(body) as T;
    return super.request<T>(method, path, body);
  }
}

const box: HostTarget = { kind: 'ssh', host: 'box', label: 'box' };

describe('toggleAutostart', () => {
  let m: Daemon;

  beforeEach(async () => {
    Object.assign(lifecycle, { status: null, host: null, busy: false });
    m = new Daemon({ live: false, delayMs: 0 });
    setClient(m);
    app.client = m;
    await app.connect(box);
    vi.spyOn(app, 'notify').mockImplementation(() => {});
  });

  afterEach(async () => {
    vi.restoreAllMocks();
    await app.disconnect();
    Object.assign(lifecycle, { status: null, host: null, busy: false });
  });

  it('puts the switch back off when the daemon refuses to enable autostart', async () => {
    const before = await m.request<Lifecycle>('GET', '/v1/lifecycle');
    m.override['PATCH /v1/lifecycle'] = () => ({ ...before, error: 'systemctl --user daemon-reload: Failed to connect to user scope bus', autostart: { ...before.autostart, enabled: false } });
    const input = { checked: true };
    await toggleAutostart(input);
    expect(input.checked).toBe(false);
  });

  it('puts the switch back to the previous state when the request is rejected', async () => {
    const before = await m.request<Lifecycle>('GET', '/v1/lifecycle');
    lifecycle.status = { ...before, autostart: { ...before.autostart, enabled: true } };
    m.override['PATCH /v1/lifecycle'] = () => {
      throw new DaemonError(500, 'boom');
    };
    const input = { checked: false };
    await toggleAutostart(input);
    expect(input.checked).toBe(true);
  });

  it('keeps the switch on when autostart was enabled', async () => {
    const input = { checked: true };
    await toggleAutostart(input);
    expect(input.checked).toBe(true);
  });
});
