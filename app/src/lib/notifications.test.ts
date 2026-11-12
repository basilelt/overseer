import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { addEntry, loadEntries, MAX_ENTRIES, storageKey, SUPERSEDE_MS, type Entry } from './notifications';
import { center, NotificationCenter, runEntry, startCenter } from './notifications.svelte';
import { app } from './state.svelte';
import { updater } from './updater.svelte';
import type { KV } from './notify';

function memory(): KV & { data: Map<string, string> } {
  const data = new Map<string, string>();
  return { data, getItem: (k) => data.get(k) ?? null, setItem: (k, v) => void data.set(k, v) };
}

function clock(start = 1_000_000) {
  const c = { t: start, now: () => c.t };
  return c;
}

describe('addEntry', () => {
  it('prepends a new unread entry', () => {
    const a = addEntry([], { kind: 'error', title: 'boom' }, 5)!;
    const b = addEntry(a, { kind: 'finished', title: 'done' }, 6)!;
    expect(b.map((e) => e.title)).toEqual(['done', 'boom']);
    expect(b.every((e) => !e.read)).toBe(true);
  });

  it('ignores a replayed stable id', () => {
    const a = addEntry([], { id: 'needsInput|line:1|ev-1', kind: 'needsInput', title: 'Claude needs input' }, 5)!;
    expect(addEntry(a, { id: 'needsInput|line:1|ev-1', kind: 'needsInput', title: 'Claude needs input' }, 9)).toBeNull();
  });

  it('keeps one entry per key, newest first', () => {
    let list: Entry[] = [];
    for (let i = 0; i < 5; i++) {
      list = addEntry(list, { kind: 'portForwarded', title: 'Port 3000 forwarded', coalesce: 'port:ssh:dev:3000', quiet: true }, 1000 + i)!;
      list = addEntry(list, { kind: 'portClosed', title: 'Port 3000 closed', coalesce: 'port:ssh:dev:3000', quiet: true }, 1000 + i)!;
    }
    list = addEntry(list, { kind: 'error', title: 'other' }, 2000)!;
    list = addEntry(list, { kind: 'portForwarded', title: 'Port 3000 forwarded', coalesce: 'port:ssh:dev:3000', quiet: true }, 10 * 3600_000)!;
    expect(list.map((e) => [e.id, e.kind, e.at])).toEqual([
      ['port:ssh:dev:3000', 'portForwarded', 10 * 3600_000],
      [expect.any(String), 'error', 2000],
    ]);
    expect(list[0].read).toBe(true);
  });

  it('marks a merged entry unread on a new event, and a quiet one keeps its read state', () => {
    const read = [{ ...addEntry([], { kind: 'error', title: 'x', coalesce: 'k' }, 0)![0], read: true }];
    expect(addEntry(read, { kind: 'error', title: 'x', coalesce: 'k' }, 1)![0].read).toBe(false);
    expect(addEntry(read, { kind: 'error', title: 'x', coalesce: 'k', quiet: true }, 1)![0].read).toBe(true);
    const unread = addEntry([], { kind: 'error', title: 'x', coalesce: 'k' }, 0)!;
    expect(addEntry(unread, { kind: 'portClosed', title: 'x', coalesce: 'k', quiet: true }, 1)![0].read).toBe(false);
  });

  it('ignores a replayed event id after it was merged into a row', () => {
    const a = addEntry([], { id: 'ev-1', kind: 'finished', title: 'done', coalesce: 'line:a' }, 0)!;
    expect(a[0]).toMatchObject({ id: 'line:a', ev: 'ev-1' });
    expect(addEntry(a, { id: 'ev-1', kind: 'finished', title: 'done', coalesce: 'line:a' }, 5)).toBeNull();
  });

  it('keeps a testing or failed row over the finished turn that follows it, not later', () => {
    const testing = addEntry([], { kind: 'taskTesting', title: 'ready', coalesce: 'line:a' }, 0)!;
    expect(addEntry(testing, { kind: 'finished', title: 'done', coalesce: 'line:a' }, SUPERSEDE_MS - 1)).toBeNull();
    expect(addEntry(testing, { kind: 'finished', title: 'done', coalesce: 'line:a' }, SUPERSEDE_MS)![0].kind).toBe('finished');
    const failed = addEntry([], { kind: 'failed', title: 'boom', coalesce: 'line:a' }, 0)!;
    expect(addEntry(failed, { kind: 'finished', title: 'done', coalesce: 'line:a' }, 1)).toBeNull();
    expect(addEntry(testing, { kind: 'needsInput', title: 'ask', coalesce: 'line:a' }, 1)![0].kind).toBe('needsInput');
  });

  it('compacts a stored list from older versions to one row per session', () => {
    const kv = memory();
    const line = (id: string, kind: Entry['kind'], at: number, read = false): Entry => ({ id, kind, title: kind, at, read, count: 3, action: { kind: 'line', key: 'line:a' } }) as Entry;
    const idle = (id: string, at: number): Entry => ({ id: `allIdle|All agents in overseer are idle|${id}`, kind: 'allIdle', title: 'All agents in overseer are idle', at, read: false });
    const named = (id: string): Entry => ({ id, kind: 'allIdle', title: 'All agents in overseer are idle', at: 1, read: false });
    kv.data.set(
      storageKey('local'),
      JSON.stringify([line('t3', 'taskTesting', 30), line('f3', 'finished', 29), idle('i3', 28), line('t2', 'taskTesting', 20, true), idle('2026-11-09T07:39:19Z', 19), named('w:1'), named('g:1'), { id: 'update:1.0', kind: 'update', title: 'u', at: 1, read: true }]),
    );
    const list = loadEntries(kv, 'local');
    expect(list.map((e) => [e.id, e.kind, e.ev])).toEqual([
      ['line:a', 'taskTesting', 't3'],
      ['allIdle|All agents in overseer are idle|i3', 'allIdle', undefined],
      ['w:1', 'allIdle', undefined],
      ['g:1', 'allIdle', undefined],
      ['update:1.0', 'update', undefined],
    ]);
    expect(list[0]).not.toHaveProperty('count');
    expect(addEntry(list, { id: 't3', kind: 'taskTesting', title: 'again', coalesce: 'line:a' }, 40)).toBeNull();
    expect(addEntry(list, { kind: 'needsInput', title: 'ask', coalesce: 'line:a' }, 40)!).toHaveLength(5);
  });

  it('caps the list', () => {
    let list: Entry[] = [];
    for (let i = 0; i < MAX_ENTRIES + 20; i++) list = addEntry(list, { kind: 'error', title: String(i) }, i)!;
    expect(list).toHaveLength(MAX_ENTRIES);
    expect(list[0].title).toBe(String(MAX_ENTRIES + 19));
  });
});

