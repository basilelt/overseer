import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DEMO_KEY, DEMO_PREFIX, enterDemo, exitDemo, installDemoStorage, isDemo, scopedStorage, startDemo, uninstallDemoStorage } from './demo';

class MemStorage implements Storage {
  private m = new Map<string, string>();
  get length() {
    return this.m.size;
  }
  key(i: number) {
    return [...this.m.keys()][i] ?? null;
  }
  getItem(k: string) {
    return this.m.get(k) ?? null;
  }
  setItem(k: string, v: string) {
    this.m.set(k, String(v));
  }
  removeItem(k: string) {
    this.m.delete(k);
  }
  clear() {
    this.m.clear();
  }
  dump() {
    return Object.fromEntries(this.m);
  }
}

let real: MemStorage;

beforeEach(() => {
  real = new MemStorage();
  vi.stubGlobal('localStorage', real);
});

afterEach(() => {
  uninstallDemoStorage();
  vi.unstubAllGlobals();
});

describe('isDemo', () => {
  it('is off by default', () => {
    expect(isDemo()).toBe(false);
  });

  it('is on when the flag is set', () => {
    real.setItem(DEMO_KEY, '1');
    expect(isDemo()).toBe(true);
  });

  it('is off when storage throws', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('denied');
      },
    });
    expect(isDemo()).toBe(false);
  });

  it('is off when there is no storage', () => {
    vi.stubGlobal('localStorage', undefined);
    expect(isDemo()).toBe(false);
  });
});

describe('scopedStorage', () => {
  it('reads and writes under the prefix only', () => {
    real.setItem('overseer.settings', 'real');
    const s = scopedStorage(real);
    expect(s.getItem('overseer.settings')).toBeNull();
    s.setItem('overseer.settings', 'demo');
    expect(real.getItem('overseer.settings')).toBe('real');
    expect(real.getItem(`${DEMO_PREFIX}overseer.settings`)).toBe('demo');
    expect(s.getItem('overseer.settings')).toBe('demo');
  });

  it('lists and clears only its own keys', () => {
    real.setItem('keep', '1');
    const s = scopedStorage(real);
    s.setItem('a', '1');
    s.setItem('b', '2');
    expect(s.length).toBe(2);
    expect([s.key(0), s.key(1)].sort()).toEqual(['a', 'b']);
    expect(s.key(5)).toBeNull();
    s.removeItem('a');
    expect(s.getItem('a')).toBeNull();
    s.clear();
    expect(s.length).toBe(0);
    expect(real.dump()).toEqual({ keep: '1' });
  });
});

describe('installDemoStorage', () => {
  it('leaves storage alone outside the demo', () => {
    const target = { localStorage: real as Storage };
    expect(installDemoStorage(target)).toBe(false);
    expect(target.localStorage).toBe(real);
  });

  it('routes every store to the demo scope while the flag stays readable', () => {
    real.setItem(DEMO_KEY, '1');
    real.setItem('overseer.settings', '{"lastHost":{"kind":"direct","host":"home"}}');
    expect(installDemoStorage()).toBe(true);
    expect(globalThis.localStorage).not.toBe(real);
    expect(globalThis.localStorage.getItem('overseer.settings')).toBeNull();
    globalThis.localStorage.setItem('overseer.settings', '{"theme":"light"}');
    expect(real.getItem('overseer.settings')).toBe('{"lastHost":{"kind":"direct","host":"home"}}');
    expect(isDemo()).toBe(true);
  });

  it('reports failure instead of throwing when storage cannot be replaced', () => {
    real.setItem(DEMO_KEY, '1');
    const target = Object.freeze({ localStorage: real as Storage });
    expect(installDemoStorage(target)).toBe(false);
  });

  it('does nothing without storage', () => {
    real.setItem(DEMO_KEY, '1');
    expect(installDemoStorage({ localStorage: undefined })).toBe(false);
  });
});

