import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DEMO_KEY } from './demo';

const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/api/core', () => ({ invoke, Channel: class {} }));

const store = new Map<string, string>();
const storage = {
  getItem: (k: string) => store.get(k) ?? null,
  setItem: (k: string, v: string) => void store.set(k, v),
  removeItem: (k: string) => void store.delete(k),
  key: (i: number) => [...store.keys()][i] ?? null,
  get length() {
    return store.size;
  },
};

async function fresh() {
  vi.resetModules();
  return import('./client');
}

beforeEach(() => {
  store.clear();
  invoke.mockReset();
  vi.stubGlobal('localStorage', storage);
  vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
});

afterEach(() => {
  vi.unstubAllGlobals();
});

describe('getClient', () => {
  it('uses the real daemon in the native app outside the demo', async () => {
    const { getClient } = await fresh();
    expect((await getClient()).kind).toBe('tauri');
  });

  it('uses the in-app mock in demo mode, with no rebuild', async () => {
    store.set(DEMO_KEY, '1');
    invoke.mockResolvedValue({ os: 'android', mobile: true });
    const { getClient } = await fresh();
    const c = await getClient();
    expect(c.kind).toBe('mock');
    expect(invoke).toHaveBeenCalledWith('app_platform');
  });

  it('keeps the demo after a relaunch until it is exited', async () => {
    store.set(DEMO_KEY, '1');
    invoke.mockResolvedValue({ os: 'ios', mobile: true });
    expect((await (await fresh()).getClient()).kind).toBe('mock');
    expect((await (await fresh()).getClient()).kind).toBe('mock');
    store.delete(DEMO_KEY);
    expect((await (await fresh()).getClient()).kind).toBe('tauri');
  });

  it('reports the real device to the demo so a phone gets the phone layout', async () => {
    store.set(DEMO_KEY, '1');
    invoke.mockResolvedValue({ os: 'android', mobile: true });
    const c = await (await fresh()).getClient();
    expect(await c.platform()).toEqual({ os: 'android', mobile: true });
  });

  it('falls back to the mock platform when the native one fails', async () => {
    store.set(DEMO_KEY, '1');
    invoke.mockRejectedValue(new Error('no such command'));
    const c = await (await fresh()).getClient();
    expect(c.kind).toBe('mock');
    expect((await c.platform()).os).toBe('linux');
  });

  it('copies and pastes through the native clipboard in the demo', async () => {
    store.set(DEMO_KEY, '1');
    invoke.mockImplementation(async (cmd: string) => (cmd === 'app_platform' ? { os: 'android', mobile: true } : cmd === 'plugin:clipboard-manager|read_text' ? 'pasted' : undefined));
    const c = await (await fresh()).getClient();
    await c.copyText('hello');
    expect(invoke).toHaveBeenCalledWith('plugin:clipboard-manager|write_text', { text: 'hello' });
    expect(await c.pasteText()).toBe('pasted');
  });

  it('never reaches the native saved hosts or keychain in the demo', async () => {
    store.set(DEMO_KEY, '1');
    invoke.mockResolvedValue({ os: 'android', mobile: true });
    const c = await (await fresh()).getClient();
    const hosts = await c.savedHosts();
    await c.saveHost({ id: '', label: 'Lab', host: 'lab.example.com', port: 22, user: 'demo', auth: 'password', rememberPassword: true });
    await c.deleteHost(hosts[0].id);
    await c.deviceKey();
    expect(invoke.mock.calls.map(([cmd]) => cmd)).toEqual(['app_platform']);
  });

  it('uses the mock in a plain browser in demo mode', async () => {
    store.set(DEMO_KEY, '1');
    vi.stubGlobal('window', {});
    const c = await (await fresh()).getClient();
    expect(c.kind).toBe('mock');
    expect(invoke).not.toHaveBeenCalled();
  });

  it('returns the same client on every call', async () => {
    store.set(DEMO_KEY, '1');
    invoke.mockResolvedValue({ os: 'linux', mobile: false });
    const { getClient } = await fresh();
    expect(await getClient()).toBe(await getClient());
  });
});

describe('mockSelected and isMockClient', () => {
  it('selects the real daemon only in the native app outside the demo', async () => {
    expect((await fresh()).mockSelected()).toBe(false);
  });

  it('selects the mock in a plain browser', async () => {
    vi.stubGlobal('window', {});
    expect((await fresh()).mockSelected()).toBe(true);
  });

  it('selects the mock in the demo, even natively', async () => {
    store.set(DEMO_KEY, '1');
    expect((await fresh()).mockSelected()).toBe(true);
  });

  it('selects the mock for a VITE_MOCK build', async () => {
    vi.stubEnv('VITE_MOCK', '1');
    expect((await fresh()).mockSelected()).toBe(true);
    vi.unstubAllEnvs();
  });

  it('recognises the mock client by its kind', async () => {
    const { isMockClient } = await fresh();
    expect(isMockClient({ kind: 'mock' })).toBe(true);
    expect(isMockClient({ kind: 'tauri' })).toBe(false);
    expect(isMockClient(null)).toBe(false);
    expect(isMockClient(undefined)).toBe(false);
  });
});
