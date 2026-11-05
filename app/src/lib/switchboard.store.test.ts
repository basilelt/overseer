import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DaemonError, setClient } from './client';
import { MockDaemon } from './mock';
import { app } from './state.svelte';
import { DISMISS_KEY } from './switchboard';
import { loadSwitchboard, offerSwitchboard, reloadApp, resetImport, runImport, switchboard, switchboardNever } from './switchboard.svelte';
import type { ConnState, HostTarget } from './types';

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
const memory = () => {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k), clear: () => m.clear() };
};

const reset = () => Object.assign(switchboard, { status: null, host: null, offered: [], checked: false, busy: false, error: '', done: null });

describe('switchboard store', () => {
  let m: Daemon;

  beforeEach(async () => {
    reset();
    vi.stubGlobal('localStorage', memory());
    m = new Daemon({ live: false, delayMs: 0, switchboard: true });
    setClient(m);
    app.client = m;
    await app.connect(box);
    m.calls = [];
  });

  afterEach(async () => {
    await app.disconnect();
    app.open(null);
    vi.unstubAllGlobals();
    reset();
  });

  describe('loadSwitchboard', () => {
    it('loads once per host', async () => {
      await loadSwitchboard();
      await loadSwitchboard();
      expect(switchboard.status?.found).toBe(true);
      expect(switchboard.host).toBe('ssh:box');
      expect(m.calls).toEqual(['GET /v1/import/switchboard']);
    });

    it('reloads when forced', async () => {
      await loadSwitchboard();
      await loadSwitchboard(true);
      expect(m.calls).toHaveLength(2);
    });

    it('clears the status when the daemon fails', async () => {
      await loadSwitchboard();
      m.override['GET /v1/import/switchboard'] = () => {
        throw new DaemonError(500, 'boom');
      };
      await loadSwitchboard(true);
      expect(switchboard.status).toBeNull();
    });
  });

  describe('offerSwitchboard', () => {
    it('opens the dialog once when data is found', async () => {
      await offerSwitchboard();
      expect(app.overlay).toBe('switchboardImport');
      app.open(null);
      await offerSwitchboard();
      expect(app.overlay).toBeNull();
    });

    it('marks the check done, found or not, so the tour can follow', async () => {
      expect(switchboard.checked).toBe(false);
      m.switchboard = { ...m.switchboard, found: false };
      await offerSwitchboard();
      expect(switchboard.checked).toBe(true);
    });

    it('stays closed when nothing is found', async () => {
      m.switchboard = { ...m.switchboard, found: false };
      await offerSwitchboard();
      expect(app.overlay).toBeNull();
    });

    it('stays closed when already imported', async () => {
      m.switchboard = { ...m.switchboard, imported: true };
      await offerSwitchboard();
      expect(app.overlay).toBeNull();
    });

    it('stays closed once the user chose Don\'t ask again', async () => {
      switchboardNever();
      expect(localStorage.getItem(DISMISS_KEY)).toBe('dismissed');
      await offerSwitchboard();
      expect(app.overlay).toBeNull();
    });

    it('does not replace another open overlay', async () => {
      app.open('settings');
      await offerSwitchboard();
      expect(app.overlay).toBe('settings');
    });

    it('stays closed when the daemon is down', async () => {
      m.override['GET /v1/import/switchboard'] = () => {
        throw new Error('down');
      };
      await offerSwitchboard();
      expect(app.overlay).toBeNull();
    });
  });

  describe('runImport', () => {
    it('posts an empty body, writes the settings and records the result', async () => {
      await loadSwitchboard();
      localStorage.setItem('overseer.mockImported', '0');
      let body: unknown;
      m.override['POST /v1/import/switchboard'] = (b) => {
        body = b;
        return { imported: true, sessions: 12, tasks: 4, workspaces: 3, stopped: true, hooks: true };
      };
      expect(await runImport()).toBe(true);
      expect(body).toEqual({});
      expect(localStorage.getItem('overseer.mockImported')).toBe('1');
      expect(switchboard.done).toMatchObject({ sessions: 12, stopped: true });
      expect(switchboard.error).toBe('');
      expect(switchboard.busy).toBe(false);
    });

    it('shows the daemon 404 message and writes nothing', async () => {
      m.switchboard = { ...m.switchboard, found: false };
      await loadSwitchboard();
      expect(await runImport()).toBe(false);
      expect(switchboard.error).toBe('no switchboard data found');
      expect(switchboard.done).toBeNull();
      expect(localStorage.getItem('overseer.mockImported')).toBeNull();
    });

    it('reports a rejected promise and a daemon that is down', async () => {
      m.override['POST /v1/import/switchboard'] = () => {
        throw 'network down';
      };
      expect(await runImport()).toBe(false);
      expect(switchboard.error).toBe('network down');
      expect(switchboard.busy).toBe(false);
    });

    it('imports even when the settings fetch fails, using the loaded ones', async () => {
      await loadSwitchboard();
      m.override['GET /v1/import/switchboard'] = () => {
        throw new Error('gone');
      };
      expect(await runImport()).toBe(true);
      expect(localStorage.getItem('overseer.mockImported')).toBe('1');
    });

    it('ignores a second call while busy', async () => {
      switchboard.busy = true;
      expect(await runImport()).toBe(false);
      expect(m.calls).toEqual([]);
    });

    it('resets the error and result', () => {
      Object.assign(switchboard, { error: 'x', done: { imported: true, sessions: 0, tasks: 0, workspaces: 0, stopped: false, hooks: false } });
      resetImport();
      expect(switchboard.error).toBe('');
      expect(switchboard.done).toBeNull();
    });
  });

  it('reloads the app through location.reload', () => {
    const reload = vi.fn();
    vi.stubGlobal('location', { reload });
    reloadApp();
    expect(reload).toHaveBeenCalledOnce();
  });
});
