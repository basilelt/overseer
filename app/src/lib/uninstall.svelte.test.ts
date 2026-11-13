import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { render } from 'svelte/server';
import { DaemonError, setClient } from './daemon/client';
import { MockDaemon } from './daemon/mock';
import { app } from './state.svelte';
import { applyUninstall, canQuit, planUninstall, quitApp, resetUninstall, uninstall } from './uninstall.svelte';
import UninstallDialog from './components/overlays/UninstallDialog.svelte';
import ServerSettings from './components/overlays/ServerSettings.svelte';
import type { ConnState, HostTarget, UninstallReport } from './types';
import { quietMock } from '../test/quiet-mock';

class Daemon extends MockDaemon {
  calls: { path: string; body: unknown }[] = [];
  fail: Error | null = null;

  override async connect(target: HostTarget, onState: (s: ConnState) => void): Promise<void> {
    onState({ phase: 'connected', target });
  }

  override async request<T>(method: string, path: string, body?: unknown): Promise<T> {
    if (path === '/v1/uninstall') {
      this.calls.push({ path, body });
      if (this.fail) throw this.fail;
    }
    return super.request<T>(method, path, body);
  }
}

const box: HostTarget = { kind: 'ssh', host: 'box', label: 'box' };
const clean = () => Object.assign(uninstall, { phase: 'idle', report: null, error: '' });

describe('MockDaemon uninstall route', () => {
  it('stays a dry run unless dryRun is exactly false', async () => {
    const m = quietMock();
    for (const body of [undefined, {}, { dryRun: true }, { dryRun: 'false' }, { dryRun: 0 }]) {
      expect((await m.request<UninstallReport>('POST', '/v1/uninstall', body)).dryRun).toBe(true);
    }
    expect((await m.request<UninstallReport>('POST', '/v1/uninstall', { dryRun: false })).dryRun).toBe(false);
  });

  it('lists removed items and what it keeps with a reason', async () => {
    const m = quietMock();
    const r = await m.request<UninstallReport>('POST', '/v1/uninstall', {});
    expect(r.removed.map((i) => i.kind)).toContain('hook');
    expect(r.kept[0]).toMatchObject({ kind: 'worktree', reason: expect.any(String) });
    expect(r.errors).toEqual([]);
  });
});

describe('uninstall flow', () => {
  let m: Daemon;

  beforeEach(async () => {
    clean();
    m = new Daemon({ live: false, delayMs: 0 });
    setClient(m);
    app.client = m;
    await app.connect(box);
  });

  afterEach(async () => {
    clean();
    await app.disconnect();
  });

  it('plans with a dry run and removes nothing before the user confirms', async () => {
    await planUninstall();
    expect(uninstall.phase).toBe('confirm');
    expect(m.calls.map((c) => c.body)).toEqual([{ dryRun: true }]);
    expect(uninstall.report?.dryRun).toBe(true);
  });

  it('cancel closes the dialog without applying', async () => {
    await planUninstall();
    resetUninstall();
    expect(uninstall).toMatchObject({ phase: 'idle', report: null });
    expect(m.calls).toHaveLength(1);
    await applyUninstall();
    expect(m.calls).toHaveLength(1);
  });

  it('confirm applies and shows the result', async () => {
    await planUninstall();
    await applyUninstall();
    expect(uninstall.phase).toBe('done');
    expect(m.calls.map((c) => c.body)).toEqual([{ dryRun: true }, { dryRun: false }]);
    expect(uninstall.report?.dryRun).toBe(false);
  });

  it('disconnects after a successful apply so the app does not start the server again', async () => {
    const drop = vi.spyOn(m, 'disconnect');
    await planUninstall();
    expect(drop).not.toHaveBeenCalled();
    expect(app.conn.phase).toBe('connected');
    await applyUninstall();
    expect(drop).toHaveBeenCalledOnce();
    expect(app.conn.phase).toBe('idle');
    expect(uninstall.phase).toBe('done');
  });

  it('stays connected when the apply fails', async () => {
    const drop = vi.spyOn(m, 'disconnect');
    await planUninstall();
    m.fail = new DaemonError(500, 'boom');
    await applyUninstall();
    expect(drop).not.toHaveBeenCalled();
    expect(app.conn.phase).toBe('connected');
  });

  it('ignores a second start while one is running', async () => {
    const a = planUninstall();
    await planUninstall();
    await a;
    expect(m.calls).toHaveLength(1);
  });

  it('cannot be dismissed while it is working', async () => {
    uninstall.phase = 'applying';
    resetUninstall();
    expect(uninstall.phase).toBe('applying');
  });

  it('shows a daemon error from the dry run', async () => {
    m.fail = new DaemonError(404, 'not found');
    await planUninstall();
    expect(uninstall).toMatchObject({ phase: 'error', error: 'not found' });
    expect(m.calls).toHaveLength(1);
    resetUninstall();
    expect(uninstall.phase).toBe('idle');
  });

  it('shows a server error from the apply', async () => {
    await planUninstall();
    m.fail = new DaemonError(500, 'boom');
    await applyUninstall();
    expect(uninstall).toMatchObject({ phase: 'error', error: 'boom' });
  });

  it('shows a rejected promise that is not an Error', async () => {
    vi.spyOn(m, 'request').mockRejectedValue('daemon down');
    await planUninstall();
    expect(uninstall).toMatchObject({ phase: 'error', error: 'daemon down' });
  });

  it('does not offer quit for a remote host', async () => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    expect(canQuit()).toBe(false);
  });

  it('reports a failed quit', async () => {
    await quitApp();
    expect(uninstall.phase).toBe('error');
  });
});

