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
    status: vi.fn(async () => ({ current: '0.9.0', hasToken: true, supported: true })),
    setToken: vi.fn(async () => {}),
    clearToken: vi.fn(async () => {}),
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

describe('github token step', () => {
  it('checks at start without a token and never opens the token sheet by itself', async () => {
    const b = backend({ status: async () => ({ current: '0.9.0', hasToken: false, supported: true }) });
    const u = new ApkUpdater(b);
    const stop = u.start();
    await flush();
    stop();
    expect(b.check).toHaveBeenCalledTimes(1);
    expect(u.phase).toBe('available');
    expect(u.hasToken).toBe(false);
    expect(u.tokenSheet).toBe(false);
  });

  it('offers the optional token sheet only after a rate limit without a token', async () => {
    const u = new ApkUpdater(backend({ check: async () => Promise.reject({ kind: 'ratelimited' }) }));
    await u.check();
    expect(u.phase).toBe('error');
    expect(u.tokenSheet).toBe(false);
    expect(sheetAction(u)).toBe('token');
    expect(errorText(u.error!)).toMatch(/without a token.*optional GitHub token/);
  });

  it('goes from the update sheet to the token sheet and back', async () => {
    let token = false;
    const b = backend({ check: async () => (token ? available() : Promise.reject({ kind: 'ratelimited' })), setToken: vi.fn(async () => void (token = true)) });
    const u = new ApkUpdater(b);
    await u.check(true);
    expect(u.sheet).toBe(true);
    expect(sheetAction(u)).toBe('token');
    u.openToken();
    expect(u.sheet).toBe(false);
    expect(u.tokenSheet).toBe(true);
    expect(await u.saveToken('  github_pat_abc  ')).toBe(true);
    expect(b.setToken).toHaveBeenCalledWith('github_pat_abc');
    expect(u.tokenSheet).toBe(false);
    expect(u.hasToken).toBe(true);
    expect(u.sheet).toBe(true);
    expect(u.phase).toBe('available');
  });

  it('keeps the token sheet open with a clear message when GitHub refuses the token', async () => {
    const cases: [string, RegExp][] = [
      ['rejected', /rejected this token/],
      ['noaccess', /cannot read basilelt\/overseer/],
      ['forbidden', /token is valid/],
    ];
    for (const [kind, text] of cases) {
      const u = new ApkUpdater(backend({ setToken: async () => Promise.reject({ kind }) }));
      u.openToken();
      expect(await u.saveToken('ghp_bad')).toBe(false);
      expect(u.tokenSheet).toBe(true);
      expect(u.tokenBusy).toBe(false);
      expect(u.hasToken).toBeNull();
      expect(errorText(u.tokenError!)).toMatch(text);
    }
  });

  it('refuses an empty token without calling GitHub', async () => {
    const b = backend();
    const u = new ApkUpdater(b);
    expect(await u.saveToken('   ')).toBe(false);
    expect(b.setToken).not.toHaveBeenCalled();
    expect(u.tokenError?.kind).toBe('rejected');
  });

  it('removes the token', async () => {
    const b = backend();
    const u = new ApkUpdater(b);
    await u.check();
    await u.removeToken();
    expect(b.clearToken).toHaveBeenCalled();
    expect(u.hasToken).toBe(false);
    expect(b.check).toHaveBeenCalledTimes(2);
    expect(u.phase).toBe('available');
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

describe('token only, for the server download on iOS', () => {
  it('never checks for app updates', async () => {
    vi.useFakeTimers();
    const b = backend({ status: async () => ({ current: '0.9.0', hasToken: false, supported: false }) });
    const u = new ApkUpdater(b);
    u.updates = false;
    const stop = u.start(1000);
    await vi.advanceTimersByTimeAsync(5000);
    expect(b.check).not.toHaveBeenCalled();
    expect(u.tokenSheet).toBe(false);
    expect(u.hasToken).toBe(false);
    u.resume();
    await u.check(true);
    expect(b.check).not.toHaveBeenCalled();
    stop();

    u.openToken();
    expect(await u.saveToken('github_pat_x')).toBe(true);
    expect(u.hasToken).toBe(true);
    expect(u.tokenSheet).toBe(false);
    expect(b.check).not.toHaveBeenCalled();
    await u.removeToken();
    expect(u.phase).toBe('idle');
  });
});

describe('helpers', () => {
  it('normalises errors', () => {
    expect(apkError({ kind: 'noaccess' })).toEqual({ kind: 'noaccess' });
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
    expect(errorText({ kind: 'ratelimited' })).toMatch(/without a token/);
  });
});
