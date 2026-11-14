import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { changes, folderName, idleTransitions, needsInputCount, notificationsFor, notifyGate, placeIn, prefsOf, startNotifications, stillDue, type KV, type Notice } from './notify';
import type { SidebarItem } from '../session/grouping';
import { settings } from '../settings/settings.svelte';
import type { app as App } from '../state.svelte';
import { osId, routeOf } from './os-send';
import type { Snapshot } from '../reduce';
import type { DaemonEvent, LocalLine, Task, Term } from '../types';
import { stateOfGroup } from '../../test/fixtures';

const onAction = vi.hoisted(() => vi.fn());
const unregister = vi.hoisted(() => vi.fn(async () => {}));
vi.mock('@tauri-apps/plugin-notification', () => ({ onAction }));

const line = (id: string, agent: string, local: LocalLine['local'], state: LocalLine['line']['state'] = 'active'): LocalLine => ({
  line: { id, agent, state, lastActivityAt: '', worktreePath: `/home/u/work/${id}-repo` },
  local: { title: `T ${id}`, ...local },
});
const snap = (lines: LocalLine[]): Snapshot => ({ lines, terms: [], tasks: [], activity: {} });
const all = { notifyNeedsInput: true, notifyFinished: true, notifyFailed: true };

describe('agent notifications', () => {
  it('announces a finished turn with the agent and workspace', () => {
    const prev = snap([line('a', 'opencode', { busy: true }), line('b', 'claude', { busy: false }), line('c', 'amp', { busy: true })]);
    const next = snap([line('a', 'opencode', { busy: false }), line('b', 'claude', { busy: false }), line('c', 'amp', { busy: true })]);
    const n = notificationsFor(prev, next, all, () => 'webshop');
    expect(n).toEqual([{ kind: 'finished', key: 'line:a', title: 'opencode finished in webshop', body: 'T a' }]);
    expect(notificationsFor(prev, next, { ...all, notifyFinished: false }, () => 'webshop')).toEqual([]);
  });

  it('falls back to the folder name and skips ended or waiting lines', () => {
    const prev = snap([line('a', 'gemini', { busy: true }), line('b', 'codex', { busy: true }), line('c', 'cursor', { busy: true })]);
    const next = snap([line('a', 'gemini', { busy: false }), line('b', 'codex', { busy: false, needsInput: true }), line('c', 'cursor', { busy: false }, 'held')]);
    const n = notificationsFor(prev, next, { ...all, notifyNeedsInput: false });
    expect(n.map((x) => [x.key, x.title])).toEqual([['line:a', 'Gemini CLI finished in a-repo']]);
    expect(folderName(line('x', 'claude', {}))).toBe('x-repo');
    expect(folderName({ line: { id: 'y', agent: 'claude', state: 'idle', lastActivityAt: '', worktreePath: 'unknown' } })).toBeUndefined();
  });

  it('names a renamed session by its custom title', () => {
    const prev = snap([line('a', 'claude', { busy: true, titleOverride: 'Payments' })]);
    const next = snap([line('a', 'claude', { busy: false, titleOverride: 'Payments' })]);
    expect(notificationsFor(prev, next, all, () => 'webshop')[0].body).toBe('Payments');
    expect(notificationsFor(prev, next, all, () => 'Payments')[0].body).toBe('');
  });

  it('says which agent needs input', () => {
    const prev = snap([line('a', 'copilot', { needsInput: false })]);
    const next = snap([line('a', 'copilot', { needsInput: true })]);
    expect(notificationsFor(prev, next, all, () => 'web')).toMatchObject([{ kind: 'needsInput', key: 'line:a', title: 'Copilot CLI needs your input', body: 'T a · web' }]);
  });
});

