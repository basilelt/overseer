import { describe, expect, it } from 'vitest';
import { coalesce } from './coalesce';

function deferred() {
  let resolve!: () => void;
  const promise = new Promise<void>((r) => (resolve = r));
  return { promise, resolve };
}

describe('coalesce', () => {
  it('runs once when called alone', async () => {
    let n = 0;
    const run = coalesce(async () => void n++);
    await run();
    expect(n).toBe(1);
  });

  it('folds every call made during a run into one trailing run', async () => {
    const gates: ReturnType<typeof deferred>[] = [];
    let n = 0;
    const run = coalesce(() => {
      n++;
      const g = deferred();
      gates.push(g);
      return g.promise;
    });
    const first = run();
    const others = Array.from({ length: 20 }, () => run());
    expect(n).toBe(1);
    gates[0].resolve();
    await Promise.resolve();
    await Promise.resolve();
    expect(n).toBe(2);
    gates[1].resolve();
    await Promise.all([first, ...others]);
    expect(n).toBe(2);
  });

  it('never runs two calls at the same time', async () => {
    let inFlight = 0;
    let peak = 0;
    const run = coalesce(async () => {
      inFlight++;
      peak = Math.max(peak, inFlight);
      await Promise.resolve();
      inFlight--;
    });
    await Promise.all(Array.from({ length: 10 }, () => run()));
    expect(peak).toBe(1);
  });

  it('starts a fresh run after a failed one', async () => {
    let n = 0;
    const run = coalesce(async () => {
      n++;
      if (n === 1) throw new Error('boom');
    });
    await expect(run()).rejects.toThrow('boom');
    await run();
    expect(n).toBe(2);
  });
});
