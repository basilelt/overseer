import { describe, expect, it } from 'vitest';
import { EVENT_BATCH_MS, eventBatcher, type BatchClock } from './event-batch';

function setup(apply?: (b: number[]) => void) {
  let t = 1000;
  let pending: { fn: () => void; at: number } | null = null;
  const batches: number[][] = [];
  const clock: BatchClock = {
    now: () => t,
    later: (fn, ms) => (pending = { fn, at: t + ms }),
    cancel: () => (pending = null),
  };
  const b = eventBatcher<number>(apply ?? ((x) => batches.push(x)), EVENT_BATCH_MS, clock);
  const advance = (ms: number) => {
    t += ms;
    if (pending && pending.at <= t) {
      const p = pending;
      pending = null;
      p.fn();
    }
  };
  return { b, batches, advance, waiting: () => pending !== null };
}

describe('eventBatcher', () => {
  it('hands over an event that arrives alone at once', () => {
    const { b, batches, waiting } = setup();
    b.push(1);
    expect(batches).toEqual([[1]]);
    expect(waiting()).toBe(false);
  });

  it('hands over the events of a burst together, in order', () => {
    const { b, batches, advance } = setup();
    b.push(1);
    for (let i = 2; i <= 500; i++) b.push(i);
    expect(batches).toEqual([[1]]);
    advance(EVENT_BATCH_MS);
    expect(batches).toHaveLength(2);
    expect(batches[1]).toEqual(Array.from({ length: 499 }, (_, i) => i + 2));
  });

  it('a long burst costs one batch per window, not one per event', () => {
    const { b, batches, advance } = setup();
    for (let i = 0; i < 3000; i++) {
      b.push(i);
      advance(1);
    }
    advance(EVENT_BATCH_MS);
    expect(batches.flat()).toHaveLength(3000);
    expect(batches.length).toBeLessThanOrEqual(3000 / EVENT_BATCH_MS + 2);
  });

  it('is immediate again once the stream went quiet', () => {
    const { b, batches, advance } = setup();
    b.push(1);
    advance(EVENT_BATCH_MS);
    b.push(2);
    expect(batches).toEqual([[1], [2]]);
  });

  it('measures the window from the end of a slow batch', () => {
    const s = setup((batch) => {
      seen.push(batch);
      s.advance(200);
    });
    const seen: number[][] = [];
    s.b.push(1);
    s.b.push(2);
    expect(seen).toEqual([[1]]);
    s.advance(EVENT_BATCH_MS);
    expect(seen).toEqual([[1], [2]]);
  });

  it('keeps going after a batch that throws', () => {
    let fail = true;
    const seen: number[][] = [];
    const s = setup((batch) => {
      if (fail) throw new Error('boom');
      seen.push(batch);
    });
    expect(() => s.b.push(1)).toThrow('boom');
    fail = false;
    s.b.push(2);
    s.advance(EVENT_BATCH_MS);
    expect(seen).toEqual([[2]]);
  });

  it('drops what is queued and ignores later events once stopped', () => {
    const { b, batches, advance, waiting } = setup();
    b.push(1);
    b.push(2);
    b.stop();
    expect(waiting()).toBe(false);
    advance(EVENT_BATCH_MS);
    b.push(3);
    expect(batches).toEqual([[1]]);
  });
});