describe('incremental notifications', () => {
  const big = (n: number) => Array.from({ length: n }, (_, i) => line(`l${i}`, 'claude', { busy: i % 2 === 0, needsInput: false }));

  it('only inspects the lines whose object changed', () => {
    const lines = big(300);
    const prev = snap(lines);
    const moved = lines.slice();
    moved[42] = line('l42', 'claude', { busy: false });
    const placeOf = vi.fn(() => 'ws');
    const n = notificationsFor(prev, snap(moved), all, placeOf);
    expect(n.map((x) => x.key)).toEqual(['line:l42']);
    expect(placeOf).toHaveBeenCalledTimes(1);
    expect(changes(lines, moved, (l) => l.line.id).map((c) => c.item.line.id)).toEqual(['l42']);
    expect(changes(lines, lines, (l) => l.line.id)).toEqual([]);
  });

  it('matches a full comparison for random edits, inserts and removals', () => {
    const naive = (prev: Snapshot, next: Snapshot) => {
      const out: string[] = [];
      const need = new Set(prev.lines.filter((l) => l.local?.needsInput).map((l) => l.line.id));
      for (const l of next.lines) if (l.local?.needsInput && !need.has(l.line.id) && l.line.state !== 'held' && l.line.state !== 'hung-up') out.push(`needsInput:${l.line.id}`);
      const busy = new Set(prev.lines.filter((l) => l.local?.busy).map((l) => l.line.id));
      for (const l of next.lines) if (busy.has(l.line.id) && l.local?.busy === false && !l.local?.needsInput && l.line.state !== 'held' && l.line.state !== 'hung-up') out.push(`finished:${l.line.id}`);
      const st = new Map(prev.tasks.map((x) => [x.id, x.status]));
      for (const x of next.tasks) if (x.status === 'in_testing' && st.get(x.id) !== 'in_testing') out.push(`taskTesting:${x.id}`);
      const ex = new Set(prev.terms.filter((x) => x.exited).map((x) => x.id));
      for (const x of next.terms) if (x.exited && !ex.has(x.id) && (x.lineId || x.taskId)) out.push(`${(x.exitCode ?? 0) === 0 ? 'finished' : 'failed'}:${x.id}`);
      return out.sort();
    };
    let seed = 7;
    const rnd = (n: number) => ((seed = (seed * 1103515245 + 12345) % 2 ** 31), seed % n);
    const task = (i: number, status: Task['status']) => ({ id: `t${i}`, number: i, title: `t${i}`, status, lineId: null }) as unknown as Task;
    const term = (i: number, exited: boolean) => ({ id: `x${i}`, lineId: `l${i}`, exited, exitCode: i % 3, cmd: ['sh'], title: `x${i}` }) as unknown as Term;
    let cur: Snapshot = { lines: big(40), tasks: Array.from({ length: 20 }, (_, i) => task(i, 'in_progress')), terms: Array.from({ length: 10 }, (_, i) => term(i, false)), activity: {} };
    for (let round = 0; round < 300; round++) {
      const next: Snapshot = { ...cur, lines: cur.lines.slice(), tasks: cur.tasks.slice(), terms: cur.terms.slice() };
      const op = rnd(6);
      if (op === 0 && next.lines.length) {
        const i = rnd(next.lines.length);
        next.lines[i] = line(next.lines[i].line.id, 'claude', { busy: rnd(2) === 0, needsInput: rnd(3) === 0 });
      }
      else if (op === 1) next.lines.splice(rnd(next.lines.length), 1);
      else if (op === 2) next.lines.push(line(`n${round}`, 'claude', { busy: false, needsInput: rnd(2) === 0 }));
      else if (op === 3) {
        const i = rnd(next.tasks.length);
        next.tasks[i] = task(i, rnd(2) ? 'in_testing' : 'in_progress');
      } else if (op === 4) {
        const i = rnd(next.terms.length);
        next.terms[i] = term(i, rnd(2) === 0);
      }
      else next.lines.reverse();
      const got = notificationsFor(cur, next, all).map((n) => `${n.kind}:${n.key?.replace(/^(line|term):/, '') ?? ''}`);
      const want = naive(cur, next).map((k) => (k.startsWith('taskTesting') ? 'taskTesting:' : k.replace(/^(finished|failed):x(\d+)$/, (_m, kind, i) => `${kind}:l${i}`)));
      expect(got.sort(), `round ${round}`).toEqual(want.sort());
      cur = next;
    }
  });
});

describe('notification clicks', () => {
  const sent = new Map([[3, 'line:b']]);

  it('routes a tap to the tab of the notification', () => {
    expect(routeOf({ actionId: 'tap', notification: { id: 9, extra: { key: 'line:a' } } }, sent)).toBe('line:a');
    expect(routeOf({ actionId: 'tap', notification: { id: 3, extra: {} } }, sent)).toBe('line:b');
    expect(routeOf({ notification: { id: 3 } }, sent)).toBe('line:b');
  });

  it('ignores other actions and unknown notifications', () => {
    expect(routeOf({ actionId: 'snooze', notification: { id: 3 } }, sent)).toBeNull();
    expect(routeOf({ actionId: 'tap', notification: { id: 4 } }, sent)).toBeNull();
    expect(routeOf({ actionId: 'tap', notification: null }, sent)).toBeNull();
  });
});