describe('NotificationCenter', () => {
  it('marks every row of a session read when it is opened', () => {
    const c = new NotificationCenter(memory(), clock().now);
    c.setHost('local');
    c.add({ kind: 'needsInput', title: 'a', coalesce: 'line:a', action: { kind: 'line', key: 'line:a' } });
    c.add({ kind: 'finished', title: 'b', coalesce: 'line:b', action: { kind: 'line', key: 'line:b' } });
    const before = c.entries;
    c.markSubjectRead('line:zzz');
    expect(c.entries).toBe(before);
    c.markSubjectRead('line:a');
    expect(c.entries.map((e) => [e.id, e.read])).toEqual([
      ['line:b', false],
      ['line:a', true],
    ]);
  });

  it('counts unread, marks read and clears', () => {
    const c = new NotificationCenter(memory(), clock().now);
    c.setHost('local');
    c.add({ kind: 'error', title: 'a' });
    c.add({ kind: 'finished', title: 'b' });
    c.add({ kind: 'portForwarded', title: 'c', quiet: true });
    expect(c.unread).toBe(2);
    c.markRead(c.entries[1].id);
    expect(c.unread).toBe(1);
    c.markAllRead();
    expect(c.unread).toBe(0);
    c.clear();
    expect(c.entries).toEqual([]);
  });

  it('persists per host', () => {
    const kv = memory();
    const c = new NotificationCenter(kv, clock().now);
    c.setHost('local');
    c.add({ kind: 'error', title: 'local one' });
    c.setHost('ssh:dev');
    expect(c.entries).toEqual([]);
    c.add({ kind: 'error', title: 'remote one' });
    const again = new NotificationCenter(kv, clock().now);
    again.setHost('local');
    expect(again.entries.map((e) => e.title)).toEqual(['local one']);
    again.setHost('ssh:dev');
    expect(again.entries.map((e) => e.title)).toEqual(['remote one']);
  });

  it('survives storage that throws or holds garbage', () => {
    const broken: KV = {
      getItem: () => {
        throw new Error('denied');
      },
      setItem: () => {
        throw new Error('quota');
      },
    };
    const c = new NotificationCenter(broken, clock().now);
    c.setHost('local');
    expect(c.add({ kind: 'error', title: 'still works' })).toBe(true);
    expect(c.entries).toHaveLength(1);

    const kv = memory();
    kv.data.set(storageKey('local'), '{not json');
    expect(loadEntries(kv, 'local')).toEqual([]);
    kv.data.set(storageKey('local'), JSON.stringify([{ id: 'ok', kind: 'error', title: 't', at: 1, read: false }, { nope: 1 }, null]));
    expect(loadEntries(kv, 'local').map((e) => e.id)).toEqual(['ok']);
    expect(new NotificationCenter(undefined).add({ kind: 'error', title: 'no storage' })).toBe(true);
  });

  it('two windows on one host keep each other entries', () => {
    const kv = memory();
    const c = clock();
    const a = new NotificationCenter(kv, c.now);
    const b = new NotificationCenter(kv, c.now);
    a.setHost('ssh:dev');
    b.setHost('ssh:dev');
    a.add({ id: 'one', kind: 'finished', title: 'one' });
    b.add({ id: 'two', kind: 'failed', title: 'two' });
    expect(b.entries.map((e) => e.id)).toEqual(['two', 'one']);
    expect(loadEntries(kv, 'ssh:dev').map((e) => e.id)).toEqual(['two', 'one']);
    expect(a.add({ id: 'two', kind: 'failed', title: 'two' })).toBe(false);
    a.sync(storageKey('ssh:dev'));
    expect(a.entries.map((e) => e.id)).toEqual(['two', 'one']);
    a.markRead('one');
    b.sync(storageKey('ssh:dev'));
    expect(b.unread).toBe(1);
    b.sync(storageKey('ssh:other'));
    expect(b.entries).toHaveLength(2);
  });
});

