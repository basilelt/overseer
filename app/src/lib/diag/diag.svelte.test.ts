import { afterEach, beforeEach, describe, expect, it, vi, type Mock } from 'vitest';

const h = vi.hoisted(() => ({
  invoke: vi.fn(async (..._a: unknown[]): Promise<unknown> => undefined),
  ping: null as (() => void) | null,
  unlisten: vi.fn(),
  listenGate: null as Promise<void> | null,
}));

vi.mock('@tauri-apps/api/event', () => ({
  listen: async (name: string, fn: () => void) => {
    if (h.listenGate) await h.listenGate;
    if (name === 'diag:ping') h.ping = fn;
    return h.unlisten;
  },
}));

import type { DaemonClient } from '../daemon/client';
import type { PreviousSession } from './diag';

type Mod = typeof import('./diag.svelte');

const previous: PreviousSession = {
  kind: 'crash',
  startedAt: 1,
  endedAt: null,
  detectedAt: 2,
  version: '1.0.0',
  frozenSecs: 0,
  view: 'agents',
  crashReport: 'boom',
  appLog: [],
};

type Listeners = Map<string, (e: any) => void>;

function stubWindow(tauri: boolean): Listeners {
  const listeners: Listeners = new Map();
  const win: Record<string, unknown> = {
    addEventListener: vi.fn((type: string, fn: (e: unknown) => void) => listeners.set(type, fn)),
    removeEventListener: vi.fn((type: string, fn: (e: unknown) => void) => {
      if (listeners.get(type) === fn) listeners.delete(type);
    }),
  };
  if (tauri) win.__TAURI_INTERNALS__ = { invoke: (cmd: string, args: unknown) => h.invoke(cmd, args) };
  vi.stubGlobal('window', win);
  return listeners;
}

function stubDocument() {
  const listeners: Listeners = new Map();
  const doc = {
    visibilityState: 'visible',
    addEventListener: vi.fn((type: string, fn: (e: unknown) => void) => listeners.set(type, fn)),
    removeEventListener: vi.fn((type: string, fn: (e: unknown) => void) => {
      if (listeners.get(type) === fn) listeners.delete(type);
    }),
  };
  vi.stubGlobal('document', doc);
  return { doc, listeners };
}

async function load(): Promise<Mod> {
  vi.resetModules();
  return import('./diag.svelte');
}

async function daemonError(status: number, message: string) {
  const { DaemonError } = await import('../daemon/client');
  return new DaemonError(status, message);
}

const calls = (cmd: string) => h.invoke.mock.calls.filter((c) => c[0] === cmd).map((c) => c[1]);
const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

const origError = console.error;
const origWarn = console.warn;

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['Date'] });
  vi.setSystemTime(Date.UTC(2026, 0, 1));
  h.invoke.mockReset();
  h.invoke.mockResolvedValue(undefined);
  h.ping = null;
  h.unlisten.mockReset();
  h.listenGate = null;
});

afterEach(() => {
  console.error = origError;
  console.warn = origWarn;
  vi.useRealTimers();
  vi.unstubAllGlobals();
  vi.restoreAllMocks();
});

describe('diagBackend outside Tauri', () => {
  it('serves a mock app log and neutral answers without touching Tauri', async () => {
    const { diagBackend } = await load();
    const tail = await diagBackend.tail(50);
    expect(tail.path).toBe('app.log');
    expect(tail.lines.length).toBeGreaterThan(0);
    await expect(diagBackend.info()).resolves.toBeNull();
    await expect(diagBackend.previous()).resolves.toBeNull();
    await expect(diagBackend.dismiss()).resolves.toBeUndefined();
    await expect(diagBackend.sessionEnd('quit')).resolves.toBeUndefined();
    expect(h.invoke).not.toHaveBeenCalled();
  });
});