describe('notification gate', () => {
  const ask = (id: string): Notice => ({ kind: 'needsInput', key: `line:${id}`, title: `${id} needs input`, body: '' });
  const store = (): KV & { data: Map<string, string> } => {
    const data = new Map<string, string>();
    return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v) };
  };
  let clock = 0;
  const now = () => clock;
  const tick = (ms: number) => {
    clock += ms;
    vi.advanceTimersByTime(ms);
  };

  beforeEach(() => {
    vi.useFakeTimers();
    clock = 0;
  });
  afterEach(() => vi.useRealTimers());

  it('never re-shows a notification replayed after a reconnect', () => {
    const sent: Notice[] = [];
    const ids: string[] = [];
    const gate = notifyGate((n, id) => (sent.push(n), ids.push(id)), () => true, store(), now);
    gate.push(ask('a'), '2026-10-30T10:00:00.000Z');
    tick(2000);
    gate.push(ask('a'), '2026-10-30T10:00:00.000Z');
    tick(60_000);
    gate.push(ask('a'), '2026-10-30T10:00:00.000Z');
    tick(2000);
    expect(sent).toHaveLength(1);
    expect(ids).toEqual([`needsInput|${ask('a').key}|2026-10-30T10:00:00.000Z`]);
  });

  it('remembers delivered notifications across a reload', () => {
    const kv = store();
    const sent: Notice[] = [];
    const first = notifyGate((n) => sent.push(n), () => true, kv, now);
    first.push(ask('a'), 't1');
    tick(2000);
    first.stop();
    const second = notifyGate((n) => sent.push(n), () => true, kv, now);
    second.push(ask('a'), 't1');
    tick(60_000);
    expect(sent).toHaveLength(1);
    second.push(ask('a'), 't2');
    tick(2000);
    expect(sent).toHaveLength(2);
  });

  it('coalesces a burst into one notification per line', () => {
    const sent: Notice[] = [];
    const gate = notifyGate((n) => sent.push(n), () => true, store(), now);
    for (let i = 0; i < 50; i++) {
      gate.push(ask('a'), `a${i}`);
      gate.push(ask('b'), `b${i}`);
      tick(10);
    }
    tick(2000);
    expect(sent.map((n) => n.key)).toEqual(['line:a', 'line:b']);
  });

  it('drops a notification whose condition no longer holds when it settles', () => {
    const sent: Notice[] = [];
    let waiting = true;
    const gate = notifyGate((n) => sent.push(n), () => waiting, store(), now);
    gate.push(ask('a'), 't1');
    waiting = false;
    tick(2000);
    expect(sent).toEqual([]);
  });

  it('rate-limits a flapping line', () => {
    const sent: Notice[] = [];
    const gate = notifyGate((n) => sent.push(n), () => true, store(), now);
    for (let i = 0; i < 5; i++) {
      gate.push(ask('a'), `t${i}`);
      tick(3000);
    }
    expect(sent).toHaveLength(1);
    tick(20_000);
    gate.push(ask('a'), 'later');
    tick(2000);
    expect(sent).toHaveLength(2);
  });

  it('survives a broken store and bounds what it keeps', () => {
    const bad: KV = { getItem: () => '{oops', setItem: () => { throw new Error('full'); } };
    const sent: Notice[] = [];
    const gate = notifyGate((n) => sent.push(n), () => true, bad, now);
    gate.push(ask('a'), 't1');
    tick(2000);
    expect(sent).toHaveLength(1);
    const kv = store();
    const many = notifyGate(() => {}, () => true, kv, now);
    for (let i = 0; i < 600; i++) {
      many.push(ask(`l${i}`), 't');
      tick(1600);
    }
    expect(JSON.parse(kv.data.get('overseer.notified')!)).toHaveLength(500);
  });

  it('checks the live line state before showing a notification', () => {
    const s = (local: LocalLine['local'], state: LocalLine['line']['state'] = 'active') => snap([line('a', 'claude', local, state)]);
    expect(stillDue(s({ needsInput: true }), ask('a'))).toBe(true);
    expect(stillDue(s({ needsInput: false }), ask('a'))).toBe(false);
    expect(stillDue(s({ needsInput: true }, 'hung-up'), ask('a'))).toBe(false);
    const done: Notice = { kind: 'finished', key: 'line:a', title: 'done', body: '' };
    expect(stillDue(s({ busy: false }), done)).toBe(true);
    expect(stillDue(s({ busy: true }), done)).toBe(false);
    expect(stillDue(s({}), { ...done, kind: 'failed' })).toBe(true);
  });

  it('reuses one OS notification id per line so a new one replaces the old', () => {
    expect(osId(ask('a'))).toBe(osId({ key: 'line:a', title: 'other' }));
    expect(osId(ask('a'))).not.toBe(osId(ask('b')));
    expect(osId({ key: null, title: 'x' })).toBeGreaterThan(0);
  });
});

