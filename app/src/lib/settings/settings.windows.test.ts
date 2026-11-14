import { describe, expect, it, vi } from 'vitest';
import { eventTarget } from '../../test/fake-events';

const KEY = 'overseer.settings';

function storage(initial: Record<string, string> = {}) {
  const data = new Map(Object.entries(initial));
  return {
    getItem: (k: string) => data.get(k) ?? null,
    setItem: (k: string, v: string) => void data.set(k, v),
    removeItem: (k: string) => void data.delete(k),
    stored: () => JSON.parse(data.get(KEY) ?? '{}'),
    write: (v: object) => data.set(KEY, JSON.stringify(v)),
  };
}

async function load(store: ReturnType<typeof storage>) {
  vi.resetModules();
  const listeners = eventTarget();
  vi.stubGlobal('localStorage', store);
  vi.stubGlobal('matchMedia', () => ({ matches: true, addEventListener: () => {} }));
  vi.stubGlobal('addEventListener', listeners.addEventListener);
  const mod = await import('./settings.svelte');
  const storageEvent = (key: string, newValue: string | null) => listeners.fire('storage', { key, newValue });
  return { ...mod, storageEvent };
}

const box = { kind: 'ssh' as const, host: 'box', label: 'box' };
const local = { kind: 'local' as const, label: 'Local' };

describe('settings shared by several windows', () => {
  it('only writes the changed keys so another window’s saves survive', async () => {
    const store = storage();
    const s = await load(store);
    store.write({ ...store.stored(), fontSize: 18, recentHosts: [box] });
    s.saveSettings({ sidebarWidth: 300 });
    expect(store.stored()).toMatchObject({ fontSize: 18, recentHosts: [box], sidebarWidth: 300 });
  });

  it('picks up what another window saved', async () => {
    const store = storage();
    const s = await load(store);
    const hosts = s.settings.recentHosts;
    s.storageEvent(KEY, JSON.stringify({ ...s.settings, fontSize: 20, theme: 'light' }));
    expect(s.settings.fontSize).toBe(20);
    expect(s.settings.recentHosts).toBe(hosts);
    expect(s.resolvedTheme()).toBe('light');
    s.storageEvent('other', JSON.stringify({ fontSize: 9 }));
    s.storageEvent(KEY, null);
    s.storageEvent(KEY, '{broken');
    expect(s.settings.fontSize).toBe(20);
  });

  it('lets only the main window choose the host to reopen at launch', async () => {
    const store = storage();
    const s = await load(store);
    s.rememberHost(box, false);
    expect(store.stored().lastHost).toBeUndefined();
    expect(store.stored().recentHosts).toEqual([box]);
    s.rememberHost(local, true);
    expect(store.stored().lastHost).toEqual(local);
    s.rememberHost(box, false);
    expect(store.stored().lastHost).toEqual(local);
  });

  it('remembers the selected workspace per host', async () => {
    const store = storage({ [KEY]: JSON.stringify({ workspaceId: 'legacy' }) });
    const s = await load(store);
    expect(s.workspaceFor(box)).toBe('legacy');
    s.rememberWorkspace(local, 'ws-local');
    s.rememberWorkspace(box, 'ws-box');
    expect(s.workspaceFor(local)).toBe('ws-local');
    expect(s.workspaceFor(box)).toBe('ws-box');
    expect(store.stored().workspaceByHost).toEqual({ local: 'ws-local', 'ssh:box': 'ws-box' });
  });
});
