import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { agentName } from './agents';
import { DaemonError, setClient } from './daemon/client';
import { hooks, hooksLater, hooksNever, loadHooks, setHooks } from './hooks.svelte';
import { t } from './i18n.svelte';
import { MockDaemon } from './daemon/mock';
import { settings } from './settings/settings.svelte';
import { app } from './state.svelte';
import type { ConnState, HooksRun, HostTarget } from './types';

class Daemon extends MockDaemon {
  calls: string[] = [];
  override: Record<string, (body: unknown) => unknown> = {};

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    onState({ phase: 'connected', target });
  }

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    const key = `${method} ${path.split('?')[0]}`;
    this.calls.push(key);
    const fn = this.override[key];
    if (fn) return fn(body) as T;
    return super.request<T>(method, path, body);
  }
}

const box: HostTarget = { kind: 'ssh', host: 'box', label: 'box' };
const reset = () => Object.assign(hooks, { status: null, host: null, later: [], busy: false });

describe('hooks store', () => {
  let m: Daemon;
  let notify: ReturnType<typeof vi.spyOn>;

  beforeEach(async () => {
    reset();
    settings.hooksNever = [];
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
    settings.hooksNever = [];
    reset();
  });

  describe('loadHooks', () => {
    it('loads the status once per host and remembers the host id', async () => {
      await loadHooks();
      await loadHooks();
      expect(hooks.host).toBe('ssh:box');
      expect(hooks.status?.agents.map((a) => a.agent)).toContain('claude');
      expect(m.calls).toEqual(['GET /v1/hooks/status']);
    });

    it('reloads when forced', async () => {
      await loadHooks();
      await loadHooks(true);
      expect(m.calls).toEqual(['GET /v1/hooks/status', 'GET /v1/hooks/status']);
    });

    it('reloads when the host changed', async () => {
      await loadHooks();
      hooks.host = 'ssh:other';
      await loadHooks();
      expect(hooks.host).toBe('ssh:box');
      expect(m.calls).toHaveLength(2);
    });

    it('clears the status when the daemon fails', async () => {
      await loadHooks();
      m.override['GET /v1/hooks/status'] = () => {
        throw new DaemonError(500, 'boom');
      };
      await loadHooks(true);
      expect(hooks.status).toBeNull();
      expect(hooks.host).toBe('ssh:box');
    });

    it('does nothing while disconnected or still connecting', async () => {
      app.conn = { phase: 'reconnecting', target: box };
      await loadHooks(true);
      expect(m.calls).toEqual([]);
      await app.disconnect();
      await loadHooks(true);
      expect(m.calls).toEqual([]);
      expect(hooks.status).toBeNull();
    });
  });

  describe('setHooks', () => {
    it('ignores an empty agent list', async () => {
      await setHooks([], true);
      expect(m.calls).toEqual([]);
      expect(notify).not.toHaveBeenCalled();
    });

    it('ignores a second request while one is running', async () => {
      hooks.busy = true;
      await setHooks(['claude'], true);
      expect(m.calls).toEqual([]);
      expect(hooks.busy).toBe(true);
    });

    it('installs, names the agents, writes for real and reloads the status', async () => {
      const bodies: unknown[] = [];
      m.override['POST /v1/hooks/install'] = (body) => {
        bodies.push(body);
        return { dryRun: false, results: [{ agent: 'claude', path: '', changed: true, written: true, diff: '' }] } satisfies HooksRun;
      };
      await setHooks(['claude'], true);
      expect(bodies).toEqual([{ agents: ['claude'], dryRun: false }]);
      expect(notify).toHaveBeenCalledWith(t('hooks.installed', { agents: agentName(app.agents, 'claude') }));
      expect(m.calls).toEqual(['POST /v1/hooks/install', 'GET /v1/hooks/status']);
      expect(hooks.busy).toBe(false);
      expect(hooks.status).not.toBeNull();
    });

    it('reports the removal of hooks', async () => {
      await setHooks(['claude', 'opencode'], false);
      expect(m.calls[0]).toBe('POST /v1/hooks/uninstall');
      expect(notify).toHaveBeenCalledWith(t('hooks.removed', { agents: [agentName(app.agents, 'claude'), agentName(app.agents, 'opencode')].join(', ') }));
    });

    it('asks the user to trust codex hooks after writing them', async () => {
      await setHooks(['claude', 'codex'], true);
      expect(notify).toHaveBeenCalledTimes(1);
      expect(notify).toHaveBeenCalledWith(t('hooks.codexTrust'));
      expect(m.hooks.agents.find((a) => a.agent === 'codex')?.installed).toBe(true);
    });

    it('lists every failed agent with its error as an error toast', async () => {
      m.override['POST /v1/hooks/install'] = () => ({
        dryRun: false,
        results: [
          { agent: 'claude', path: '', changed: false, written: false, diff: '', error: 'read-only' },
          { agent: 'codex', path: '', changed: false, written: false, diff: '', error: 'bad json' },
          { agent: 'opencode', path: '', changed: true, written: true, diff: '' },
        ],
      });
      await setHooks(['claude', 'codex', 'opencode'], true);
      const claude = agentName(app.agents, 'claude');
      const codex = agentName(app.agents, 'codex');
      expect(notify).toHaveBeenCalledWith(t('hooks.failed', { error: `${claude}: read-only; ${codex}: bad json` }), 'error');
      expect(notify).toHaveBeenCalledTimes(1);
    });

    it('stays silent when every agent was skipped', async () => {
      m.override['POST /v1/hooks/install'] = () => ({ dryRun: false, results: [{ agent: 'claude', path: '', changed: false, written: false, diff: '', skipped: 'unchanged' }] });
      await setHooks(['claude'], true);
      expect(notify).not.toHaveBeenCalled();
      expect(m.calls).toContain('GET /v1/hooks/status');
    });

    it('reports a thrown request, clears busy and still reloads the status', async () => {
      m.override['POST /v1/hooks/install'] = () => {
        throw new Error('socket closed');
      };
      await setHooks(['claude'], true);
      expect(notify).toHaveBeenCalledWith(t('hooks.failed', { error: 'socket closed' }), 'error');
      expect(hooks.busy).toBe(false);
      expect(m.calls).toEqual(['POST /v1/hooks/install', 'GET /v1/hooks/status']);
    });
  });

  describe('declining the offer', () => {
    it('postpones only for the current host, once a host is known', () => {
      hooksLater();
      expect(hooks.later).toEqual([]);
      hooks.host = 'ssh:box';
      hooksLater();
      expect(hooks.later).toEqual(['ssh:box']);
    });

    it('never offers again for the host, without duplicates', () => {
      hooksNever();
      expect(settings.hooksNever).toEqual([]);
      hooks.host = 'ssh:box';
      hooksNever();
      hooksNever();
      expect(settings.hooksNever).toEqual(['ssh:box']);
    });
  });
});