describe('notification helpers', () => {
  it('turns kind settings into on/off prefs', () => {
    expect(prefsOf({ notifyKinds: { needsInput: 'os', finished: 'off', failed: 'app' } as never })).toEqual({ notifyNeedsInput: true, notifyFinished: false, notifyFailed: true });
  });

  it('counts lines waiting for input, ignoring hung-up ones', () => {
    expect(needsInputCount(snap([line('a', 'claude', { needsInput: true }), line('b', 'claude', { needsInput: true }, 'hung-up'), line('c', 'claude', {})]))).toBe(1);
  });

  it('reports only groups that went from busy to idle', () => {
    const before = new Map([
      ['w:a', false],
      ['w:b', true],
      ['w:c', false],
    ]);
    const after = new Map([
      ['w:a', true],
      ['w:b', true],
      ['w:c', false],
      ['w:new', true],
    ]);
    expect(idleTransitions(before, after)).toEqual(['w:a']);
  });

  it('names the place after the workspace, then the folder', () => {
    const app = { itemWs: new Map([['line:a', 'w1'], ['line:b', 'gone']]), ws: { workspaces: [{ id: 'w1', name: 'Web' }] } } as unknown as typeof App;
    const placeOf = placeIn(app);
    expect(placeOf(line('a', 'claude', {}))).toBe('Web');
    expect(placeOf(line('b', 'claude', {}))).toBe('b-repo');
    expect(placeOf(line('c', 'claude', {}))).toBe('c-repo');
  });

  it('drops terminal exits that belong to no line or task and honours the failed pref', () => {
    const term = (id: string, exited: boolean, exitCode: number, owner: Partial<Term> = { lineId: 'l1' }) => ({ id, exited, exitCode, cmd: ['make', 'test'], title: '', ...owner }) as unknown as Term;
    const prev: Snapshot = { ...snap([]), terms: [term('a', false, 0), term('b', false, 2, { taskId: 't1' }), term('c', false, 1, {})] };
    const next: Snapshot = { ...snap([]), terms: [term('a', true, 0), term('b', true, 2, { taskId: 't1' }), term('c', true, 1, {})] };
    expect(notificationsFor(prev, next, all)).toEqual([
      { kind: 'finished', key: 'line:l1', title: 'make test finished', body: '' },
      { kind: 'failed', key: 'term:b', title: 'make test failed', body: expect.stringContaining('2') },
    ]);
    expect(notificationsFor(prev, next, { ...all, notifyFailed: false }).map((n) => n.key)).toEqual(['line:l1']);
  });
});

