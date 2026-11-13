import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DaemonError, setClient } from '../daemon/client';
import { t } from '../i18n.svelte';
import { lid, lidAvailable, loadLid, setLid } from './lid.svelte';
import { MockDaemon } from '../daemon/mock';
import { settings } from '../settings/settings.svelte';
import { app } from '../state.svelte';
import type { ConnState, HostTarget } from '../types';

class Daemon extends MockDaemon {
  calls: string[] = [];
  fail: DaemonError | null = null;

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    onState({ phase: 'connected', target });
  }

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    this.calls.push(`${method} ${path}${body === undefined ? '' : ` ${JSON.stringify(body)}`}`);
    if (this.fail) throw this.fail;
    return super.request<T>(method, path, body);
  }
}

const local: HostTarget = { kind: 'local', label: 'Local' };
const box: HostTarget = { kind: 'ssh', host: 'box', label: 'box' };
const reset = () => Object.assign(lid, { status: null, host: null, busy: false });
const input = (checked: boolean) => ({ checked }) as HTMLInputElement;

describe('lid store', () => {
  let m: Daemon;
  let notify: ReturnType<typeof vi.spyOn>;

  async function connect(target: HostTarget) {
    m = new Daemon({ live: false, delayMs: 0 });
    setClient(m);
    app.client = m;
    await app.connect(target);
    m.calls = [];
  }

  beforeEach(async () => {
    reset();
    await connect(local);
    notify = vi.spyOn(app, 'notify').mockImplementation(() => {});
  });

  afterEach(async () => {
    notify.mockRestore();
    await app.disconnect();
    settings.explicitlyDisconnected = false;
    app.mobile = false;
    reset();
  });

  it('is offered for a local daemon on the desktop only', async () => {
    expect(lidAvailable()).toBe(true);
    app.mobile = true;
    expect(lidAvailable()).toBe(false);
    app.mobile = false;
    await app.disconnect();
    await connect(box);
    expect(lidAvailable()).toBe(false);
  });

  it('loads the status once per host unless forced', async () => {
    await loadLid();
    await loadLid();
    expect(lid.status).toEqual({ enabled: false, active: false, supported: true, error: null });
    expect(m.calls).toEqual(['GET /v1/power/lid']);
    await loadLid(true);
    expect(m.calls).toHaveLength(2);
  });

  it('never asks a remote daemon', async () => {
    await app.disconnect();
    await connect(box);
    await loadLid(true);
    expect(m.calls).toEqual([]);
    expect(lid.status).toBeNull();
  });

  it('clears the status when the daemon cannot answer', async () => {
    lid.status = { enabled: true, active: true, supported: true, error: null };
    m.fail = new DaemonError(404, 'no route GET /v1/power/lid');
    await loadLid(true);
    expect(lid.status).toBeNull();
  });

  it('turns the option on and off with the body the daemon expects', async () => {
    await loadLid();
    await setLid(input(true));
    expect(lid.status).toEqual({ enabled: true, active: true, supported: true, error: null });
    await setLid(input(false));
    expect(lid.status?.enabled).toBe(false);
    expect(m.calls.slice(1)).toEqual(['PUT /v1/power/lid {"enabled":true}', 'PUT /v1/power/lid {"enabled":false}']);
    expect(notify).not.toHaveBeenCalled();
  });

  it('reverts the switch and shows why when the system refuses', async () => {
    await loadLid();
    m.fail = new DaemonError(500, 'Failed to inhibit: Access denied');
    const el = input(true);
    await setLid(el);
    expect(el.checked).toBe(false);
    expect(lid.status?.enabled).toBe(false);
    expect(lid.status?.error).toBe('Failed to inhibit: Access denied');
    expect(lid.busy).toBe(false);
    expect(notify).toHaveBeenCalledWith(t('settings.lidClosedFailed', { error: 'Failed to inhibit: Access denied' }), 'error');
  });

  it('ignores a toggle while a change is running', async () => {
    lid.busy = true;
    const el = input(true);
    await setLid(el);
    expect(el.checked).toBe(false);
    expect(m.calls).toEqual([]);
  });

  it('rejects a non boolean body like the daemon', async () => {
    await expect(m.request('PUT', '/v1/power/lid', { enabled: 'yes' })).rejects.toMatchObject({ status: 400, message: 'enabled must be a boolean' });
  });
});
