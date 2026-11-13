import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { center } from './notifications.svelte';
import { DEFAULT_KINDS, DEFAULT_QUIET } from './notify-prefs';
import { cooldownGate, dispatch, isTargetMuted, mutedList, toggleTargetMute } from './os-notify';
import { settings } from '../settings/settings.svelte';
import type { app as App } from '../state.svelte';

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