describe('uninstall dialog', () => {
  beforeEach(() => {
    clean();
    app.conn = { phase: 'connected', target: box };
  });
  afterEach(clean);

  const html = () => render(UninstallDialog, { props: {} }).body;

  it('lists what will be removed and what is kept with the reason', () => {
    uninstall.phase = 'confirm';
    uninstall.report = {
      dryRun: true,
      removed: [{ kind: 'hook', path: '/h/.claude/settings.json' }],
      kept: [{ kind: 'worktree', path: '/h/wt', reason: 'uncommitted changes' }],
      errors: [],
    };
    const h = html();
    expect(h).toContain('Remove Overseer from box');
    expect(h).toContain('Will be removed (1)');
    expect(h).toContain('/h/.claude/settings.json');
    expect(h).toContain('Kept (1)');
    expect(h).toContain('uncommitted changes');
    expect(h).toContain('Cancel');
    expect(h).toContain('>Remove Overseer<');
  });

  it('labels the lid keep-awake row and its refused admin prompt', () => {
    uninstall.phase = 'confirm';
    uninstall.report = {
      dryRun: true,
      removed: [{ kind: 'power', path: 'pmset -a disablesleep 0' }],
      kept: [],
      errors: [{ kind: 'power', path: 'powercfg lid close action', error: 'User canceled. (-128)' }],
    };
    const h = html();
    expect(h).toContain('Lid keep-awake');
    expect(h).toContain('pmset -a disablesleep 0');
    expect(h).toContain('User canceled. (-128)');
  });

  it('shows the result with failures and only a close button', () => {
    uninstall.phase = 'done';
    uninstall.report = { dryRun: false, removed: [], kept: [], errors: [{ kind: 'dir', path: '/h/.overseer', error: 'busy' }] };
    const h = html();
    expect(h).toContain('Overseer was removed');
    expect(h).toContain('Nothing to remove.');
    expect(h).toContain('Could not remove (1)');
    expect(h).toContain('busy');
    expect(h).not.toContain('>Remove Overseer<');
    expect(h).not.toContain('Quit Overseer');
  });

  it('shows the error', () => {
    uninstall.phase = 'error';
    uninstall.error = 'not found';
    expect(html()).toContain('Could not remove Overseer: not found');
  });

  it('shows progress', () => {
    uninstall.phase = 'planning';
    expect(html()).toContain('Checking what Overseer added');
    uninstall.phase = 'applying';
    expect(html()).toContain('Removing Overseer...');
  });
});

describe('server settings uninstall control', () => {
  afterEach(clean);

  it('names the remote host it acts on', () => {
    app.conn = { phase: 'connected', target: box };
    const h = render(ServerSettings, { props: {} }).body;
    expect(h).toContain('Remove Overseer from box');
    expect(h).toContain('data-tour="uninstall"');
  });

  it('says this computer for the local target', () => {
    app.conn = { phase: 'connected', target: { kind: 'local', label: 'Local' } };
    expect(render(ServerSettings, { props: {} }).body).toContain('Remove Overseer from this computer');
  });
});
