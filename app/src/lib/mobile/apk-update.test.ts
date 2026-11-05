import { afterEach, describe, expect, it, vi } from 'vitest';
import { loadLocale, setLocale } from '../i18n.svelte';
import { ApkUpdater, CHECK_INTERVAL_MS, RESUME_CHECK_MS, apkError, errorText, formatSize, pillLabel, sheetAction, type ApkBackend, type ApkCheck, type DownloadEvent } from './apk-update.svelte';

const available = (ready = false): ApkCheck => ({
  status: 'available',
  current: '0.9.0',
  version: '0.10.0',
  notes: '### Features\n- in-app updates',
  date: '2026-10-29T11:18:39Z',
  releaseUrl: 'https://github.com/basilelt/overseer/releases/tag/v0.10.0',
  size: 40 * 1024 * 1024,
  ready,
});

function backend(over: Partial<ApkBackend> = {}): ApkBackend {
  return {
    status: vi.fn(async () => ({ current: '0.9.0', supported: true })),
    check: vi.fn(async () => available()),
    download: vi.fn(async (on: (e: DownloadEvent) => void) => {
      on({ event: 'progress', downloaded: 10, total: 40 });
      on({ event: 'progress', downloaded: 40, total: 40 });
      on({ event: 'verifying' });
    }),
    install: vi.fn(async () => 'started' as const),
    allowInstall: vi.fn(async () => {}),
    openUrl: vi.fn(async () => {}),
    ...over,
  };
}

const flush = () => new Promise((r) => setTimeout(r, 0));

afterEach(() => {
  setLocale('en');
  vi.useRealTimers();
});

describe('apk update check', () => {
  it('is disabled without a backend', async () => {
    const u = new ApkUpdater(null);
    expect(u.enabled).toBe(false);
    await u.check(true);
    expect(u.phase).toBe('idle');
    expect(u.start()).toBeTypeOf('function');
  });

  it('shows the pill quietly when an update is available', async () => {
    const u = new ApkUpdater(backend());
    await u.check();
    expect(u.phase).toBe('available');
    expect(u.info?.version).toBe('0.10.0');
    expect(u.sheet).toBe(false);
    expect(pillLabel(u)).toBe('Update 0.10.0');
    expect(sheetAction(u)).toBe('install');
  });

  it('says up to date', async () => {
    const u = new ApkUpdater(backend({ check: async () => ({ status: 'none', current: '0.10.0' }) }));
    await u.check(true);
    expect(u.sheet).toBe(true);
    expect(u.phase).toBe('current');
    expect(u.current).toBe('0.10.0');
    expect(pillLabel(u)).toBeNull();
    expect(sheetAction(u)).toBeNull();
  });

  it('keeps the known update after a failed background check', async () => {
    let fail = false;
    const u = new ApkUpdater(backend({ check: async () => (fail ? Promise.reject({ kind: 'network', message: 'offline' }) : available()) }));
    await u.check();
    fail = true;
    await u.check();
    expect(u.phase).toBe('available');
    expect(u.error).toEqual({ kind: 'network', message: 'offline' });
    await u.check(true);
    expect(u.phase).toBe('error');
    expect(sheetAction(u)).toBe('install');
  });
});

describe('public releases, no sign-in', () => {
  it('checks at start with no credentials of any kind', async () => {
    const b = backend();
    const u = new ApkUpdater(b);
    const stop = u.start();
    await flush();
    stop();
    expect(b.check).toHaveBeenCalledTimes(1);
    expect(u.phase).toBe('available');
    expect(sheetAction(u)).toBe('install');
  });

  it('offers a retry, never a token, after a network error', async () => {
    const u = new ApkUpdater(backend({ check: async () => Promise.reject({ kind: 'network', message: 'dns' }) }));
    await u.check(true);
    expect(u.phase).toBe('error');
    expect(sheetAction(u)).toBe('retry');
  });

  it.each(['rejected', 'ratelimited', 'forbidden', 'noaccess', 'auth', 'nope'])('maps the legacy or unknown kind %s to failed', (kind) => {
    expect(apkError({ kind })).toEqual({ kind: 'failed', message: '[object Object]' });
    expect(apkError({ kind, message: 'old' })).toEqual({ kind: 'failed', message: 'old' });
    expect(errorText(apkError({ kind }))).toBe('[object Object]');
  });

  it('shows no size and an indeterminate download when the release size is unknown', async () => {
    const u = new ApkUpdater(backend({ check: async () => ({ ...available(), size: 0 }), download: vi.fn(async () => {}) }));
    await u.check();
    expect(formatSize(u.info!.size)).toBe('');
    const p = u.install();
    expect(u.total).toBeNull();
    expect(u.progress).toBeNull();
    await p;
  });
});


