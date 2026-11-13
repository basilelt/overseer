import { describe, expect, it, vi } from 'vitest';
import { Drafts, type DraftStorage } from './drafts';
import { inFlight, single } from './daemon/inflight';

const memory = () => {
  const data = new Map<string, string>();
  const s: DraftStorage & { data: Map<string, string> } = {
    data,
    getItem: (k) => data.get(k) ?? null,
    setItem: (k, v) => void data.set(k, v),
    removeItem: (k) => void data.delete(k),
  };
  return s;
};

const manual = () => {
  const queue: (() => void)[] = [];
  return { queue, set: (fn: () => void) => queue.push(fn), clear: vi.fn() };
};

describe('persisted drafts', () => {
  it('survives a reload and writes once per burst of keystrokes', () => {
    const storage = memory();
    const timers = manual();
    const setItem = vi.spyOn(storage, 'setItem');
    const a = new Drafts('k', undefined, { storage, timers });
    for (const text of ['h', 'he', 'hel', 'hello']) a.set('line:1', text);
    expect(timers.queue).toHaveLength(1);
    timers.queue[0]();
    expect(setItem).toHaveBeenCalledTimes(1);
    const b = new Drafts('k', undefined, { storage, timers: manual() });
    expect(b.get('line:1')).toBe('hello');
  });

  it('drops empty drafts, keeps the newest ones and skips oversized values', () => {
    const storage = memory();
    const d = new Drafts('k', undefined, { storage, timers: manual(), max: 2, maxLength: 20 });
    d.set('a', 'one');
    d.set('b', 'two');
    d.set('c', 'three');
    d.set('big', 'x'.repeat(50));
    d.set('b', '');
    d.flush();
    expect(new Drafts('k', undefined, { storage, timers: manual() }).get('c')).toBe('three');
    const saved = JSON.parse(storage.data.get('k')!);
    expect(saved).toEqual([['c', 'three']]);
    d.set('c', '');
    d.flush();
    expect(storage.data.has('k')).toBe(false);
  });

  it('works without storage and ignores corrupt data', () => {
    const storage = memory();
    storage.data.set('k', '{not json');
    expect(new Drafts('k', undefined, { storage, timers: manual() }).get('x')).toBeUndefined();
    const none = new Drafts('k', undefined, { storage: null, timers: manual() });
    none.set('x', 'y');
    none.flush();
    expect(none.get('x')).toBe('y');
  });
});

describe('single flight', () => {
  it('runs one action per key until it settles, even when it fails', async () => {
    let release!: () => void;
    const run = vi.fn(() => new Promise<string>((r) => (release = () => r('ok'))));
    const a = single('send:1', run);
    const b = single('send:1', run);
    expect(b).toBe(a);
    expect(inFlight('send:1')).toBe(true);
    release();
    await expect(a).resolves.toBe('ok');
    expect(inFlight('send:1')).toBe(false);
    const boom = single('send:1', () => Promise.reject(new Error('503')));
    await expect(boom).rejects.toThrow('503');
    expect(inFlight('send:1')).toBe(false);
    await expect(single('send:1', () => Promise.resolve(2))).resolves.toBe(2);
    expect(run).toHaveBeenCalledTimes(1);
  });
});