const entry = (over: Partial<Entry>): Entry => ({ id: 'e1', kind: 'error', title: 'x', at: 1, read: false, ...over });

describe('runEntry', () => {
  beforeEach(() => {
    center.clear();
  });
  afterEach(() => {
    vi.restoreAllMocks();
    updater.dialog = false;
  });

  it('marks the entry read and opens the line it points at', () => {
    center.add({ id: 'e1', kind: 'needsInput', title: 'needs you', action: { kind: 'line', key: 'line:a' } });
    const open = vi.spyOn(app, 'open').mockImplementation(() => {});
    const select = vi.spyOn(app, 'select').mockImplementation(() => {});
    runEntry(app, center.entries[0]);
    expect(center.entries[0].read).toBe(true);
    expect(open).toHaveBeenCalledWith(null);
    expect(select).toHaveBeenCalledWith('line:a');
  });

  it('brings the host of a background session into the main area before opening it', async () => {
    const others = app.others;
    app.others = [{ key: 'ssh:prod' }] as typeof app.others;
    vi.spyOn(app, 'open').mockImplementation(() => {});
    const order: string[] = [];
    vi.spyOn(app, 'switchHost').mockImplementation(async (k) => void order.push(`switch ${k}`));
    vi.spyOn(app, 'select').mockImplementation((k) => void order.push(`select ${k}`));
    runEntry(app, entry({ action: { kind: 'line', key: 'line:b', host: 'ssh:prod' } }));
    await vi.waitFor(() => expect(order).toEqual(['switch ssh:prod', 'select line:b']));
    order.length = 0;
    runEntry(app, entry({ action: { kind: 'line', key: 'line:c', host: 'ssh:gone' } }));
    expect(order).toEqual(['select line:c']);
    app.others = others;
  });

  it('opens a link through the app link router', () => {
    const openUrl = vi.spyOn(app, 'openUrl').mockImplementation(() => {});
    vi.spyOn(app, 'open').mockImplementation(() => {});
    runEntry(app, entry({ action: { kind: 'url', url: 'https://x.dev/pr/1' } }));
    expect(openUrl).toHaveBeenCalledWith('https://x.dev/pr/1');
  });

  it('opens the update dialog for update entries', () => {
    vi.spyOn(app, 'open').mockImplementation(() => {});
    runEntry(app, entry({ kind: 'update', action: { kind: 'update' } }));
    expect(updater.dialog).toBe(true);
  });

  it('marks a session read when the user opens it', () => {
    vi.spyOn(app, 'open').mockImplementation(() => {});
    center.add({ kind: 'needsInput', title: 'needs you', coalesce: 'line:open-me', action: { kind: 'line', key: 'line:open-me' } });
    center.add({ kind: 'finished', title: 'other', coalesce: 'line:other', action: { kind: 'line', key: 'line:other' } });
    app.selectedKey = 'line:open-me';
    expect(center.entries.map((e) => [e.id, e.read])).toEqual([
      ['line:other', false],
      ['line:open-me', true],
    ]);
    app.selectedKey = null;
    app.select('line:other');
    expect(center.unread).toBe(0);
    app.select(null);
  });

  it('only marks read when the entry has no action', () => {
    center.add({ id: 'plain', kind: 'error', title: 'boom' });
    const open = vi.spyOn(app, 'open').mockImplementation(() => {});
    runEntry(app, center.entries[0]);
    expect(center.unread).toBe(0);
    expect(open).not.toHaveBeenCalled();
  });
});

describe('startCenter', () => {
  const listeners = new Map<string, (e: unknown) => void>();

  beforeEach(() => {
    listeners.clear();
    vi.stubGlobal('addEventListener', (t: string, fn: (e: unknown) => void) => listeners.set(t, fn));
    vi.stubGlobal('removeEventListener', (t: string, fn: (e: unknown) => void) => listeners.get(t) === fn && listeners.delete(t));
  });

  afterEach(() => {
    vi.unstubAllGlobals();
    center.setHost('local');
  });

  it('reloads the list when another window writes the same host, and stops listening', () => {
    const stop = startCenter(app);
    center.setHost('ssh:center-box');
    center.clear();
    const stale = [entry({ id: 'stale' })];
    center.entries = stale;
    listeners.get('storage')!({ key: storageKey('ssh:other') });
    expect(center.entries).toBe(stale);
    listeners.get('storage')!({ key: storageKey('ssh:center-box') });
    expect(center.entries).toEqual([]);
    center.entries = stale;
    listeners.get('storage')!({ key: null });
    expect(center.entries).toEqual([]);
    stop();
    expect(listeners.has('storage')).toBe(false);
  });
});
