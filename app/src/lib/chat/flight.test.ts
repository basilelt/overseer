import { describe, expect, it } from 'vitest';
import { Flights } from './flight';

describe('single flight per key', () => {
  it('skips a key already in flight', async () => {
    const f = new Flights();
    let calls = 0;
    let release!: () => void;
    const first = f.run('a', () => new Promise<void>((r) => { calls++; release = r; }));
    const second = f.run('a', async () => void calls++);
    expect(await second).toBe(false);
    release();
    expect(await first).toBe(true);
    expect(calls).toBe(1);
  });

  it('runs different keys independently', async () => {
    const f = new Flights();
    const a = f.run('a', () => new Promise<void>(() => {}));
    expect(await f.run('b', async () => {})).toBe(true);
    void a;
  });

  it('frees the key once settled, even on failure', async () => {
    const f = new Flights();
    await expect(f.run('a', async () => { throw new Error('x'); })).rejects.toThrow('x');
    expect(await f.run('a', async () => {})).toBe(true);
  });
});