describe('download and install', () => {
  it('downloads with progress, verifies, then hands off to the installer', async () => {
    const seen: (string | null)[] = [];
    const b = backend();
    const u = new ApkUpdater(b);
    await u.check();
    b.download = vi.fn(async (on: (e: DownloadEvent) => void) => {
      on({ event: 'progress', downloaded: 10, total: 40 });
      seen.push(u.phase, pillLabel(u));
      on({ event: 'verifying' });
      seen.push(u.phase, pillLabel(u));
    });
    await u.install();
    expect(seen).toEqual(['downloading', 'Downloading 25%', 'verifying', 'Verifying…']);
    expect(b.install).toHaveBeenCalledTimes(1);
    expect(u.phase).toBe('installing');
    expect(u.info?.ready).toBe(true);
    expect(sheetAction(u)).toBe('reinstall');
  });

  it('skips the download when the APK is already verified', async () => {
    const b = backend({ check: async () => available(true) });
    const u = new ApkUpdater(b);
    await u.check();
    await u.install();
    expect(b.download).not.toHaveBeenCalled();
    expect(u.phase).toBe('installing');
  });

  it('asks for the install permission, then installs when the app comes back', async () => {
    let allowed = false;
    const b = backend({ install: vi.fn(async () => (allowed ? 'started' : 'permission') as 'started' | 'permission') });
    const u = new ApkUpdater(b);
    await u.check();
    await u.install();
    expect(u.phase).toBe('permission');
    expect(sheetAction(u)).toBe('allow');
    expect(pillLabel(u)).toBe('Allow installs');
    await u.allowInstall();
    expect(b.allowInstall).toHaveBeenCalled();
    allowed = true;
    u.resume();
    await flush();
    expect(u.phase).toBe('installing');
    expect(b.download).toHaveBeenCalledTimes(1);
  });

  it('comes back to the update when the installer is dismissed', async () => {
    const u = new ApkUpdater(backend());
    await u.check();
    await u.install();
    u.resume();
    expect(u.phase).toBe('available');
    expect(sheetAction(u)).toBe('install');
  });

  it('explains a signing key mismatch', async () => {
    const u = new ApkUpdater(backend({ install: async () => 'signature' as const }));
    await u.check();
    await u.install();
    expect(u.phase).toBe('error');
    expect(u.error?.kind).toBe('signature');
    expect(errorText(u.error!)).toMatch(/different keys.*Uninstall Overseer/s);
    expect(sheetAction(u)).toBeNull();
  });

  it('reports a checksum mismatch and lets the user retry', async () => {
    const u = new ApkUpdater(backend({ download: async () => Promise.reject({ kind: 'checksum' }) }));
    await u.check();
    await u.install();
    expect(u.phase).toBe('error');
    expect(errorText(u.error!)).toMatch(/checksum/);
    expect(u.info?.ready).toBe(false);
    expect(sheetAction(u)).toBe('install');
  });

  it('does not check again while installing', async () => {
    const b = backend();
    const u = new ApkUpdater(b);
    await u.check();
    await u.install();
    await u.check(true);
    expect(b.check).toHaveBeenCalledTimes(1);
    expect(u.phase).toBe('installing');
  });
});

describe('schedule', () => {
  it('checks at start and on every interval', async () => {
    vi.useFakeTimers();
    const b = backend();
    const stop = new ApkUpdater(b).start(1000);
    await vi.advanceTimersByTimeAsync(2500);
    expect(b.check).toHaveBeenCalledTimes(3);
    stop();
    await vi.advanceTimersByTimeAsync(5000);
    expect(b.check).toHaveBeenCalledTimes(3);
    expect(CHECK_INTERVAL_MS).toBe(6 * 60 * 60 * 1000);
  });

  it('checks on resume, at most every fifteen minutes', async () => {
    let now = 1_000_000;
    const b = backend();
    const u = new ApkUpdater(b, () => now);
    await u.check();
    u.resume();
    expect(b.check).toHaveBeenCalledTimes(1);
    now += RESUME_CHECK_MS;
    u.resume();
    await flush();
    expect(b.check).toHaveBeenCalledTimes(2);
  });
});

