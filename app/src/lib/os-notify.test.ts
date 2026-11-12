import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DEMO_KEY } from './demo';
import { center } from './notifications.svelte';
import { DEFAULT_KINDS, DEFAULT_QUIET, FOCUS_FALLBACK_MS } from './notify-prefs';
import {
  cooldownGate,
  dispatch,
  isTargetMuted,
  mutedList,
  openSystemSettings,
  osId,
  permission,
  requestPermission,
  sendOs,
  sendTest,
  startOs,
  toggleTargetMute,
} from './os-notify';
import { settings } from './settings.svelte';
import type { app as App } from './state.svelte';

const plug = vi.hoisted(() => ({
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
  onAction: vi.fn(),
  createChannel: vi.fn(),
  removeActive: vi.fn(),
  sendNotification: vi.fn(),
  Importance: { High: 4, Default: 3, Low: 2 },
  Visibility: { Private: 0 },
}));
const win = vi.hoisted(() => ({ unminimize: vi.fn(), setFocus: vi.fn() }));
const invoke = vi.hoisted(() => vi.fn());
vi.mock('@tauri-apps/plugin-notification', () => plug);
vi.mock('@tauri-apps/api/window', () => ({ getCurrentWindow: () => win }));
vi.mock('@tauri-apps/api/core', () => ({ invoke }));

const fakeApp = () =>
  ({
    os: 'linux',
    selectedKey: null as string | null,
    itemWs: new Map([['line:a', 'ws1']]),
    ws: { groups: [{ id: 'g1', members: ['line:b'] }], workspaces: [] },
    conn: { target: { kind: 'local' } },
    open: vi.fn(),
    select: vi.fn(),
  }) as unknown as typeof App;