describe('diagBackend inside Tauri', () => {
  beforeEach(() => stubWindow(true));

  it.each([
    ['tail', (b: Mod['diagBackend']) => b.tail(200), 'diag_tail', { lines: 200 }],
    ['info', (b: Mod['diagBackend']) => b.info(), 'diag_info', {}],
    ['previous', (b: Mod['diagBackend']) => b.previous(), 'diag_previous', {}],
    ['dismiss', (b: Mod['diagBackend']) => b.dismiss(), 'diag_dismiss', {}],
    ['save', (b: Mod['diagBackend']) => b.save('bundle'), 'diag_save', { text: 'bundle' }],
    ['openDir', (b: Mod['diagBackend']) => b.openDir(), 'diag_open_dir', {}],
    ['sessionEnd', (b: Mod['diagBackend']) => b.sessionEnd('update'), 'diag_session_end', { reason: 'update' }],
  ] as const)('%s invokes %s', async (_, run, cmd, args) => {
    const { diagBackend } = await load();
    h.invoke.mockResolvedValueOnce('result');
    await expect(run(diagBackend)).resolves.toBe('result');
    expect(h.invoke).toHaveBeenCalledWith(cmd, args);
  });
});

describe('fetchServerLog', () => {
  const client = (request: (...a: unknown[]) => Promise<unknown>) => ({ request: vi.fn(request) }) as unknown as DaemonClient & { request: ReturnType<typeof vi.fn> };

  it('is offline without a client or a connection', async () => {
    const { fetchServerLog } = await load();
    await expect(fetchServerLog(null, true)).resolves.toEqual({ status: 'offline' });
    const c = client(async () => ({}));
    await expect(fetchServerLog(c, false)).resolves.toEqual({ status: 'offline' });
    expect(c.request).not.toHaveBeenCalled();
  });

  it('returns the tail with the default and a custom length', async () => {
    const { fetchServerLog } = await load();
    const tail = { path: '/d.log', lines: ['a'] };
    const c = client(async () => tail);
    await expect(fetchServerLog(c, true)).resolves.toEqual({ status: 'ok', tail });
    expect(c.request).toHaveBeenCalledWith('GET', '/v1/logs?tail=300');
    await fetchServerLog(c, true, 20);
    expect(c.request).toHaveBeenLastCalledWith('GET', '/v1/logs?tail=20');
  });

  it.each([404, 405])('treats HTTP %i as an old server', async (status) => {
    const { fetchServerLog } = await load();
    const err = await daemonError(status, 'nope');
    const c = client(async () => {
      throw err;
    });
    await expect(fetchServerLog(c, true)).resolves.toEqual({ status: 'unsupported' });
  });

  it('reports other failures with their message', async () => {
    const { fetchServerLog } = await load();
    const err = await daemonError(500, 'internal');
    const c = client(async () => {
      throw err;
    });
    await expect(fetchServerLog(c, true)).resolves.toEqual({ status: 'error', message: 'Error: internal' });
    const s = client(async () => {
      throw 'socket closed';
    });
    await expect(fetchServerLog(s, true)).resolves.toEqual({ status: 'error', message: 'socket closed' });
  });
});

describe('diag dialog state', () => {
  it('opens in a mode and closes back to nothing', async () => {
    const { diag } = await load();
    diag.open('live');
    expect(diag.mode).toBe('live');
    diag.close();
    expect(diag.mode).toBeNull();
  });

  it('dismisses the previous session report when closing it', async () => {
    stubWindow(true);
    const { diag } = await load();
    diag.previous = previous;
    diag.open('previous');
    diag.close();
    expect(diag.mode).toBeNull();
    expect(diag.previous).toBeNull();
    await vi.waitFor(() => expect(h.invoke).toHaveBeenCalledWith('diag_dismiss', {}));
  });

  it('keeps the previous report when closing the live view', async () => {
    stubWindow(true);
    const { diag } = await load();
    diag.previous = previous;
    diag.open('live');
    diag.close();
    expect(diag.previous).toBe(previous);
    await flush();
    expect(h.invoke).not.toHaveBeenCalled();
  });

  it('survives a failing dismiss', async () => {
    stubWindow(true);
    h.invoke.mockRejectedValue(new Error('ipc'));
    const { diag } = await load();
    diag.open('previous');
    expect(() => diag.close()).not.toThrow();
    await vi.waitFor(() => expect(h.invoke).toHaveBeenCalled());
    await flush();
    expect(diag.mode).toBeNull();
  });
});

