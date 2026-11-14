import { beforeEach, describe, expect, it, vi } from 'vitest';
import { applySettings, canImport, countKeys, DISMISS_KEY, dismissForever, isDismissed, shouldOffer } from './switchboard';
import type { ConnState, SwitchboardStatus } from './types';

const memory = () => {
  const m = new Map<string, string>();
  return { getItem: (k: string) => m.get(k) ?? null, setItem: (k: string, v: string) => void m.set(k, v), removeItem: (k: string) => void m.delete(k), clear: () => m.clear() };
};

const conn = (phase: ConnState['phase'] = 'connected'): ConnState => ({ phase, target: { kind: 'ssh', host: 'box', label: 'box' } }) as ConnState;
const status = (o: Partial<SwitchboardStatus> = {}): SwitchboardStatus => ({ found: true, dir: '/d', imported: false, running: false, sessions: 1, tasks: 1, workspaces: 1, hooks: false, settings: {}, ...o });

beforeEach(() => {
  vi.stubGlobal('localStorage', memory());
});

describe('shouldOffer', () => {
  it('offers found data that is not imported or dismissed', () => {
    expect(shouldOffer(status(), conn(), false, [])).toBe(true);
  });

  it.each([
    ['not found', status({ found: false }), conn(), false, []],
    ['already imported', status({ imported: true }), conn(), false, []],
    ['dismissed', status(), conn(), true, []],
    ['already offered for the host', status(), conn(), false, ['ssh:box']],
    ['not connected', status(), conn('connecting'), false, []],
    ['no status', null, conn(), false, []],
  ])('does not offer when %s', (_n, s, c, d, o) => {
    expect(shouldOffer(s as SwitchboardStatus | null, c as ConnState, d as boolean, o as string[])).toBe(false);
  });
});

describe('dismissal and settings', () => {
  it('persists the dismissal in localStorage', () => {
    expect(isDismissed()).toBe(false);
    dismissForever();
    expect(localStorage.getItem(DISMISS_KEY)).toBe('dismissed');
    expect(isDismissed()).toBe(true);
  });

  it('survives a broken localStorage', () => {
    const blocked = () => {
      throw new Error('blocked');
    };
    vi.stubGlobal('localStorage', { getItem: blocked, setItem: blocked });
    expect(isDismissed()).toBe(false);
    expect(() => dismissForever()).not.toThrow();
    expect(() => applySettings({ 'overseer.a': '1' })).not.toThrow();
  });

  it('overwrites existing entries and ignores a missing map', () => {
    localStorage.setItem('overseer.theme', 'light');
    applySettings({ 'overseer.theme': 'dark', 'overseer:x': '2' });
    expect(localStorage.getItem('overseer.theme')).toBe('dark');
    expect(localStorage.getItem('overseer:x')).toBe('2');
    expect(() => applySettings(undefined)).not.toThrow();
  });
});

describe('countKeys', () => {
  const tr = (key: string, vars: { n: number }) => `${key}:${vars.n}`;

  it('uses the singular key for a count of one', () => {
    expect(countKeys({ sessions: 1, tasks: 1, workspaces: 1 }, tr)).toEqual({
      sessions: 'switchboard.sessionsOne:1',
      tasks: 'switchboard.tasksOne:1',
      workspaces: 'switchboard.workspacesOne:1',
    });
  });

  it('uses the plural key for zero and many', () => {
    expect(countKeys({ sessions: 0, tasks: 2, workspaces: 12 }, tr)).toEqual({
      sessions: 'switchboard.sessions:0',
      tasks: 'switchboard.tasks:2',
      workspaces: 'switchboard.workspaces:12',
    });
  });
});

describe('canImport', () => {
  it('offers the import while old data exists and is not imported', () => {
    expect(canImport(status())).toBe(true);
  });

  it('hides the import once imported, without data or without a status', () => {
    expect(canImport(status({ imported: true }))).toBe(false);
    expect(canImport(status({ found: false }))).toBe(false);
    expect(canImport(null)).toBe(false);
  });
});