describe('dispatch', () => {
  let focused = false;
  const show = vi.fn();

  beforeEach(() => {
    focused = false;
    show.mockReset();
    center.clear();
    vi.stubGlobal('document', { hasFocus: () => focused });
    Object.assign(settings, {
      notifyKinds: { ...DEFAULT_KINDS },
      notifyQuiet: { ...DEFAULT_QUIET },
      notifyOnlyUnfocused: false,
      notifySkipWatched: true,
      notifyBackgroundToasts: true,
      notifyCooldown: 20,
      notifyMuted: {},
    });
  });
  afterEach(() => {
    vi.useRealTimers();
    vi.unstubAllGlobals();
  });

  it('records in-app events and toasts them when toasts are on', () => {
    const app = fakeApp();
    expect(dispatch(app, { kind: 'portForwarded', title: 'Port 3000', quiet: true }, show)).toBe(true);
    expect(center.entries[0]).toMatchObject({ kind: 'portForwarded', read: true });
    expect(show).toHaveBeenCalledWith('Port 3000', 'info', undefined);
    settings.notifyBackgroundToasts = false;
    dispatch(app, { kind: 'portClosed', title: 'Port 3000 closed' }, show);
    expect(show).toHaveBeenCalledTimes(1);
  });

  it('drops events whose kind is off', () => {
    settings.notifyKinds = { ...DEFAULT_KINDS, errors: 'off' };
    expect(dispatch(fakeApp(), { kind: 'error', title: 'boom' }, show)).toBe(false);
    expect(center.entries).toHaveLength(0);
  });

  it('keeps muted sessions, workspaces and groups silent', () => {
    const app = fakeApp();
    settings.notifyMuted = { local: ['w:ws1'], 'ssh:other': ['g:g1'] };
    dispatch(app, { id: '1', kind: 'needsInput', title: 'a', action: { kind: 'line', key: 'line:a' } }, show);
    settings.notifyMuted = { local: ['w:ws1', 'g:g1'] };
    dispatch(app, { id: '2', kind: 'finished', title: 'b', action: { kind: 'line', key: 'line:b' } }, show);
    dispatch(app, { id: '3', kind: 'allIdle', title: 'idle', target: 'w:ws1' }, show);
    expect(center.entries.map((e) => [e.id, e.read])).toEqual([
      ['w:ws1', true],
      ['line:b', true],
      ['line:a', true],
    ]);
    expect(show).not.toHaveBeenCalled();
    expect(center.entries[0]).not.toHaveProperty('target');
    settings.notifyMuted = { 'ssh:other': ['line:c'] };
    dispatch(app, { id: '4', kind: 'finished', title: 'c', action: { kind: 'line', key: 'line:c' } }, show);
    expect(center.entries[0].read).toBe(false);
  });

  it('keeps one row per session, newest event first, unread again on each new event', () => {
    vi.useFakeTimers({ toFake: ['Date'] });
    const app = fakeApp();
    for (let i = 0; i < 3; i++) {
      vi.setSystemTime(i * 600_000);
      dispatch(app, { id: `t${i}`, kind: 'taskTesting', title: 'Task #116 is ready for testing', action: { kind: 'line', key: 'line:a' } }, show);
      dispatch(app, { id: `f${i}`, kind: 'finished', title: 'Claude Code finished in overseer', action: { kind: 'line', key: 'line:a' } }, show);
      dispatch(app, { id: `i${i}`, kind: 'allIdle', title: 'All agents in overseer are idle', target: 'w:ws1' }, show);
      center.markAllRead();
    }
    vi.setSystemTime(3 * 600_000);
    dispatch(app, { id: 'f9', kind: 'finished', title: 'Claude Code finished in overseer', action: { kind: 'line', key: 'line:a' } }, show);
    expect(center.entries.map((e) => [e.id, e.kind, e.read])).toEqual([
      ['line:a', 'finished', false],
      ['w:ws1', 'allIdle', true],
    ]);
    expect(center.unread).toBe(1);
    vi.useRealTimers();
  });

  it('folds a finished turn into the testing notice just before it, without a second toast', () => {
    settings.notifyKinds = { ...DEFAULT_KINDS, finished: 'app' };
    const app = fakeApp();
    dispatch(app, { id: 't', kind: 'taskTesting', title: 'Task #1 is ready for testing', action: { kind: 'line', key: 'line:a' } }, show);
    expect(dispatch(app, { id: 'f', kind: 'finished', title: 'Claude finished', action: { kind: 'line', key: 'line:a' } }, show)).toBe(false);
    expect(center.entries.map((e) => e.kind)).toEqual(['taskTesting']);
    expect(show).toHaveBeenCalledTimes(1);
  });

  it('lets a testing notice replace the finished turn just before it', () => {
    settings.notifyKinds = { ...DEFAULT_KINDS, finished: 'app' };
    const app = fakeApp();
    dispatch(app, { id: 'f', kind: 'finished', title: 'Claude finished', action: { kind: 'line', key: 'line:a' } }, show);
    expect(dispatch(app, { id: 't', kind: 'taskTesting', title: 'Task #1 is ready for testing', action: { kind: 'line', key: 'line:a' } }, show)).toBe(true);
    expect(center.entries.map((e) => e.kind)).toEqual(['taskTesting']);
    expect(show).toHaveBeenCalledTimes(2);
  });

  it('leaves the session on screen alone', () => {
    const app = fakeApp();
    app.selectedKey = 'line:a';
    focused = true;
    dispatch(app, { id: '1', kind: 'needsInput', title: 'a', action: { kind: 'line', key: 'line:a' } }, show);
    expect(center.entries[0].read).toBe(true);
    expect(show).not.toHaveBeenCalled();
  });

  it('falls back to a toast when the system cannot show it', async () => {
    dispatch(fakeApp(), { id: '1', kind: 'failed', title: 'x failed', action: { kind: 'line', key: 'line:x' } }, show);
    await vi.waitFor(() => expect(show).toHaveBeenCalledWith('x failed', 'error', undefined));
  });

  it('never toasts twice for errors that already show one', async () => {
    settings.notifyKinds = { ...DEFAULT_KINDS, errors: 'os' };
    dispatch(fakeApp(), { kind: 'error', title: 'boom', toast: false }, show);
    await Promise.resolve();
    expect(center.entries[0]).toMatchObject({ kind: 'error' });
    expect(show).not.toHaveBeenCalled();
  });
});