describe('startDiagnostics outside Tauri', () => {
  it('does nothing and leaves the console alone', async () => {
    stubWindow(false);
    const { startDiagnostics } = await load();
    const before = console.error;
    const stop = startDiagnostics(() => 'agents');
    expect(console.error).toBe(before);
    stop();
    expect(h.invoke).not.toHaveBeenCalled();
  });

  it('does nothing without a window', async () => {
    const { startDiagnostics } = await load();
    expect(() => startDiagnostics(() => 'agents')()).not.toThrow();
  });
});

describe('startDiagnostics inside Tauri', () => {
  let win: Listeners;
  let doc: ReturnType<typeof stubDocument>;
  let error: Mock<(...a: unknown[]) => void>;
  let warn: Mock<(...a: unknown[]) => void>;
  let mod: Mod;
  let stop: () => void;
  let view: string;

  beforeEach(async () => {
    win = stubWindow(true);
    doc = stubDocument();
    error = vi.fn();
    warn = vi.fn();
    console.error = error;
    console.warn = warn;
    view = 'agents';
    mod = await load();
  });

  afterEach(() => stop?.());

  const start = () => {
    stop = mod.startDiagnostics(() => view);
    return stop;
  };

  const logs = () => calls('diag_log') as { level: string; message: string; stack: string | null }[];

  it('reports the current view right away and on every ping', async () => {
    start();
    view = 'kanban';
    await vi.waitFor(() => expect(calls('diag_pong')).toEqual([{ view: 'agents' }]));
    await vi.waitFor(() => expect(h.ping).not.toBeNull());
    h.ping!();
    await vi.waitFor(() => expect(calls('diag_pong')).toEqual([{ view: 'agents' }, { view: 'kanban' }]));
  });

  it('opens the previous session report when the last run ended badly', async () => {
    h.invoke.mockImplementation(async (cmd) => (cmd === 'diag_previous' ? previous : undefined));
    start();
    await vi.waitFor(() => expect(mod.diag.mode).toBe('previous'));
    expect(mod.diag.previous).toEqual(previous);
  });

  it('stays closed after a clean exit or when the lookup fails', async () => {
    start();
    await vi.waitFor(() => expect(calls('diag_previous')).toHaveLength(1));
    await flush();
    expect(mod.diag.mode).toBeNull();
    stop();
    h.invoke.mockImplementation(async (cmd) => {
      if (cmd === 'diag_previous') throw new Error('io');
    });
    start();
    await vi.waitFor(() => expect(calls('diag_previous')).toHaveLength(2));
    await flush();
    expect(mod.diag.mode).toBeNull();
  });

  it('forwards console errors with the error stack and still prints them', async () => {
    start();
    const err = new TypeError('bad');
    console.error('render failed', err);
    expect(error).toHaveBeenCalledWith('render failed', err);
    await vi.waitFor(() => expect(logs()).toEqual([{ level: 'error', message: 'render failed TypeError: bad', stack: err.stack }]));
  });

  it('formats non-string console arguments', async () => {
    start();
    console.error({ code: 7 }, 3);
    await vi.waitFor(() => expect(logs()).toEqual([{ level: 'error', message: '{"code":7} 3', stack: null }]));
  });

  it('forwards only warnings from the subsystems worth reporting', async () => {
    start();
    console.warn('some library chatter');
    expect(warn).toHaveBeenCalledWith('some library chatter');
    console.warn('ghostty: wasm fallback');
    await vi.waitFor(() => expect(logs()).toEqual([{ level: 'warn', message: 'ghostty: wasm fallback', stack: null }]));
  });

  it('forwards uncaught errors with their location', async () => {
    start();
    const err = new Error('kaboom');
    win.get('error')!({ error: err, message: 'ignored', filename: 'App.svelte', lineno: 3, colno: 9 });
    await vi.waitFor(() => expect(logs()).toEqual([{ level: 'error', message: 'Uncaught Error: kaboom (App.svelte:3:9)', stack: err.stack }]));
  });

  it('forwards uncaught errors without an error object or a location', async () => {
    start();
    win.get('error')!({ error: null, message: 'Script error.', filename: '' });
    await vi.waitFor(() => expect(logs()).toEqual([{ level: 'error', message: 'Uncaught Script error.', stack: null }]));
  });

  it('forwards unhandled rejections', async () => {
    start();
    win.get('unhandledrejection')!({ reason: 'timeout' });
    await vi.waitFor(() => expect(logs()).toEqual([{ level: 'error', message: 'Unhandled rejection: timeout', stack: null }]));
  });

  it('reports visibility changes', async () => {
    start();
    doc.doc.visibilityState = 'hidden';
    doc.listeners.get('visibilitychange')!({});
    doc.doc.visibilityState = 'visible';
    doc.listeners.get('visibilitychange')!({});
    await vi.waitFor(() => expect(calls('diag_visibility')).toEqual([{ visible: false }, { visible: true }]));
  });

  it('rate limits repeated messages and reports how many were dropped', async () => {
    start();
    for (let i = 0; i < 5; i++) console.error('same');
    await vi.waitFor(() => expect(logs().map((l) => l.message)).toEqual(['same', 'same', 'same']));
    console.error('other');
    await vi.waitFor(() => expect(logs().slice(3)).toEqual([
      { level: 'warn', message: '2 frontend messages not forwarded (rate limit)', stack: null },
      { level: 'error', message: 'other', stack: null },
    ]));
  });

  it('caps the burst of distinct messages and recovers after the window', async () => {
    start();
    for (let i = 0; i < 25; i++) console.error(`m${i}`);
    await vi.waitFor(() => expect(logs()).toHaveLength(20));
    vi.setSystemTime(Date.now() + 10_000);
    console.error('later');
    await vi.waitFor(() => expect(logs().slice(20).map((l) => l.message)).toEqual(['5 frontend messages not forwarded (rate limit)', 'later']));
  });

  it('keeps working when forwarding fails', async () => {
    h.invoke.mockRejectedValue(new Error('ipc down'));
    start();
    expect(() => console.error('x')).not.toThrow();
    await flush();
    expect(error).toHaveBeenCalledTimes(1);
  });

  it('restores the console and removes every listener on stop', async () => {
    start();
    await vi.waitFor(() => expect(h.ping).not.toBeNull());
    stop();
    expect(console.error).toBe(error);
    expect(console.warn).toBe(warn);
    expect(win.size).toBe(0);
    expect(doc.listeners.size).toBe(0);
    expect(h.unlisten).toHaveBeenCalledTimes(1);
    console.error('after');
    expect(error).toHaveBeenCalledWith('after');
    await flush();
    expect(logs()).toEqual([]);
  });

  it('drops the ping listener that resolves after stop', async () => {
    let open!: () => void;
    h.listenGate = new Promise<void>((r) => (open = r));
    start();
    stop();
    expect(h.unlisten).not.toHaveBeenCalled();
    open();
    await vi.waitFor(() => expect(h.unlisten).toHaveBeenCalledTimes(1));
  });
});

describe('dt', () => {
  it('returns English strings by default and fills parameters', async () => {
    const { dt } = await load();
    expect(dt('title')).toBe('Logs');
    expect(dt('saved', { path: '/tmp/x.log' })).toBe('Saved to /tmp/x.log');
  });

  it('switches to French', async () => {
    const { dt } = await load();
    const i18n = await import('../i18n.svelte');
    i18n.setLocale('fr');
    await i18n.loadLocale('fr');
    expect(dt('title')).toBe('Journaux');
    expect(dt('lines', { n: 3 })).toBe('3 lignes');
  });

  it('keeps unknown placeholders visible', async () => {
    const { dt } = await load();
    expect(dt('freezeHint', {})).toContain('{n}');
  });

  it('falls back to English for an unsupported locale', async () => {
    const { dt } = await load();
    (await import('../i18n.svelte')).setLocale('xx');
    expect(dt('close')).toBe('Close');
  });
});
