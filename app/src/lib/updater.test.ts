import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadLocale, setLocale } from './i18n.svelte';
import { BREW_UPGRADE, LOG_LINES, Updater, brewError, brewMessage, brewStepLabel, dialogAction, pillLabel, type CheckResult, type InstallKind, type ProgressEvent, type UpdaterBackend } from './updater.svelte';

const available = (kind: InstallKind = 'bundle'): CheckResult => ({
  status: 'available',
  current: '0.2.0',
  version: '0.3.0',
  notes: '- faster',
  date: '2026-10-29T11:18:39Z',
  kind,
  releaseUrl: 'https://github.com/basilelt/overseer/releases/tag/v0.3.0',
});

function backend(over: Partial<UpdaterBackend> = {}): UpdaterBackend {
  return {
    status: vi.fn(async () => ({ current: '0.2.0', kind: 'bundle' as const, releasesUrl: '' })),
    check: vi.fn(async () => available()),
    install: vi.fn(async () => {}),
    relaunch: vi.fn(async () => {}),
    brew: vi.fn(async () => null),
    brewRelaunch: vi.fn(async () => {}),
    openUrl: vi.fn(async () => {}),
    ...over,
  };
}

afterEach(() => {
  setLocale('en');
  vi.useRealTimers();
});

describe('updater check', () => {
  it('shows the pill when an update is available', async () => {
    const u = new Updater(backend());
    expect(pillLabel(u)).toBeNull();
    await u.check();
    expect(u.phase).toBe('available');
    expect(u.info?.version).toBe('0.3.0');
    expect(pillLabel(u)).toBe('Update 0.3.0');
    expect(u.dialog).toBe(false);
    expect(dialogAction(u)).toBe('install');
  });

  it('stays quiet when up to date', async () => {
    const u = new Updater(backend({ check: async () => ({ status: 'none', current: '0.3.0' }) }));
    await u.check();
    expect(u.phase).toBe('current');
    expect(u.current).toBe('0.3.0');
    expect(pillLabel(u)).toBeNull();

  });

  it('checks the public releases without any sign-in', async () => {
    const b = backend({ status: async () => ({ current: '0.2.0', kind: 'bundle' as const, releasesUrl: '' }), check: vi.fn(async () => ({ status: 'none' as const, current: '0.2.0' })) });
    const u = new Updater(b);
    await u.refreshStatus();
    await u.check();
    expect(b.check).toHaveBeenCalledTimes(1);
    expect(u.phase).toBe('current');
    expect(dialogAction(u)).toBeNull();
  });

  it('shows the rate limit error with a way to retry', async () => {
    const limit = "GitHub's limit for requests without a token is reached: add a GitHub token in Settings > Updates, or try again later";
    const u = new Updater(backend({ check: async () => Promise.reject(new Error(limit)) }));
    await u.check(true);
    expect(u.phase).toBe('error');
    expect(u.error).toBe(limit);
    expect(dialogAction(u)).toBe('retry');
  });

  it('opens the dialog on a manual check', async () => {
    const u = new Updater(backend({ check: async () => ({ status: 'none', current: '0.3.0' }) }));
    await u.check(true);
    expect(u.dialog).toBe(true);
  });

  it('keeps a known update when a background check fails', async () => {
    let fail = false;
    const u = new Updater(
      backend({
        check: async () => {
          if (fail) throw new Error('GitHub is unreachable');
          return available();
        },
      }),
    );
    await u.check();
    fail = true;
    await u.check();
    expect(u.phase).toBe('available');
    expect(u.error).toBe('GitHub is unreachable');
    await u.check(true);
    expect(u.phase).toBe('error');
    expect(dialogAction(u)).toBe('install');
  });

  it('reports errors without an update as retryable', async () => {
    const u = new Updater(backend({ check: async () => Promise.reject(new Error('boom')) }));
    await u.check(true);
    expect(u.phase).toBe('error');
    expect(dialogAction(u)).toBe('retry');
    expect(pillLabel(u)).toBeNull();
  });

  it('reports a clear error, not a TypeError, when the backend returns nothing', async () => {
    const u = new Updater(backend({ check: async () => null as unknown as CheckResult }));
    await u.check(true);
    expect(u.phase).toBe('error');
    expect(u.error).toBe('Not available here');
    expect(u.error).not.toContain('null');
  });

  it('keeps the version empty when the status comes back empty', async () => {
    const u = new Updater(backend({ status: async () => null as never }));
    await u.refreshStatus();
    expect(u.current).toBe('');
  });

  it('does nothing without a backend', async () => {
    const u = new Updater(null);
    expect(u.enabled).toBe(false);
    await u.check();
    expect(u.phase).toBe('idle');
    expect(u.start()).toBeTypeOf('function');
  });

  it('checks at start and every interval', async () => {
    vi.useFakeTimers();
    const b = backend();
    const stop = new Updater(b).start(1000);
    await vi.advanceTimersByTimeAsync(2500);
    expect(b.check).toHaveBeenCalledTimes(3);
    stop();
    await vi.advanceTimersByTimeAsync(5000);
    expect(b.check).toHaveBeenCalledTimes(3);
  });
});

