import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DaemonError, setClient } from '../daemon/client';
import { t } from '../i18n.svelte';
import { keepRunningAnswered, lifecycle, loadLifecycle, patchLifecycle } from './lifecycle.svelte';
import { MockDaemon } from '../daemon/mock';
import { settings } from '../settings/settings.svelte';
import { app } from '../state.svelte';
import type { ConnState, HostTarget, Lifecycle } from '../types';

class Daemon extends MockDaemon {
  calls: string[] = [];
  bodies: unknown[] = [];
  override: Record<string, (body: unknown) => unknown> = {};

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    onState({ phase: 'connected', target });
  }

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const key = `${method} ${path.split('?')[0]}`;
    this.calls.push(key);
    if (method === 'PATCH') this.bodies.push(body);
    const fn = this.override[key];
    if (fn) return fn(body) as T;
    return super.request<T>(method, path, body);
  }
}

const box: HostTarget = { kind: 'ssh', host: 'box', label: 'box' };
const reset = () => Object.assign(lifecycle, { status: null, host: null, busy: false });

describe('lifecycle store', () => {
  let m: Daemon;
  let notify: ReturnType<typeof vi.spyOn>;

  beforeEach(async () => {
    reset();
    settings.keepRunningOffered = [];
    m = new Daemon({ live: false, delayMs: 0 });
    setClient(m);
    app.client = m;
    await app.connect(box);
    m.calls = [];
    notify = vi.spyOn(app, 'notify').mockImplementation(() => {});
  });

  afterEach(async () => {
    notify.mockRestore();
    await app.disconnect();
    settings.explicitlyDisconnected = false;
    settings.keepRunningOffered = [];
    reset();
  });

  describe('loadLifecycle', () => {
    it('loads the status once per host', async () => {
      await loadLifecycle();
      await loadLifecycle();
      expect(lifecycle.host).toBe('ssh:box');
      expect(lifecycle.status?.autostart.platform).toBe('systemd');
      expect(m.calls).toEqual(['GET /v1/lifecycle']);
    });

    it('reloads when forced or when the host changed', async () => {
      await loadLifecycle();
      await loadLifecycle(true);
      lifecycle.host = 'local:';
      await loadLifecycle();
      expect(m.calls).toHaveLength(3);
      expect(lifecycle.host).toBe('ssh:box');
    });

    it('clears the status when the daemon fails', async () => {
      await loadLifecycle();
      m.override['GET /v1/lifecycle'] = () => {
        throw new DaemonError(404, 'no route');
      };
      await loadLifecycle(true);
      expect(lifecycle.status).toBeNull();
    });

    it('does nothing unless fully connected', async () => {
      app.conn = { phase: 'reconnecting', target: box };
      await loadLifecycle();
      await app.disconnect();
      await loadLifecycle();
      expect(m.calls).toEqual([]);
      expect(lifecycle.host).toBeNull();
    });
  });

  describe('patchLifecycle', () => {
    it('warns that lingering is needed when autostart is on but linger is off', async () => {
      await patchLifecycle({ autostart: true });
      expect(lifecycle.status?.autostart.enabled).toBe(true);
      expect(notify).toHaveBeenCalledWith(t('lifecycle.lingerOff', { command: 'loginctl enable-linger alice' }));
    });

    it('confirms autostart once lingering is already on', async () => {
      await patchLifecycle({ linger: true });
      expect(notify).toHaveBeenLastCalledWith(t('lifecycle.lingerOn'));
      await patchLifecycle({ autostart: true });
      expect(notify).toHaveBeenLastCalledWith(t('lifecycle.enabled'));
    });

    it('confirms that autostart was turned off', async () => {
      await patchLifecycle({ autostart: false });
      expect(lifecycle.status?.autostart.enabled).toBe(false);
      expect(notify).toHaveBeenCalledWith(t('lifecycle.disabled'));
    });

    it('stays silent when only the after-reboot choice changes', async () => {
      await patchLifecycle({ afterReboot: 'shells' });
      expect(lifecycle.status?.afterReboot).toBe('shells');
      expect(notify).not.toHaveBeenCalled();
    });

    it('shows the error the daemon reports in the status', async () => {
      const failed: Lifecycle = {
        afterReboot: 'agents',
        autostart: { platform: 'systemd', supported: true, installed: false, enabled: false, current: false, supervised: false, path: null, linger: false, lingerCommand: null },
        error: 'systemctl failed',
      };
      m.override['PATCH /v1/lifecycle'] = () => failed;
      await patchLifecycle({ autostart: true });
      expect(notify).toHaveBeenCalledWith(t('lifecycle.failed', { error: 'systemctl failed' }), 'error');
      expect(notify).toHaveBeenCalledTimes(1);
      expect(lifecycle.status?.error).toBe('systemctl failed');
    });

    it('reports a rejected patch, keeps the previous status and clears busy', async () => {
      await loadLifecycle();
      const before = lifecycle.status;
      await patchLifecycle({ afterReboot: 'all' as never });
      expect(notify).toHaveBeenCalledWith(t('lifecycle.failed', { error: 'afterReboot must be agents, shells or nothing' }), 'error');
      expect(lifecycle.status).toBe(before);
      expect(lifecycle.busy).toBe(false);
    });

    it('ignores a second patch while one is running', async () => {
      lifecycle.busy = true;
      await patchLifecycle({ autostart: true });
      expect(m.calls).toEqual([]);
      expect(notify).not.toHaveBeenCalled();
    });

    it('sends the patch as given', async () => {
      await patchLifecycle({ autostart: true, afterReboot: 'shells' });
      expect(m.bodies).toEqual([{ autostart: true, afterReboot: 'shells' }]);
    });
  });

  describe('keepRunningAnswered', () => {
    it('records the host once', () => {
      keepRunningAnswered();
      keepRunningAnswered();
      expect(settings.keepRunningOffered).toEqual(['ssh:box']);
    });

    it('keeps hosts answered before', () => {
      settings.keepRunningOffered = ['ssh:other'];
      keepRunningAnswered();
      expect(settings.keepRunningOffered).toEqual(['ssh:other', 'ssh:box']);
    });

    it('starts the list when none was saved', () => {
      settings.keepRunningOffered = undefined;
      keepRunningAnswered();
      expect(settings.keepRunningOffered).toEqual(['ssh:box']);
    });
  });
});
