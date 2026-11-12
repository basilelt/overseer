import { describe, expect, it } from 'vitest';
import {
  channelFor,
  DEFAULT_KINDS,
  DEFAULT_QUIET,
  inQuietHours,
  isMuted,
  kindOf,
  mayAsk,
  migrateKinds,
  NOTIFY_KINDS,
  opensSettings,
  permOf,
  route,
  soundFor,
  toggleMute,
  type Delivery,
  type NotifyPrefs,
  type RouteCtx,
} from './notify-prefs';

const at = (day: number, h: number, m = 0) => new Date(2024, 0, 7 + day, h, m);
const prefs = (p: Partial<NotifyPrefs> = {}): NotifyPrefs => ({
  notifyKinds: { ...DEFAULT_KINDS },
  notifyQuiet: { ...DEFAULT_QUIET },
  notifyOnlyUnfocused: false,
  notifySkipWatched: true,
  notifyMuted: {},
  ...p,
});
const ctx = (c: Partial<RouteCtx> = {}): RouteCtx => ({ focused: false, watching: false, muted: false, now: at(1, 12), ...c });

describe('route', () => {
  const deliveries: Delivery[] = ['os', 'app', 'off'];

  it('follows the delivery of each kind', () => {
    for (const k of NOTIFY_KINDS)
      for (const d of deliveries) {
        const r = route(k, prefs({ notifyKinds: { ...DEFAULT_KINDS, [k]: d } }), ctx());
        expect(r, `${k}/${d}`).toBe(d);
      }
  });

  it('keeps muted and watched events silent in the center', () => {
    expect(route('needsInput', prefs(), ctx({ muted: true }))).toBe('silent');
    expect(route('needsInput', prefs(), ctx({ focused: true, watching: true }))).toBe('silent');
    expect(route('needsInput', prefs({ notifySkipWatched: false }), ctx({ focused: true, watching: true }))).toBe('os');
    expect(route('ports', prefs(), ctx({ muted: true }))).toBe('silent');
    expect(route('needsInput', prefs({ notifyKinds: { ...DEFAULT_KINDS, needsInput: 'off' } }), ctx({ muted: true }))).toBe('off');
  });

  it('stays in the app while focused when asked to', () => {
    expect(route('finished', prefs({ notifyOnlyUnfocused: true }), ctx({ focused: true }))).toBe('app');
    expect(route('finished', prefs({ notifyOnlyUnfocused: true }), ctx({ focused: false }))).toBe('os');
    expect(route('finished', prefs(), ctx({ focused: true }))).toBe('os');
  });

  it('downgrades to in-app during quiet hours', () => {
    const quiet = prefs({ notifyQuiet: { on: true, start: '22:00', end: '07:00', days: [1, 2, 3, 4, 5] } });
    expect(route('failed', quiet, ctx({ now: at(1, 23) }))).toBe('app');
    expect(route('failed', quiet, ctx({ now: at(1, 12) }))).toBe('os');
    expect(route('failed', quiet, ctx({ now: at(6, 23) }))).toBe('os');
  });
});

describe('quiet hours', () => {
  it('handles a same-day window', () => {
    const q = { on: true, start: '12:00', end: '14:00', days: [3] };
    expect(inQuietHours(q, at(3, 12))).toBe(true);
    expect(inQuietHours(q, at(3, 13, 59))).toBe(true);
    expect(inQuietHours(q, at(3, 14))).toBe(false);
    expect(inQuietHours(q, at(4, 13))).toBe(false);
  });

  it('wraps past midnight on the day the window started', () => {
    const q = { on: true, start: '22:00', end: '07:00', days: [5] };
    expect(inQuietHours(q, at(5, 22, 30))).toBe(true);
    expect(inQuietHours(q, at(6, 6, 59))).toBe(true);
    expect(inQuietHours(q, at(6, 7))).toBe(false);
    expect(inQuietHours(q, at(5, 6))).toBe(false);
    expect(inQuietHours(q, at(0, 1))).toBe(false);
  });

  it('is off when disabled, without days, and whole-day when start equals end', () => {
    expect(inQuietHours({ on: false, start: '00:00', end: '23:59', days: [0, 1, 2, 3, 4, 5, 6] }, at(1, 12))).toBe(false);
    expect(inQuietHours({ on: true, start: '22:00', end: '07:00', days: [] }, at(1, 23))).toBe(false);
    expect(inQuietHours({ on: true, start: '09:00', end: '09:00', days: [0] }, at(0, 3))).toBe(true);
  });
});