describe('updater install', () => {
  it('reports progress, then restarts', async () => {
    let emit: (e: ProgressEvent) => void = () => {};
    let finish: () => void = () => {};
    const b = backend({
      install: (p) => {
        emit = p;
        return new Promise<void>((r) => (finish = r));
      },
    });
    const u = new Updater(b);
    await u.check();
    const done = u.install();
    expect(u.phase).toBe('downloading');
    expect(pillLabel(u)).toBe('Downloading…');
    emit({ event: 'progress', downloaded: 25, total: 100 });
    expect(u.progress).toBe(0.25);
    expect(pillLabel(u)).toBe('Updating 25%');
    expect(dialogAction(u)).toBeNull();
    emit({ event: 'installing' });
    expect(u.phase).toBe('installing');
    expect(pillLabel(u)).toBe('Installing…');
    finish();
    await done;
    expect(u.phase).toBe('updated');
    expect(u.installedVersion).toBe('0.3.0');
    expect(dialogAction(u)).toBe('restart');
    expect(pillLabel(u)).toBe('Restart to finish');
    expect(b.relaunch).not.toHaveBeenCalled();
    await u.restart();
    expect(u.phase).toBe('restarting');
    expect(b.relaunch).toHaveBeenCalledOnce();
    expect(b.brewRelaunch).not.toHaveBeenCalled();
  });

  it('keeps the restart prompt when the relaunch fails', async () => {
    const u = new Updater(backend({ relaunch: async () => Promise.reject(new Error('relaunch failed')) }));
    await u.check();
    await u.install();
    await u.restart();
    expect(u.phase).toBe('updated');
    expect(u.error).toBe('relaunch failed');
    expect(dialogAction(u)).toBe('restart');
  });

  it('lets the user retry a failed download', async () => {
    const u = new Updater(backend({ install: async () => Promise.reject(new Error('signature mismatch')) }));
    await u.check();
    await u.install();
    expect(u.phase).toBe('error');
    expect(u.error).toBe('signature mismatch');
    expect(pillLabel(u)).toBe('Update 0.3.0');
    expect(dialogAction(u)).toBe('install');
  });

  it('never installs Homebrew or package installs', async () => {
    for (const [kind, action] of [
      ['homebrew', 'brew'],
      ['package', 'release'],
    ] as const) {
      const b = backend({ check: async () => available(kind) });
      const u = new Updater(b);
      await u.check();
      expect(dialogAction(u)).toBe(action);
      await u.install();
      expect(b.install).not.toHaveBeenCalled();
      expect(u.phase).toBe('available');
    }
  });
});

