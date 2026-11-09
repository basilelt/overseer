import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadLocale, setLocale } from './i18n.svelte';
import { Updater, dialogAction, pillLabel, type CheckResult, type InstallKind, type ProgressEvent, type UpdaterBackend } from './updater.svelte';

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

  it('installs an msix update and leaves the finish to Windows App Installer', async () => {
    const b = backend({ check: async () => available('msix') });
    const u = new Updater(b);
    await u.check();
    expect(dialogAction(u)).toBe('install');
    await u.install();
    expect(b.install).toHaveBeenCalledOnce();
    expect(u.phase).toBe('updated');
    expect(u.installedVersion).toBe('0.3.0');
    expect(dialogAction(u)).toBeNull();
    expect(pillLabel(u)).toBe('Finish in App Installer');
    await u.restart();
    expect(b.relaunch).not.toHaveBeenCalled();
  });

  it('retries a failed msix install', async () => {
    let fail = true;
    const u = new Updater(backend({ check: async () => available('msix'), install: async () => (fail ? Promise.reject(new Error('hash mismatch')) : undefined) }));
    await u.check();
    await u.install();
    expect(u.phase).toBe('error');
    expect(u.error).toBe('hash mismatch');
    expect(dialogAction(u)).toBe('install');
    fail = false;
    await u.install();
    expect(u.phase).toBe('updated');
    expect(u.error).toBe('');
  });

  it('ignores an install while busy or without an update', async () => {
    const b = backend();
    const u = new Updater(b);
    await u.install();
    expect(b.install).not.toHaveBeenCalled();
    await u.check();
    u.phase = 'installing';
    await u.install();
    expect(b.install).not.toHaveBeenCalled();
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