describe('server download only, on iOS', () => {
  it('never checks for app updates', async () => {
    vi.useFakeTimers();
    const b = backend({ status: async () => ({ current: '0.9.0', supported: false }) });
    const u = new ApkUpdater(b);
    u.updates = false;
    const stop = u.start(1000);
    await vi.advanceTimersByTimeAsync(5000);
    expect(b.check).not.toHaveBeenCalled();
    u.resume();
    await u.check(true);
    expect(b.check).not.toHaveBeenCalled();
    expect(u.phase).toBe('idle');
    stop();
  });
});

describe('Google Play build', () => {
  const play = () => backend({ status: vi.fn(async () => ({ current: '0.9.0', supported: false })) });

  it('never checks GitHub, on start, interval, resume or by hand', async () => {
    vi.useFakeTimers();
    const b = play();
    const u = new ApkUpdater(b);
    const stop = u.start(1000);
    await vi.advanceTimersByTimeAsync(5000);
    u.resume();
    await u.check(true);
    stop();
    expect(b.status).toHaveBeenCalledTimes(1);
    expect(b.check).not.toHaveBeenCalled();
    expect(b.download).not.toHaveBeenCalled();
    expect(u.current).toBe('0.9.0');
    expect(u.phase).toBe('idle');
  });

  it('hides every update entry point', async () => {
    const u = new ApkUpdater(play());
    u.start();
    expect(u.enabled && u.updates).toBe(false);
    await flush();
    expect(u.enabled && u.updates).toBe(false);
    expect(u.sheet).toBe(false);
    expect(pillLabel(u)).toBeNull();
  });

  it('turns updates off when the settings page reads the status', async () => {
    const u = new ApkUpdater(play());
    expect(await u.refreshStatus()).toBe(false);
    expect(u.updates).toBe(false);
  });

  it('shows no update entry before the status confirms the GitHub build', async () => {
    let resolve!: (s: { current: string; supported: boolean }) => void;
    const b = backend({ status: () => new Promise((r) => (resolve = r)) });
    const u = new ApkUpdater(b);
    const stop = u.start();
    expect(u.updates).toBe(false);
    resolve({ current: '0.9.0', supported: true });
    await flush();
    stop();
    expect(u.updates).toBe(true);
    expect(b.check).toHaveBeenCalledTimes(1);
  });

  it('still checks when the status cannot be read', async () => {
    const b = backend({ status: async () => Promise.reject(new Error('ipc')) });
    const u = new ApkUpdater(b);
    const stop = u.start();
    await flush();
    stop();
    expect(u.updates).toBe(true);
    expect(b.check).toHaveBeenCalledTimes(1);
  });

  it('does not start checking when stopped before the status arrives', async () => {
    let resolve!: (s: { current: string; supported: boolean }) => void;
    const b = backend({ status: () => new Promise((r) => (resolve = r)) });
    const u = new ApkUpdater(b);
    u.start()();
    resolve({ current: '0.9.0', supported: true });
    await flush();
    expect(b.check).not.toHaveBeenCalled();
  });
});

describe('helpers', () => {
  it('normalises errors', () => {
    expect(apkError({ kind: 'checksum' })).toEqual({ kind: 'checksum' });
    expect(apkError({ kind: 'network', message: 'dns' })).toEqual({ kind: 'network', message: 'dns' });
    expect(apkError(new Error('boom'))).toEqual({ kind: 'failed', message: 'boom' });
    expect(apkError('plain')).toEqual({ kind: 'failed', message: 'plain' });
    expect(errorText({ kind: 'network', message: 'dns' })).toBe('GitHub is unreachable: dns');
    expect(errorText({ kind: 'failed' })).toBe('The update failed.');
  });

  it('formats sizes and speaks French', async () => {
    expect(formatSize(0)).toBe('');
    expect(formatSize(512)).toBe('1 KB');
    expect(formatSize(42 * 1024 * 1024)).toBe('42.0 MB');
    setLocale('fr');
    await loadLocale('fr');
    expect(pillLabel({ phase: 'available', info: { version: '1.0.0' } as never, progress: null })).toBe('Mise à jour 1.0.0');
  });
});

describe('startApkUpdater', () => {
  afterEach(() => vi.unstubAllGlobals());

  it('stays off in demo mode, so no update check or install prompt reaches the native side', async () => {
    vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    vi.stubGlobal('localStorage', { getItem: (k: string) => (k === 'overseer.demo' ? '1' : null) });
    const { apk, startApkUpdater } = await import('./apk-update.svelte');
    const onResume = vi.fn(() => () => {});
    startApkUpdater('android', onResume)();
    expect(onResume).not.toHaveBeenCalled();
    expect(apk.enabled).toBe(false);
  });
});