describe('cooldownGate', () => {
  it('rate-limits each slot on its own and can be turned off', () => {
    let t = 0;
    const pass = cooldownGate(() => t, 2);
    expect(pass('a', 20_000)).toBe(true);
    expect(pass('a', 20_000)).toBe(false);
    expect(pass('b', 20_000)).toBe(true);
    t = 20_000;
    expect(pass('a', 20_000)).toBe(true);
    expect(pass('a', 0)).toBe(true);
  });

  it('forgets the oldest slots beyond its bound', () => {
    const pass = cooldownGate(() => 0, 2);
    pass('a', 1000);
    pass('b', 1000);
    pass('c', 1000);
    expect(pass('a', 1000)).toBe(true);
    expect(pass('c', 1000)).toBe(false);
  });
});

type Tap = (e: unknown) => void;

function memoryStorage() {
  const data = new Map<string, string>();
  return { data, getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
}

class FakeNotification {
  static permission: 'default' | 'granted' | 'denied' = 'default';
  static requestPermission = vi.fn(async () => {
    FakeNotification.permission = 'granted';
    return 'granted';
  });
  static made: FakeNotification[] = [];
  onclick: (() => void) | null = null;
  constructor(
    public title: string,
    public opts: { body?: string; tag?: string; silent?: boolean },
  ) {
    FakeNotification.made.push(this);
  }
}

describe('system notifications', () => {
  let focused = true;
  let tap: Tap | null = null;
  let storage = memoryStorage();
  const listeners = new Map<string, () => void>();
  const unregister = vi.fn(async () => {});

  const tauri = () => vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
  const web = () => vi.stubGlobal('window', {});
  const appOn = (os: string) => Object.assign(fakeApp(), { os });
  const started = async (app: typeof App) => {
    const stop = startOs(app);
    await vi.waitFor(() => expect(plug.onAction).toHaveBeenCalled());
    return stop;
  };

  beforeEach(() => {
    focused = true;
    tap = null;
    storage = memoryStorage();
    listeners.clear();
    for (const f of [plug.isPermissionGranted, plug.requestPermission, plug.onAction, plug.createChannel, plug.removeActive, plug.sendNotification, win.unminimize, win.setFocus, invoke, unregister]) f.mockReset();
    plug.onAction.mockImplementation(async (fn: Tap) => {
      tap = fn;
      return { unregister };
    });
    plug.createChannel.mockResolvedValue(undefined);
    plug.removeActive.mockResolvedValue(undefined);
    FakeNotification.permission = 'default';
    FakeNotification.made = [];
    FakeNotification.requestPermission.mockClear();
    center.clear();
    vi.stubGlobal('document', { hasFocus: () => focused });
    vi.stubGlobal('localStorage', storage);
    vi.stubGlobal('addEventListener', (type: string, fn: () => void) => listeners.set(type, fn));
    vi.stubGlobal('removeEventListener', (type: string, fn: () => void) => listeners.get(type) === fn && listeners.delete(type));
    Object.assign(settings, {
      notifyKinds: { ...DEFAULT_KINDS },
      notifyQuiet: { ...DEFAULT_QUIET },
      notifyOnlyUnfocused: false,
      notifySkipWatched: true,
      notifyBackgroundToasts: true,
      notifyCooldown: 0,
      notifyMuted: {},
      notifyPreview: true,
      notifySound: true,
    });
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    vi.restoreAllMocks();
    startOs(fakeApp())();
  });

  describe('permission', () => {
    it('reports system-managed permission on desktop without asking the plugin', async () => {
      tauri();
      for (const os of ['macos', 'windows', 'linux']) expect(await permission(os)).toBe('system');
      expect(plug.isPermissionGranted).not.toHaveBeenCalled();
    });

    it('reads the mobile grant and remembers that it already asked', async () => {
      tauri();
      plug.isPermissionGranted.mockResolvedValue(null);
      expect(await permission('android')).toBe('prompt');
      storage.data.set('overseer.notifyAsked', '1');
      expect(await permission('android')).toBe('denied');
      plug.isPermissionGranted.mockResolvedValue(true);
      expect(await permission('android')).toBe('granted');
      plug.isPermissionGranted.mockRejectedValue(new Error('no plugin'));
      expect(await permission('ios')).toBe('denied');
    });

    it('maps the browser permission and treats a missing API as unsupported', async () => {
      web();
      expect(await permission('linux')).toBe('unsupported');
      vi.stubGlobal('Notification', FakeNotification);
      expect(await permission('linux')).toBe('prompt');
      FakeNotification.permission = 'denied';
      expect(await permission('linux')).toBe('denied');
    });

    it('asks a fresh mobile install once on its own and records it', async () => {
      tauri();
      plug.isPermissionGranted.mockResolvedValue(null);
      plug.requestPermission.mockResolvedValue('granted');
      expect(await requestPermission('android', false)).toBe('granted');
      expect(storage.data.get('overseer.notifyAsked')).toBe('1');
      plug.isPermissionGranted.mockResolvedValue(false);
      expect(await requestPermission('android', false)).toBe('denied');
      expect(plug.requestPermission).toHaveBeenCalledTimes(1);
    });

    it('asks again only when the user asks, and treats a failed request as denied', async () => {
      tauri();
      plug.isPermissionGranted.mockResolvedValue(false);
      plug.requestPermission.mockRejectedValue(new Error('boom'));
      expect(await requestPermission('android', true)).toBe('denied');
      plug.requestPermission.mockResolvedValue('granted');
      expect(await requestPermission('android', true)).toBe('granted');
      expect(plug.requestPermission).toHaveBeenCalledTimes(2);
    });

    it('asks the browser and reads the answer back', async () => {
      web();
      vi.stubGlobal('Notification', FakeNotification);
      expect(await requestPermission('linux', false)).toBe('granted');
      expect(FakeNotification.requestPermission).toHaveBeenCalledTimes(1);
      expect(await requestPermission('linux', true)).toBe('granted');
      expect(FakeNotification.requestPermission).toHaveBeenCalledTimes(1);
    });

    it('survives a storage that throws', async () => {
      tauri();
      vi.stubGlobal('localStorage', {
        getItem: () => {
          throw new Error('denied');
        },
        setItem: () => {
          throw new Error('denied');
        },
      });
      plug.isPermissionGranted.mockResolvedValue(null);
      plug.requestPermission.mockResolvedValue('denied');
      expect(await requestPermission('android', false)).toBe('denied');
    });
  });

  describe('sendOs in the desktop app', () => {
    const note = { kind: 'needsInput' as const, title: 'Claude needs input', body: 'repo · waiting', target: 'line:a' };

    it('replaces the previous Linux notification of the same tab and sends one with its route', async () => {
      tauri();
      const id = osId({ key: 'line:a', title: note.title });
      expect(await sendOs(appOn('linux'), note)).toBe(true);
      expect(plug.removeActive).toHaveBeenCalledWith([{ id }]);
      expect(plug.sendNotification).toHaveBeenCalledWith({
        id,
        title: note.title,
        body: note.body,
        extra: { key: 'line:a' },
        group: 'line:a',
        channelId: undefined,
        sound: 'message-new-instant',
        autoCancel: true,
      });
      expect(plug.removeActive.mock.invocationCallOrder[0]).toBeLessThan(plug.sendNotification.mock.invocationCallOrder[0]);
    });

    it('hides the body when previews are off and stays silent when sound is off', async () => {
      tauri();
      settings.notifyPreview = false;
      settings.notifySound = false;
      await sendOs(appOn('macos'), note);
      expect(plug.removeActive).not.toHaveBeenCalled();
      expect(plug.sendNotification.mock.calls[0][0]).toMatchObject({ body: undefined, sound: undefined });
    });

    it('sends untargeted notes without a route', async () => {
      tauri();
      await sendOs(appOn('windows'), { kind: 'errors', title: 'boom' });
      expect(plug.sendNotification.mock.calls[0][0]).toMatchObject({ id: osId({ key: null, title: 'boom' }), extra: undefined, group: undefined, sound: 'Default' });
    });

    it('creates the Android channels once and routes each kind to its channel', async () => {
      tauri();
      plug.isPermissionGranted.mockResolvedValue(true);
      const app = appOn('android');
      await sendOs(app, note);
      await sendOs(app, { kind: 'ports', title: 'Port 3000' });
      settings.notifySound = false;
      await sendOs(app, { kind: 'errors', title: 'boom' });
      expect(plug.createChannel).toHaveBeenCalledTimes(5);
      expect(plug.createChannel.mock.calls.map(([c]) => [c.id, c.vibration])).toEqual([
        ['agents', true],
        ['ports', true],
        ['updates', true],
        ['errors', true],
        ['quiet', false],
      ]);
      expect(plug.sendNotification.mock.calls.map(([n]) => n.channelId)).toEqual(['agents', 'ports', 'quiet']);
      expect(plug.removeActive).not.toHaveBeenCalled();
    });

    it('sends nothing without permission', async () => {
      tauri();
      plug.isPermissionGranted.mockResolvedValue(false);
      expect(await sendOs(appOn('android'), note)).toBe(false);
      expect(plug.sendNotification).not.toHaveBeenCalled();
      expect(plug.requestPermission).not.toHaveBeenCalled();
    });

    it('brings the window forward and opens the tab when the notification is tapped', async () => {
      tauri();
      const app = appOn('linux');
      await sendOs(app, note);
      const id = osId({ key: 'line:a', title: note.title });
      tap!({ actionId: 'tap', notification: { id } });
      await vi.waitFor(() => expect(app.select).toHaveBeenCalledWith('line:a'));
      expect(win.unminimize).toHaveBeenCalled();
      expect(win.setFocus).toHaveBeenCalled();
      expect(app.open).toHaveBeenCalledWith(null);
    });

    it('ignores taps on other actions or unknown notifications', async () => {
      tauri();
      const app = appOn('linux');
      await sendOs(app, note);
      tap!({ actionId: 'dismiss', notification: { id: osId({ key: 'line:a', title: '' }) } });
      tap!({ actionId: 'tap', notification: { id: 12345 } });
      await Promise.resolve();
      expect(app.select).not.toHaveBeenCalled();
    });

    it('registers the tap listener only once across sends', async () => {
      tauri();
      const app = appOn('linux');
      await sendOs(app, note);
      await sendOs(app, { ...note, target: 'line:b' });
      expect(plug.onAction).toHaveBeenCalledTimes(1);
    });
  });

  describe('focus fallback', () => {
    const note = { kind: 'finished' as const, title: 'done', target: 'line:f' };

    it('opens the tab when the window regains focus soon after a Linux notification', async () => {
      tauri();
      focused = false;
      const now = vi.spyOn(Date, 'now').mockReturnValue(1_000);
      const app = appOn('linux');
      const stop = await started(app);
      await sendOs(app, note);
      now.mockReturnValue(1_000 + FOCUS_FALLBACK_MS - 1);
      listeners.get('focus')!();
      expect(app.select).toHaveBeenCalledWith('line:f');
      listeners.get('focus')!();
      expect(app.select).toHaveBeenCalledTimes(1);
      stop();
      expect(listeners.has('focus')).toBe(false);
    });

    it('ignores a focus that comes too late', async () => {
      tauri();
      focused = false;
      const now = vi.spyOn(Date, 'now').mockReturnValue(1_000);
      const app = appOn('windows');
      await started(app);
      await sendOs(app, note);
      now.mockReturnValue(1_000 + FOCUS_FALLBACK_MS);
      listeners.get('focus')!();
      expect(app.select).not.toHaveBeenCalled();
    });

    it('does not arm the fallback when the window has focus or the OS routes clicks', async () => {
      tauri();
      const app = appOn('linux');
      await started(app);
      await sendOs(app, note);
      listeners.get('focus')!();
      focused = false;
      await sendOs(appOn('macos'), note);
      listeners.get('focus')!();
      expect(app.select).not.toHaveBeenCalled();
    });

    it('a real tap wins over the fallback so the tab is not opened twice', async () => {
      tauri();
      focused = false;
      const app = appOn('linux');
      await started(app);
      await sendOs(app, note);
      tap!({ notification: { id: osId({ key: 'line:f', title: 'done' }), extra: { key: 'line:f' } } });
      listeners.get('focus')!();
      await vi.waitFor(() => expect(app.select).toHaveBeenCalledTimes(1));
    });
  });

  describe('startOs', () => {
    it('unregisters the tap listener when stopped', async () => {
      tauri();
      const stop = startOs(fakeApp());
      await vi.waitFor(() => expect(plug.onAction).toHaveBeenCalled());
      await Promise.resolve();
      stop();
      expect(unregister).toHaveBeenCalledTimes(1);
    });

    it('still unregisters when stopped before the listener is ready', async () => {
      tauri();
      const stop = startOs(fakeApp());
      stop();
      await vi.waitFor(() => expect(unregister).toHaveBeenCalledTimes(1));
    });

    it('listens again after a stop that came before the listener was ready', async () => {
      tauri();
      startOs(fakeApp())();
      await vi.waitFor(() => expect(unregister).toHaveBeenCalledTimes(1));
      startOs(fakeApp());
      await vi.waitFor(() => expect(plug.onAction).toHaveBeenCalledTimes(2));
    });

    it('does not touch the plugin in a browser', () => {
      web();
      startOs(fakeApp())();
      expect(plug.onAction).not.toHaveBeenCalled();
    });
  });

  describe('in demo mode', () => {
    beforeEach(() => {
      tauri();
      storage.setItem(DEMO_KEY, '1');
    });

    it('reports system notifications as unsupported without asking the plugin', async () => {
      expect(await permission('android')).toBe('unsupported');
      expect(await requestPermission('android', true)).toBe('unsupported');
      expect(plug.isPermissionGranted).not.toHaveBeenCalled();
      expect(plug.requestPermission).not.toHaveBeenCalled();
    });

    it('never sends a system notification, so the app shows it in-app', async () => {
      expect(await sendOs(appOn('android'), { kind: 'needsInput', title: 'Needs input' }, true)).toBe(false);
      expect(plug.sendNotification).not.toHaveBeenCalled();
    });

    it('does not listen for notification taps', () => {
      startOs(fakeApp())();
      expect(plug.onAction).not.toHaveBeenCalled();
    });
  });

  describe('sendOs in a browser', () => {
    it('shows a browser notification that opens its tab on click', async () => {
      web();
      FakeNotification.permission = 'granted';
      vi.stubGlobal('Notification', FakeNotification);
      const focus = vi.fn();
      vi.stubGlobal('focus', focus);
      const app = fakeApp();
      expect(await sendOs(app, { kind: 'failed', title: 'build failed', body: 'exit 1', target: 'line:b' })).toBe(true);
      const [n] = FakeNotification.made;
      expect(n.title).toBe('build failed');
      expect(n.opts).toEqual({ body: 'exit 1', tag: 'line:b', silent: false });
      n.onclick!();
      await vi.waitFor(() => expect(app.select).toHaveBeenCalledWith('line:b'));
      expect(focus).toHaveBeenCalled();
    });

    it('returns false when the browser has no permission or no API', async () => {
      web();
      expect(await sendOs(fakeApp(), { kind: 'failed', title: 'x' })).toBe(false);
      FakeNotification.permission = 'denied';
      vi.stubGlobal('Notification', FakeNotification);
      expect(await sendOs(fakeApp(), { kind: 'failed', title: 'x' })).toBe(false);
      expect(FakeNotification.made).toEqual([]);
    });

    it('asks for permission on a user test and shows the test note', async () => {
      web();
      vi.stubGlobal('Notification', FakeNotification);
      settings.notifySound = false;
      expect(await sendTest(fakeApp())).toBe(true);
      expect(FakeNotification.requestPermission).toHaveBeenCalled();
      expect(FakeNotification.made[0].opts.silent).toBe(true);
      expect(FakeNotification.made[0].onclick).toBeNull();
    });
  });

  describe('dispatch to the system', () => {
    it('sends to the OS with the source in the body and no toast', async () => {
      tauri();
      const show = vi.fn();
      dispatch(appOn('linux'), { id: 'os-1', kind: 'needsInput', title: 'needs you', source: 'repo', body: 'question', action: { kind: 'line', key: 'line:os' } }, show);
      await vi.waitFor(() => expect(plug.sendNotification).toHaveBeenCalled());
      expect(plug.sendNotification.mock.calls[0][0]).toMatchObject({ title: 'needs you', body: 'repo · question', extra: { key: 'line:os' } });
      expect(show).not.toHaveBeenCalled();
    });

    it('rate-limits repeated OS notifications for the same tab', async () => {
      tauri();
      settings.notifyCooldown = 30;
      const app = appOn('linux');
      dispatch(app, { id: 'cd-1', kind: 'needsInput', title: 'a', action: { kind: 'line', key: 'line:cooldown' } }, vi.fn());
      dispatch(app, { id: 'cd-2', kind: 'needsInput', title: 'a again', action: { kind: 'line', key: 'line:cooldown' } }, vi.fn());
      await vi.waitFor(() => expect(plug.sendNotification).toHaveBeenCalled());
      await new Promise((r) => setTimeout(r, 0));
      expect(plug.sendNotification).toHaveBeenCalledTimes(1);
      expect(center.entries.map((e) => [e.id, e.title])).toEqual([['line:cooldown', 'a again']]);
    });

    it('toasts instead when sending fails', async () => {
      tauri();
      plug.sendNotification.mockImplementation(() => {
        throw new Error('dbus down');
      });
      const show = vi.fn();
      dispatch(appOn('linux'), { id: 'err-1', kind: 'failed', title: 'deploy failed', action: { kind: 'line', key: 'line:err' } }, show);
      await vi.waitFor(() => expect(show).toHaveBeenCalledWith('deploy failed', 'error', undefined));
    });
  });

  describe('muting', () => {
    it('mutes and unmutes a target for the current host only', () => {
      vi.stubGlobal('document', undefined);
      const local = fakeApp();
      const remote = Object.assign(fakeApp(), { conn: { target: { kind: 'ssh', host: 'box' } } });
      toggleTargetMute(remote, 'line:a');
      expect(isTargetMuted(remote, 'line:a')).toBe(true);
      expect(isTargetMuted(local, 'line:a')).toBe(false);
      expect(mutedList(remote)).toEqual(['line:a']);
      toggleTargetMute(remote, 'line:a');
      expect(settings.notifyMuted).toEqual({});
    });
  });

  it('opens the system notification settings', async () => {
    invoke.mockResolvedValue(undefined);
    await openSystemSettings();
    expect(invoke).toHaveBeenCalledWith('open_notification_settings');
  });
});