describe('enterDemo and exitDemo', () => {
  it('sets the flag and reloads on enter', () => {
    const reload = vi.fn();
    enterDemo(reload);
    expect(real.getItem(DEMO_KEY)).toBe('1');
    expect(reload).toHaveBeenCalledOnce();
  });

  it('does not reload when the flag cannot be saved', () => {
    const reload = vi.fn();
    vi.stubGlobal('localStorage', {
      getItem: () => null,
      setItem: () => {
        throw new Error('quota');
      },
    });
    enterDemo(reload);
    expect(reload).not.toHaveBeenCalled();
  });

  it('survives a relaunch until exited', () => {
    enterDemo(() => {});
    expect(isDemo()).toBe(true);
    expect(installDemoStorage()).toBe(true);
    expect(isDemo()).toBe(true);
  });

  it('removes the flag and every demo key on exit, keeping real data byte for byte', () => {
    real.setItem('overseer.settings', '{"theme":"dark","recentHosts":[]}');
    real.setItem('overseer.pages', '{"direct:home":{}}');
    const before = real.dump();
    enterDemo(() => {});
    installDemoStorage();
    globalThis.localStorage.setItem('overseer.settings', '{"theme":"light","lastHost":{"kind":"direct","host":"h1"}}');
    globalThis.localStorage.setItem('overseer.pages', '{"direct:h1":{}}');
    const reload = vi.fn();
    exitDemo(reload);
    expect(reload).toHaveBeenCalledOnce();
    expect(real.dump()).toEqual(before);
    expect(isDemo()).toBe(false);
  });

  it('discards what the page writes while it unloads after exit, so nothing of the demo is left', () => {
    real.setItem('overseer.settings', 'real');
    enterDemo(() => {});
    installDemoStorage();
    exitDemo(() => {
      globalThis.localStorage.setItem('overseer.sessionPages', '{"direct:h1":{}}');
      globalThis.localStorage.setItem('overseer.settings', 'demo');
    });
    expect(real.dump()).toEqual({ 'overseer.settings': 'real' });
  });

  it('deletes leftover demo data on the next start outside the demo', () => {
    real.setItem('overseer.settings', 'real');
    real.setItem(`${DEMO_PREFIX}overseer.sessionPages`, '{}');
    expect(installDemoStorage()).toBe(false);
    expect(real.dump()).toEqual({ 'overseer.settings': 'real' });
  });

  it('still reloads on exit when storage throws', () => {
    vi.stubGlobal('localStorage', {
      get length() {
        throw new Error('denied');
      },
      getItem: () => null,
    });
    const reload = vi.fn();
    exitDemo(reload);
    expect(reload).toHaveBeenCalledOnce();
  });

  it('drops the real connection before starting the demo, without touching settings', async () => {
    const order: string[] = [];
    const client = { disconnect: vi.fn(async () => void order.push('disconnect')) };
    await startDemo(client, () => order.push('reload'));
    expect(order).toEqual(['disconnect', 'reload']);
    expect(real.dump()).toEqual({ [DEMO_KEY]: '1' });
  });

  it('starts the demo even when the real connection fails to close or is missing', async () => {
    const reload = vi.fn();
    await startDemo({ disconnect: () => Promise.reject(new Error('gone')) }, reload);
    await startDemo(null, reload);
    expect(reload).toHaveBeenCalledTimes(2);
    expect(isDemo()).toBe(true);
  });

  it('reloads the page by default', () => {
    const reload = vi.fn();
    vi.stubGlobal('location', { reload });
    enterDemo();
    exitDemo();
    expect(reload).toHaveBeenCalledTimes(2);
  });
});

describe('isolation of the demo from real data', () => {
  it('keeps real hosts, settings and sessions untouched through a whole demo', async () => {
    real.setItem('overseer.settings', JSON.stringify({ theme: 'dark', lastHost: { kind: 'direct', host: 'home', label: 'Home' }, recentHosts: [{ kind: 'ssh', host: 'build', label: 'build' }] }));
    real.setItem('overseer.pages', '{"direct:home":{"ln_1":{}}}');
    const before = real.dump();
    enterDemo(() => {});
    installDemoStorage();
    vi.resetModules();
    const { saveSettings, rememberHost, settings } = await import('../settings/settings.svelte');
    expect(settings.lastHost).toBeUndefined();
    expect(settings.recentHosts).toEqual([]);
    rememberHost({ kind: 'direct', host: 'h1', label: 'Workstation' });
    rememberHost({ kind: 'ssh', host: 'gpu', label: 'gpu' });
    saveSettings({ theme: 'light', explicitlyDisconnected: true });
    const { MockDaemon } = await import('./mock');
    const mock = new MockDaemon({ live: false, delayMs: 0 });
    await mock.saveHost({ id: '', label: 'Lab', host: 'lab.example.com', port: 22, user: 'demo', auth: 'key', rememberPassword: false });
    expect(JSON.parse(globalThis.localStorage.getItem('overseer.settings')!).theme).toBe('light');
    expect(real.getItem('overseer.settings')).toBe(before['overseer.settings']);
    exitDemo(() => {});
    uninstallDemoStorage();
    expect(real.dump()).toEqual(before);
    vi.resetModules();
    const again = await import('../settings/settings.svelte');
    expect(again.settings.lastHost).toEqual({ kind: 'direct', host: 'home', label: 'Home' });
    expect(again.settings.theme).toBe('dark');
  });
});