describe('startNotifications', () => {
  type Listener = (prev: Snapshot, next: Snapshot, e: DaemonEvent) => void;
  const item = (key: string, group: SidebarItem['group']): SidebarItem => ({ key, kind: 'claude' as SidebarItem['kind'], group, state: stateOfGroup(group), title: key, at: '' });

  function fakeApp() {
    const listeners = new Set<Listener>();
    const members: Record<string, SidebarItem[]> = { w1: [item('line:a', 'running')], g1: [item('line:a', 'running')] };
    const app = {
      data: snap([]),
      itemWs: new Map([['line:a', 'w1']]),
      ws: { workspaces: [{ id: 'w1', name: 'Web', notifyIdle: true }, { id: 'w2', name: 'Quiet', notifyIdle: false }], groups: [{ id: 'g1', name: 'Ops', notifyIdle: true, members: [] }] },
      conn: { target: { kind: 'local' } },
      membersOf: (id: string) => members[id] ?? [],
      groupItems: (id: string) => members[id] ?? [],
      onChange: (fn: Listener) => {
        listeners.add(fn);
        return () => listeners.delete(fn);
      },
      others: [] as { key: string; target: { kind: string; host?: string; label: string }; data: Snapshot; ws: { workspaces: { id: string; name: string }[] }; itemWs: Map<string, string> }[],
      onOther: null as null | ((l: unknown, prev: Snapshot, next: Snapshot, e: DaemonEvent) => void),
      notice: vi.fn(),
      open: vi.fn(),
      select: vi.fn(),
    };
    const emitOther = (host: string, next: Snapshot, e: Partial<DaemonEvent> = {}) => {
      let link = app.others.find((l) => l.key === `ssh:${host}`);
      if (!link) app.others.push((link = { key: `ssh:${host}`, target: { kind: 'ssh', host, label: host }, data: snap([]), ws: { workspaces: [{ id: 'w9', name: 'Api' }] }, itemWs: new Map([['line:b', 'w9']]) }));
      const prev = link.data;
      link.data = next;
      app.onOther?.(link, prev, next, e as DaemonEvent);
    };
    const emit = (next: Snapshot, e: Partial<DaemonEvent> = {}) => {
      const prev = app.data;
      app.data = next;
      for (const l of [...listeners]) l(prev, next, e as DaemonEvent);
    };
    return { app, members, listeners, emit, emitOther };
  }

  let doc: { title: string; hasFocus: () => boolean };

  beforeEach(() => {
    vi.useFakeTimers();
    doc = { title: '', hasFocus: () => false };
    vi.stubGlobal('document', doc);
    const data = new Map<string, string>();
    vi.stubGlobal('localStorage', { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) });
    Object.assign(settings, { notifyKinds: { ...settings.notifyKinds, needsInput: 'os', finished: 'os', failed: 'os' }, notifyCooldown: 0 });
  });

  afterEach(() => {
    vi.useRealTimers();
  });

  it('notifies a line that starts waiting once it settles and badges the title', async () => {
    const { app, emit } = fakeApp();
    const stop = startNotifications(app as unknown as typeof App);
    emit(snap([line('a', 'claude', { needsInput: true })]), { at: 'ev-1' });
    await vi.waitFor(() => expect(doc.title).toBe('(1) '));
    expect(app.notice).not.toHaveBeenCalled();
    vi.advanceTimersByTime(1500);
    expect(app.notice).toHaveBeenCalledWith({
      id: 'needsInput|line:a|ev-1',
      kind: 'needsInput',
      title: 'Claude Code needs your input',
      body: 'T a · Web',
      action: { kind: 'line', key: 'line:a' },
      target: 'line:a',
    });
    emit(snap([line('a', 'claude', { needsInput: false })]), { id: 'ev-2' });
    await vi.waitFor(() => expect(doc.title).toBe(''));
    stop();
  });

  it('notifies a session waiting on a background host of the window, named after its host', async () => {
    const { app, emitOther } = fakeApp();
    const stop = startNotifications(app as unknown as typeof App);
    emitOther('prod', snap([line('b', 'claude', { needsInput: true })]), { at: 'ev-9' });
    await vi.waitFor(() => expect(doc.title).toBe('(1) '));
    await vi.waitFor(() => {
      vi.advanceTimersByTime(1500);
      expect(app.notice).toHaveBeenCalled();
    });
    expect(app.notice).toHaveBeenCalledWith(
      expect.objectContaining({
        id: 'ssh:prod|needsInput|line:b|ev-9',
        kind: 'needsInput',
        title: 'prod: Claude Code needs your input',
        action: { kind: 'line', key: 'line:b', host: 'ssh:prod' },
        target: 'line:b',
      }),
    );
    stop();
  });

  it('counts waiting sessions of every host of the window in the badge', async () => {
    const { app, emit, emitOther } = fakeApp();
    const stop = startNotifications(app as unknown as typeof App);
    emit(snap([line('a', 'claude', { needsInput: true })]));
    emitOther('prod', snap([line('b', 'claude', { needsInput: true })]));
    await vi.waitFor(() => expect(doc.title).toBe('(2) '));
    stop();
  });

  it('stops following background hosts when stopped', async () => {
    const { app, emitOther } = fakeApp();
    const stop = startNotifications(app as unknown as typeof App);
    emitOther('prod', snap([line('b', 'claude', { needsInput: true })]), { at: 'ev-9' });
    stop();
    expect(app.onOther).toBeNull();
    vi.advanceTimersByTime(5000);
    expect(app.notice).not.toHaveBeenCalled();
  });

  describe('system tap listener', () => {
    beforeEach(() => {
      vi.useRealTimers();
      onAction.mockReset();
      unregister.mockClear();
      onAction.mockResolvedValue({ unregister });
      vi.stubGlobal('window', { __TAURI_INTERNALS__: {} });
    });

    afterEach(() => {
      vi.unstubAllGlobals();
    });

    it('listens for notification taps in the desktop app and stops with the notifications', async () => {
      const { app } = fakeApp();
      const stop = startNotifications(app as unknown as typeof App);
      await vi.waitFor(() => expect(onAction).toHaveBeenCalledTimes(1));
      expect(unregister).not.toHaveBeenCalled();
      stop();
      await vi.waitFor(() => expect(unregister).toHaveBeenCalledTimes(1));
    });

    it('does not listen when stopped before the sender has loaded', async () => {
      const { app } = fakeApp();
      startNotifications(app as unknown as typeof App)();
      await new Promise((r) => setTimeout(r, 20));
      expect(onAction).not.toHaveBeenCalled();
    });
  });

  it('skips events that change no line, term or task', () => {
    const { app, emit } = fakeApp();
    startNotifications(app as unknown as typeof App);
    const same = app.data;
    emit({ ...same, activity: { a: { status: 'x' } as never } });
    vi.advanceTimersByTime(5000);
    expect(app.notice).not.toHaveBeenCalled();
    expect(doc.title).toBe('');
  });

  it('drops a pending notification when the line answered before it settled', () => {
    const { app, emit } = fakeApp();
    startNotifications(app as unknown as typeof App);
    emit(snap([line('a', 'claude', { needsInput: true })]), { at: 'ev-1' });
    app.data = snap([line('a', 'claude', { needsInput: false })]);
    vi.advanceTimersByTime(1500);
    expect(app.notice).not.toHaveBeenCalled();
  });

  it('announces a watched workspace and group going all idle, but not on the first look', () => {
    const { app, members, emit } = fakeApp();
    startNotifications(app as unknown as typeof App);
    members.w1 = [item('line:a', 'ready')];
    emit(snap([line('a', 'claude', {})]), { at: 'ev-0' });
    vi.advanceTimersByTime(1500);
    expect(app.notice).not.toHaveBeenCalled();
    members.w1 = [item('line:a', 'running')];
    members.g1 = [item('line:a', 'running')];
    emit(snap([line('a', 'claude', { busy: true })]), { at: 'ev-1' });
    members.w1 = [item('line:a', 'ready')];
    members.g1 = [item('line:a', 'ready')];
    emit(snap([line('a', 'claude', { busy: true })]), { at: 'ev-2' });
    vi.advanceTimersByTime(1500);
    const idle = app.notice.mock.calls.map(([n]) => n).filter((n) => n.kind === 'allIdle');
    expect(idle).toEqual([
      { id: 'allIdle|All agents in Web are idle|ev-2', kind: 'allIdle', title: 'All agents in Web are idle', body: undefined, action: undefined, target: 'w:w1' },
      { id: 'allIdle|All agents in Ops are idle|ev-2', kind: 'allIdle', title: 'All agents in Ops are idle', body: undefined, action: undefined, target: 'g:g1' },
    ]);
  });

  it('stops listening and cancels pending notifications', () => {
    const { app, listeners, emit } = fakeApp();
    const stop = startNotifications(app as unknown as typeof App);
    emit(snap([line('a', 'claude', { needsInput: true })]), { at: 'ev-1' });
    stop();
    vi.advanceTimersByTime(5000);
    expect(app.notice).not.toHaveBeenCalled();
    expect(listeners.size).toBe(0);
  });
});

describe('notificationsFor on snapshots', () => {
  const line = (id: string, needsInput: boolean): LocalLine => ({
    line: { id, agent: 'claude', state: 'active', lastActivityAt: '' },
    local: { needsInput, title: `T ${id}` },
  });
  const snap = (p: Partial<Snapshot>): Snapshot => ({ lines: [], terms: [], tasks: [], activity: {}, ...p });

  it('notices new needs-input lines only', () => {
    const prev = snap({ lines: [line('a', true), line('b', false)] });
    const next = snap({ lines: [line('a', true), line('b', true)] });
    const n = notificationsFor(prev, next, all);
    expect(n).toMatchObject([{ kind: 'needsInput', key: 'line:b' }]);
    expect(n[0].title).toContain('Claude Code');
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