describe('mutes', () => {
  const m = { workspaceOf: (k: string) => (k === 'line:a' ? 'ws1' : undefined), groupsOf: (k: string) => (k === 'line:b' ? ['g1'] : []) };

  it('mutes a session directly, through its workspace or through a group', () => {
    expect(isMuted(['line:a'], 'line:a', m)).toBe(true);
    expect(isMuted(['w:ws1'], 'line:a', m)).toBe(true);
    expect(isMuted(['g:g1'], 'line:b', m)).toBe(true);
    expect(isMuted(['w:ws1'], 'line:b', m)).toBe(false);
    expect(isMuted(['w:ws1'], 'w:ws1', m)).toBe(true);
    expect(isMuted(['w:ws1'], undefined, m)).toBe(false);
    expect(isMuted(undefined, 'line:a', m)).toBe(false);
  });

  it('toggles per host and drops empty hosts', () => {
    const a = toggleMute({}, 'ssh:dev', 'line:a');
    expect(a).toEqual({ 'ssh:dev': ['line:a'] });
    expect(toggleMute(a, 'local', 'w:x')).toEqual({ 'ssh:dev': ['line:a'], local: ['w:x'] });
    expect(toggleMute(a, 'ssh:dev', 'line:a')).toEqual({});
  });
});

describe('settings migration', () => {
  it('turns the old switches into deliveries and keeps stored ones', () => {
    expect(migrateKinds({})).toEqual(DEFAULT_KINDS);
    expect(migrateKinds({ notifyNeedsInput: false, notifyFinished: true, notifyFailed: false })).toMatchObject({ needsInput: 'off', finished: 'os', failed: 'off' });
    expect(migrateKinds({ notifyNeedsInput: false, notifyKinds: { needsInput: 'app', ports: 'os', bogus: 'os', errors: 'loud' } })).toMatchObject({ needsInput: 'app', ports: 'os', errors: 'app' });
  });
});

describe('entry kinds and Android channels', () => {
  it('maps every center entry to a board kind', () => {
    expect(kindOf('taskTesting')).toBe('finished');
    expect(kindOf('portForwarded')).toBe('ports');
    expect(kindOf('portClosed')).toBe('ports');
    expect(kindOf('update')).toBe('updates');
    expect(kindOf('upload')).toBe('uploads');
    expect(kindOf('error')).toBe('errors');
  });

  it('picks one of the four channels, or the silent one without sound', () => {
    expect(channelFor('needsInput', true)).toBe('agents');
    expect(channelFor('allIdle', true)).toBe('agents');
    expect(channelFor('failed', true)).toBe('agents');
    expect(channelFor('ports', true)).toBe('ports');
    expect(channelFor('updates', true)).toBe('updates');
    expect(channelFor('uploads', true)).toBe('updates');
    expect(channelFor('errors', true)).toBe('errors');
    for (const k of NOTIFY_KINDS) expect(channelFor(k, false)).toBe('quiet');
  });

  it('uses a sound name each platform understands', () => {
    expect(soundFor('windows', true)).toBe('Default');
    expect(soundFor('macos', true)).toBe('Ping');
    expect(soundFor('linux', true)).toBe('message-new-instant');
    expect(soundFor('android', true)).toBeUndefined();
    for (const os of ['windows', 'macos', 'linux', 'ios', 'android']) expect(soundFor(os, false)).toBeUndefined();
  });
});

describe('permission', () => {
  it('leaves desktop permission to the system', () => {
    for (const os of ['macos', 'windows', 'linux']) expect(permOf({ tauri: true, os, granted: true, asked: false })).toBe('system');
  });

  it('asks once on mobile, then only from the board', () => {
    const fresh = permOf({ tauri: true, os: 'android', granted: null, asked: false });
    expect(fresh).toBe('prompt');
    expect(mayAsk(fresh, false)).toBe(true);
    const refused = permOf({ tauri: true, os: 'android', granted: null, asked: true });
    expect(refused).toBe('denied');
    expect(mayAsk(refused, false)).toBe(false);
    expect(mayAsk(refused, true)).toBe(true);
    expect(permOf({ tauri: true, os: 'ios', granted: false, asked: false })).toBe('denied');
    expect(permOf({ tauri: true, os: 'ios', granted: true, asked: true })).toBe('granted');
    expect(mayAsk('granted', true)).toBe(false);
    expect(mayAsk('system', true)).toBe(false);
  });

  it('reads the browser permission', () => {
    expect(permOf({ tauri: false, os: '', web: 'default', asked: false })).toBe('prompt');
    expect(permOf({ tauri: false, os: '', web: 'denied', asked: false })).toBe('denied');
    expect(permOf({ tauri: false, os: '', asked: false })).toBe('unsupported');
  });

  it('offers the system settings where there is a page to open', () => {
    expect(opensSettings(true, 'macos')).toBe(true);
    expect(opensSettings(true, 'windows')).toBe(true);
    expect(opensSettings(true, 'android')).toBe(true);
    expect(opensSettings(true, 'linux')).toBe(false);
    expect(opensSettings(true, 'ios')).toBe(false);
    expect(opensSettings(false, 'macos')).toBe(false);
  });
});
