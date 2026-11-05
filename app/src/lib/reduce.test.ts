import { describe, expect, it } from 'vitest';
import { applyEvent, mergeFetched, touchedKey, type Snapshot, type Touched } from './reduce';
import { projectName, statusLine } from './state.svelte';
import type { Activity, DaemonEvent, EventType, Term } from './types';

const ev = (type: EventType, data: unknown): DaemonEvent => ({ id: '1', type, at: '', data });
const empty: Snapshot = { lines: [], terms: [], tasks: [], activity: {} };
const term = { id: 't1', cols: 80, rows: 24, exited: false } as Term;

describe('applyEvent', () => {
  it('upserts lines and merges local flags', () => {
    let s = applyEvent(empty, ev('line.created', { line: { id: 'a', state: 'active' }, local: { busy: true } }));
    s = applyEvent(s, ev('line.state_changed', { line: { id: 'a', state: 'idle' }, from: 'active', to: 'idle' }));
    expect(s.lines).toEqual([{ line: { id: 'a', state: 'idle' }, local: { busy: true } }]);
  });

  it('applies term created, updated and exited', () => {
    let s = applyEvent(empty, ev('term.created', { term }));
    s = applyEvent(s, ev('term.updated', { term: { ...term, cols: 120 } }));
    s = applyEvent(s, ev('term.exited', { term: { ...term, cols: 120, exited: true, exitCode: 0 } }));
    expect(s.terms).toEqual([{ ...term, cols: 120, exited: true, exitCode: 0 }]);
  });

  it('adds, updates and deletes tasks', () => {
    let s = applyEvent(empty, ev('task.created', { task: { id: 'k', status: 'pending' } }));
    s = applyEvent(s, ev('task.updated', { task: { id: 'k', status: 'auto' } }));
    expect(s.tasks.map((t) => t.status)).toEqual(['auto']);
    expect(applyEvent(s, ev('task.deleted', { id: 'k' })).tasks).toEqual([]);
  });

  it('keeps the snapshot when an update changes nothing', () => {
    const s = applyEvent(empty, ev('line.created', { line: { id: 'a', state: 'active', branch: 'main' }, local: { busy: true } }));
    expect(applyEvent(s, ev('line.updated', { line: { id: 'a', state: 'active', branch: 'main' } }))).toBe(s);
    expect(applyEvent(s, ev('line.updated', { line: { id: 'a', state: 'active' }, local: { busy: true } }))).toBe(s);
    expect(applyEvent(s, ev('line.updated', { line: { id: 'a', branch: 'dev' } })).lines[0].line.branch).toBe('dev');
    const t = applyEvent(empty, ev('term.created', { term }));
    expect(applyEvent(t, ev('term.updated', { term: { ...term } }))).toBe(t);
    const k = applyEvent(empty, ev('task.created', { task: { id: 'k', status: 'pending' } }));
    expect(applyEvent(k, ev('task.updated', { task: { id: 'k', status: 'pending' } }))).toBe(k);
  });

  it('keeps the snapshot for a bare line.activity notice', () => {
    expect(applyEvent(empty, ev('line.activity', { lineId: 'a' }))).toBe(empty);
  });

  it('keeps the lines array when activity does not flip needsInput', () => {
    const activity = { workLog: [], lastAssistantText: 'hi', needsInput: false } as unknown as Activity;
    const s = applyEvent(empty, ev('line.created', { line: { id: 'a', state: 'active' }, local: { needsInput: false } }));
    const a = applyEvent(s, ev('line.activity', { lineId: 'a', activity }));
    expect(a.lines).toBe(s.lines);
    expect(a.activity.a).toEqual(activity);
    expect(applyEvent(a, ev('line.activity', { lineId: 'a', activity: { ...activity } }))).toBe(a);
    const b = applyEvent(a, ev('line.activity', { lineId: 'a', activity: { ...activity, needsInput: true } }));
    expect(b.lines).not.toBe(a.lines);
    expect(b.lines[0].local?.needsInput).toBe(true);
  });
});

describe('helpers', () => {
  const act = { workLog: [{ label: 'Edit a.rs' }], lastAssistantText: 'Which one?\nmore', needsInput: false } as Activity;

  it('picks the status line', () => {
    expect(statusLine(act)).toBe('Edit a.rs');
    expect(statusLine({ ...act, needsInput: true })).toBe('Which one?');
  });

  it('names projects by path', () => {
    expect(projectName([{ path: '/r/x', name: 'Xavier' }], '/r/x')).toBe('Xavier');
    expect(projectName([], '/home/u/src/shop/')).toBe('shop');
  });
});

describe('touchedKey', () => {
  it('names the record a line, term or task event is about', () => {
    expect(touchedKey(ev('line.state_changed', { line: { id: 'a' } }))).toBe('line:a');
    expect(touchedKey(ev('term.updated', { term }))).toBe('term:t1');
    expect(touchedKey(ev('term.exited', term))).toBe('term:t1');
    expect(touchedKey(ev('task.updated', { task: { id: 'k' } }))).toBe('task:k');
    expect(touchedKey(ev('task.deleted', { id: 'k' }))).toBe('task:k');
    expect(touchedKey(ev('task.deleted', { task: { id: 'k' } }))).toBe('task:k');
  });

  it('ignores events that carry no record or are about something else', () => {
    expect(touchedKey(ev('line.updated', {}))).toBeNull();
    expect(touchedKey(ev('term.created', null))).toBeNull();
    expect(touchedKey(ev('task.created', {}))).toBeNull();
    expect(touchedKey(ev('task.deleted', {}))).toBeNull();
    expect(touchedKey(ev('line.activity', { lineId: 'a' }))).toBeNull();
    expect(touchedKey(ev('workspace.updated', {}))).toBeNull();
  });
});

describe('mergeFetched', () => {
  type R = { id: string; v: string };
  const key = (r: R) => r.id;
  const fetched: R[] = [{ id: 'a', v: 'old' }, { id: 'b', v: 'old' }];

  it('returns the fetched list when no event touched that kind since the fetch began', () => {
    const touched: Touched = new Map([['line:a', 1], ['task:a', 5]]);
    expect(mergeFetched(fetched, [{ id: 'a', v: 'new' }], 'line', key, touched, 1)).toBe(fetched);
    expect(mergeFetched(fetched, [{ id: 'a', v: 'new' }], 'line', key, new Map(), 0)).toBe(fetched);
  });

  it('keeps the event-applied record over a fetched one that is older than the event', () => {
    const touched: Touched = new Map([['line:a', 2]]);
    const out = mergeFetched(fetched, [{ id: 'a', v: 'new' }, { id: 'b', v: 'cur' }], 'line', key, touched, 1);
    expect(out).toEqual([{ id: 'a', v: 'new' }, fetched[1]]);
  });

  it('keeps a record an event created while the fetch was in flight', () => {
    const touched: Touched = new Map([['line:c', 2]]);
    const out = mergeFetched(fetched, [{ id: 'c', v: 'born' }], 'line', key, touched, 1);
    expect(out).toEqual([...fetched, { id: 'c', v: 'born' }]);
  });

  it('drops a fetched record an event removed while the fetch was in flight', () => {
    const touched: Touched = new Map([['line:a', 2]]);
    expect(mergeFetched(fetched, [{ id: 'b', v: 'old' }], 'line', key, touched, 1)).toEqual([fetched[1]]);
  });
});
