import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { addEntry, COALESCE_MS, loadEntries, MAX_ENTRIES, storageKey, type Entry } from './notifications';
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

  it('coalesces the same key within the window into one entry', () => {
    let list: Entry[] = [];
    for (let i = 0; i < 5; i++) {
      list = addEntry(list, { kind: 'portForwarded', title: 'Port 3000 forwarded', coalesce: 'port:ssh:dev:3000', quiet: true }, 1000 + i)!;
      list = addEntry(list, { kind: 'portClosed', title: 'Port 3000 closed', coalesce: 'port:ssh:dev:3000', quiet: true }, 1000 + i)!;
    }
    expect(list).toHaveLength(1);
    expect(list[0]).toMatchObject({ kind: 'portClosed', count: 10, read: true });
  });

  it('starts a fresh entry once the window has passed', () => {
    const a = addEntry([], { kind: 'portForwarded', title: 'a', coalesce: 'k' }, 0)!;
    const b = addEntry(a, { kind: 'portForwarded', title: 'b', coalesce: 'k' }, COALESCE_MS + 1)!;
    expect(b).toHaveLength(1);
    expect(b[0]).toMatchObject({ title: 'b', at: COALESCE_MS + 1 });
    expect(b[0].count).toBeUndefined();
  });

  it('marks a coalesced entry unread again when its kind changes', () => {
    let list = addEntry([], { kind: 'error', title: 'x', coalesce: 'k' }, 0)!;
    list = [{ ...list[0], read: true }];
    expect(addEntry(list, { kind: 'error', title: 'x', coalesce: 'k' }, 1)![0].read).toBe(true);
    expect(addEntry(list, { kind: 'failed', title: 'x', coalesce: 'k' }, 1)![0].read).toBe(false);
  });

  it('caps the list', () => {
    let list: Entry[] = [];
    for (let i = 0; i < MAX_ENTRIES + 20; i++) list = addEntry(list, { kind: 'error', title: String(i) }, i)!;
    expect(list).toHaveLength(MAX_ENTRIES);
    expect(list[0].title).toBe(String(MAX_ENTRIES + 19));
  });
});

describe('NotificationCenter', () => {
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
