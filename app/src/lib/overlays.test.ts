import { describe, expect, it } from 'vitest';
import { normalizeUrl } from './browser';
import { backoffMs, steps } from './conn';
import { fuzzy, highlight, rank } from './fuzzy';
import { needsInputCount, notificationsFor } from './notify';
import type { Snapshot } from './reduce';
import type { ConnState, LocalLine, Task, Term } from './types';

describe('fuzzy', () => {
  it('matches subsequences and merges adjacent ranges', () => {
    expect(fuzzy('nwcl', 'New Claude session')!.ranges).toEqual([[0, 1], [2, 3], [4, 6]]);
    expect(fuzzy('xyz', 'New Claude session')).toBeNull();
    expect(fuzzy('', 'anything')).toEqual({ score: 0, ranges: [] });
  });

  it('ranks contiguous and boundary matches first', () => {
    const r = rank('sh', ['Push branch', 'New shell', 'Show settings'], (x) => x);
    expect(r.map((x) => x.item)).toEqual(['Show settings', 'New shell', 'Push branch']);
    expect(rank('', ['b', 'a'], (x) => x).map((x) => x.item)).toEqual(['b', 'a']);
  });

  it('splits text into highlighted parts', () => {
    expect(highlight('abcd', [[1, 3]])).toEqual([
      { text: 'a', hit: false },
      { text: 'bc', hit: true },
      { text: 'd', hit: false },
    ]);
  });
});

describe('conn steps', () => {
  const target = { kind: 'ssh' as const, host: 'h', label: 'h' };
  const st = (phase: ConnState['phase']): ConnState => ({ phase, target });
  const status = (s: ReturnType<typeof steps>) => s.map((x) => x.status);

  it('marks progress while connecting', () => {
    expect(status(steps(st('checking'), ['connecting', 'checking']))).toEqual(['done', 'active', 'pending', 'pending', 'pending']);
    expect(status(steps(st('starting'), ['connecting', 'checking', 'starting']))).toEqual(['done', 'done', 'skipped', 'active', 'pending']);
  });

  it('skips install when the server was up to date', () => {
    expect(status(steps(st('connected'), ['connecting', 'checking', 'starting', 'connected']))).toEqual(['done', 'done', 'skipped', 'done', 'done']);
    expect(status(steps(st('connected'), ['installing', 'connected']))).toEqual(['done', 'done', 'done', 'done', 'done']);
  });

  it('fails the furthest step on error', () => {
    expect(status(steps(st('error'), ['connecting', 'checking', 'installing', 'error']))).toEqual(['done', 'done', 'failed', 'pending', 'pending']);
    expect(status(steps(st('error'), ['error']))).toEqual(['failed', 'pending', 'pending', 'pending', 'pending']);
  });

  it('shows the server download only when the phone downloads it', () => {
    expect(steps(st('downloading'), ['connecting', 'checking', 'downloading']).map((s) => s.id)).toEqual(['connecting', 'checking', 'downloading', 'installing', 'starting', 'connected']);
    expect(status(steps(st('installing'), ['connecting', 'checking', 'downloading', 'installing']))).toEqual(['done', 'done', 'done', 'active', 'pending', 'pending']);
    expect(steps(st('connected'), ['checking', 'starting', 'connected']).map((s) => s.id)).not.toContain('downloading');
  });

  it('shows the last reached step as active while reconnecting', () => {
    expect(status(steps(st('reconnecting'), ['connecting', 'reconnecting']))).toEqual(['active', 'pending', 'pending', 'pending', 'pending']);
  });

  it('backs off exponentially with jitter and a cap', () => {
    expect(backoffMs(0, () => 0)).toBe(250);
    expect(backoffMs(0, () => 1)).toBe(500);
    expect(backoffMs(3, () => 1)).toBe(4000);
    expect(backoffMs(20, () => 1)).toBe(30000);
    expect(backoffMs(20, () => 0)).toBe(15000);
  });
});

describe('normalizeUrl', () => {
  it('adds schemes sensibly', () => {
    expect(normalizeUrl('github.com/acme')).toBe('https://github.com/acme');
    expect(normalizeUrl('localhost:5173')).toBe('http://localhost:5173');
    expect(normalizeUrl('127.0.0.1:8080/x')).toBe('http://127.0.0.1:8080/x');
    expect(normalizeUrl('devbox:3000')).toBe('http://devbox:3000');
    expect(normalizeUrl('http://a.b')).toBe('http://a.b');
    expect(normalizeUrl(' ')).toBe('');
    expect(normalizeUrl('rust traits')).toContain('duckduckgo.com/?q=rust%20traits');
  });
});

describe('notificationsFor', () => {
  const line = (id: string, needsInput: boolean): LocalLine => ({
    line: { id, agent: 'claude', state: 'active', lastActivityAt: '' },
    local: { needsInput, title: `T ${id}` },
  });
  const snap = (p: Partial<Snapshot>): Snapshot => ({ lines: [], terms: [], tasks: [], activity: {}, ...p });
  const all = { notifyNeedsInput: true, notifyFinished: true, notifyFailed: true };

  it('notices new needs-input lines only', () => {
    const prev = snap({ lines: [line('a', true), line('b', false)] });
    const next = snap({ lines: [line('a', true), line('b', true)] });
    const n = notificationsFor(prev, next, all);
    expect(n).toMatchObject([{ kind: 'needsInput', key: 'line:b' }]);
    expect(n[0].title).toBe('Claude Code needs your input');
    expect(n[0].body).toContain('T b');
    expect(notificationsFor(prev, next, { ...all, notifyNeedsInput: false })).toEqual([]);
    expect(needsInputCount(next)).toBe(2);
  });

  it('notices tasks reaching testing', () => {
    const task = { id: 't', number: 7, title: 'X', status: 'in_progress', lineId: 'a' } as Task;
    const n = notificationsFor(snap({ tasks: [task] }), snap({ tasks: [{ ...task, status: 'in_testing' }] }), all);
    expect(n).toMatchObject([{ kind: 'taskTesting', key: 'line:a' }]);
    expect(n[0].title).toContain('7');
  });

  it('notices finished and failed terms of lines or tasks', () => {
    const term = (id: string, extra: Partial<Term>) => ({ id, title: id, cmd: [], exited: false, ...extra }) as Term;
    const prev = snap({ terms: [term('a', { lineId: 'l' }), term('b', { taskId: 't' }), term('c', {})] });
    const next = snap({
      terms: [term('a', { lineId: 'l', exited: true, exitCode: 0 }), term('b', { taskId: 't', exited: true, exitCode: 1 }), term('c', { exited: true, exitCode: 0 })],
    });
    expect(notificationsFor(prev, next, all).map((n) => [n.kind, n.key])).toEqual([
      ['finished', 'line:l'],
      ['failed', 'term:b'],
    ]);
    expect(notificationsFor(prev, next, { ...all, notifyFailed: false }).map((n) => n.kind)).toEqual(['finished']);
  });
});
