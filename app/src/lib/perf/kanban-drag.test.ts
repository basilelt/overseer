import { describe, expect, it } from 'vitest';
import { frameLoop, insertBefore } from '../kanban/kanban';
import type { Task } from '../types';

function fakeRaf() {
  const queue = new Map<number, () => void>();
  let seq = 0;
  return {
    raf: (cb: () => void) => (queue.set(++seq, cb), seq),
    caf: (id: number) => void queue.delete(id),
    frame() {
      const cbs = [...queue.values()];
      queue.clear();
      cbs.forEach((cb) => cb());
    },
    get pending() {
      return queue.size;
    },
  };
}

describe('kanban drag work per frame', () => {
  it('locates the drop once per frame however many pointer moves arrive', () => {
    const f = fakeRaf();
    let locates = 0;
    const loop = frameLoop(() => (locates++, false), f.raf, f.caf);
    for (let i = 0; i < 50; i++) loop.arm();
    expect(f.pending).toBe(1);
    f.frame();
    expect(locates).toBe(1);
  });

  it('does no work while the pointer is still and away from the edges', () => {
    const f = fakeRaf();
    let calls = 0;
    const loop = frameLoop(() => (calls++, false), f.raf, f.caf);
    loop.arm();
    f.frame();
    f.frame();
    f.frame();
    expect(calls).toBe(1);
    expect(f.pending).toBe(0);
  });

  it('keeps running each frame only while the board auto-scrolls', () => {
    const f = fakeRaf();
    let scrolls = 3;
    let calls = 0;
    const loop = frameLoop(() => (calls++, scrolls-- > 0), f.raf, f.caf);
    loop.arm();
    for (let i = 0; i < 10; i++) f.frame();
    expect(calls).toBe(4);
    expect(f.pending).toBe(0);
  });

  it('cancel drops the pending frame', () => {
    const f = fakeRaf();
    let calls = 0;
    const loop = frameLoop(() => (calls++, false), f.raf, f.caf);
    loop.arm();
    loop.cancel();
    loop.cancel();
    f.frame();
    expect(calls).toBe(0);
    loop.arm();
    expect(f.pending).toBe(1);
  });

  it('finds the insert marker card in one linear pass', () => {
    const col = (n: number) => Array.from({ length: n }, (_, i) => ({ id: `t${i}` }) as Task);
    let reads = 0;
    const counted = (tasks: Task[]) =>
      tasks.map((t) => new Proxy(t, { get: (o, k) => (k === 'id' && reads++, o[k as keyof Task]) }));
    insertBefore(counted(col(100)), 't0', 99);
    const small = reads;
    reads = 0;
    insertBefore(counted(col(1000)), 't0', 999);
    expect(reads).toBeLessThanOrEqual(small * 10 + 10);
  });
});