describe('homebrew update', () => {
  it('copyable upgrade names the tap so Homebrew trusts the cask', () => {
    expect(BREW_UPGRADE).toBe('brew update && brew upgrade --cask basilelt/tap/overseer');
  });

  function controlled() {
    let emit: (e: ProgressEvent) => void = () => {};
    let finish: (v: string | null) => void = () => {};
    let reject: (e: unknown) => void = () => {};
    const b = backend({
      check: async () => available('homebrew'),
      brew: vi.fn((_trust: string | null, p: (e: ProgressEvent) => void) => {
        emit = p;
        return new Promise<string | null>((res, rej) => {
          finish = res;
          reject = rej;
        });
      }),
    });
    return { b, emit: (e: ProgressEvent) => emit(e), finish: (v: string | null = '0.3.0') => finish(v), reject: (e: unknown) => reject(e) };
  }

  it('streams progress, then asks to restart', async () => {
    const c = controlled();
    const u = new Updater(c.b);
    await u.check();
    expect(dialogAction(u)).toBe('brew');
    const done = u.brewUpgrade();
    expect(u.phase).toBe('updating');
    expect(u.busy).toBe(true);
    expect(pillLabel(u)).toBe('Updating…');
    expect(brewStepLabel(u.step)).toBe('Updating…');
    c.emit({ event: 'step', name: 'update' });
    expect(brewStepLabel(u.step)).toBe('Refreshing Homebrew…');
    c.emit({ event: 'step', name: 'upgrade' });
    c.emit({ event: 'line', text: '==> Upgrading overseer' });
    c.emit({ event: 'line', text: 'overseer 0.2.0 -> 0.3.0' });
    expect(u.lastLine).toBe('overseer 0.2.0 -> 0.3.0');
    expect(u.log).toHaveLength(2);
    c.finish();
    await done;
    expect(u.phase).toBe('updated');
    expect(u.busy).toBe(false);
    expect(dialogAction(u)).toBe('restart');
    expect(u.installedVersion).toBe('0.3.0');
    expect(pillLabel(u)).toBe('Restart to finish');
    expect(c.b.brew).toHaveBeenCalledWith(null, expect.any(Function));
    await u.check();
    expect(u.phase).toBe('updated');
    await u.restart();
    expect(u.phase).toBe('restarting');
    expect(c.b.brewRelaunch).toHaveBeenCalledOnce();
    expect(c.b.relaunch).not.toHaveBeenCalled();
  });

  it('reports the version Homebrew actually installed', async () => {
    const c = controlled();
    const u = new Updater(c.b);
    await u.check();
    const done = u.brewUpgrade();
    c.emit({ event: 'line', text: 'overseer 0.2.0 -> 0.3.1' });
    c.emit({ event: 'line', text: 'overseer was successfully upgraded!' });
    c.finish('0.3.1');
    await done;
    expect(u.phase).toBe('updated');
    expect(u.error).toBe('');
    expect(u.brewError).toBeNull();
    expect(u.installedVersion).toBe('0.3.1');
    expect(dialogAction(u)).toBe('restart');
  });

  it('falls back to the offered version when brew does not report one', async () => {
    const c = controlled();
    const u = new Updater(c.b);
    await u.check();
    const done = u.brewUpgrade();
    c.finish(null);
    await done;
    expect(u.installedVersion).toBe('0.3.0');
  });

  it('retries after a stale tap, then asks to restart', async () => {
    let stale = true;
    const b = backend({ check: async () => available('homebrew'), brew: vi.fn(async () => (stale ? Promise.reject({ kind: 'stale' }) : '0.3.0')) });
    const u = new Updater(b);
    await u.check();
    await u.brewUpgrade();
    expect(u.phase).toBe('error');
    expect(dialogAction(u)).toBe('brew');
    stale = false;
    await u.brewUpgrade();
    expect(u.phase).toBe('updated');
    expect(u.error).toBe('');
    expect(dialogAction(u)).toBe('restart');
  });

  it('keeps only the last log lines', async () => {
    const c = controlled();
    const u = new Updater(c.b);
    await u.check();
    const done = u.brewUpgrade();
    for (let i = 0; i < LOG_LINES + 5; i++) c.emit({ event: 'line', text: `line ${i}` });
    expect(u.log).toHaveLength(LOG_LINES);
    expect(u.log[0]).toBe('line 5');
    c.finish();
    await done;
  });

  it('offers to trust an untrusted tap, then retries with it', async () => {
    const c = controlled();
    const u = new Updater(c.b);
    await u.check();
    const first = u.brewUpgrade();
    c.reject({ kind: 'untrusted', tap: 'basilelt/tap' });
    await first;
    expect(u.phase).toBe('error');
    expect(u.brewError).toEqual({ kind: 'untrusted', tap: 'basilelt/tap' });
    expect(u.error).toContain('brew trust basilelt/tap');
    expect(dialogAction(u)).toBe('brew');
    expect(pillLabel(u)).toBe('Update 0.3.0');
    const second = u.brewUpgrade('basilelt/tap');
    expect(u.brewError).toBeNull();
    expect(c.b.brew).toHaveBeenLastCalledWith('basilelt/tap', expect.any(Function));
    c.finish();
    await second;
    expect(u.phase).toBe('updated');
  });

  it('explains common failures', async () => {
    for (const [err, text] of [
      [{ kind: 'missing' }, 'Homebrew was not found'],
      [{ kind: 'stale' }, 'does not have this version yet'],
      [{ kind: 'failed', message: 'Error: disk full' }, 'Error: disk full'],
      [new Error('ipc closed'), 'ipc closed'],
    ] as const) {
      const u = new Updater(backend({ check: async () => available('homebrew'), brew: async () => Promise.reject(err) }));
      await u.check();
      await u.brewUpgrade();
      expect(u.phase).toBe('error');
      expect(u.error).toContain(text);
      expect(dialogAction(u)).toBe('brew');
    }
  });

  it('normalises backend errors', async () => {
    expect(brewError({ kind: 'untrusted' })).toEqual({ kind: 'untrusted', tap: 'basilelt/tap' });
    expect(brewError('boom')).toEqual({ kind: 'failed', message: 'boom' });
    expect(brewError({ kind: 'auth', message: 'old' })).toEqual({ kind: 'failed', message: 'old' });
    expect(brewError({ kind: 'nope' })).toEqual({ kind: 'failed', message: '[object Object]' });
    setLocale('fr');
    await loadLocale('fr');
    expect(brewMessage({ kind: 'untrusted', tap: 'a/b' })).toContain('brew trust a/b');
  });

  it('reports a failed relaunch', async () => {
    const u = new Updater(backend({ check: async () => available('homebrew'), brewRelaunch: async () => Promise.reject(new Error('open exited with 1')) }));
    await u.check();
    await u.restart();
    expect(u.phase).toBe('available');
    await u.brewUpgrade();
    await u.restart();
    expect(u.phase).toBe('updated');
    expect(u.error).toBe('open exited with 1');
    expect(dialogAction(u)).toBe('restart');
  });

  it('never runs brew for other installs', async () => {
    for (const kind of ['bundle', 'package'] as const) {
      const b = backend({ check: async () => available(kind) });
      const u = new Updater(b);
      await u.check();
      await u.brewUpgrade();
      expect(b.brew).not.toHaveBeenCalled();
      expect(u.phase).toBe('available');
    }
  });
});

describe('updater strings', () => {
  it('follows the locale', async () => {
    const u = new Updater(backend());
    await u.check();
    setLocale('fr');
    await loadLocale('fr');
    expect(pillLabel(u)).toBe('Mise à jour 0.3.0');
  });
});

describe('the shared updater', () => {
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.resetModules();
  });

  const native = (demo: boolean) => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    vi.stubGlobal('localStorage', { getItem: (k: string) => (demo && k === 'overseer.demo' ? '1' : null), setItem: () => {} });
    vi.resetModules();
  };

  it('checks for updates in the native app', async () => {
    native(false);
    expect((await import('./updater.svelte')).updater.enabled).toBe(true);
  });

  it('is disabled in demo mode', async () => {
    native(true);
    expect((await import('./updater.svelte')).updater.enabled).toBe(false);
  });
});
