import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { DaemonError } from '../daemon/client';
import { CHIP_MS, RETRY_MS, SystemMonitor, pageVisible } from './monitor.svelte';
import { mockSystem } from './mock';

function client(fail?: unknown) {
  const calls: string[] = [];
  return {
    calls,
    request: vi.fn(async (_m: string, path: string) => {
      calls.push(path);
      if (fail) throw fail;
      return mockSystem(path.split('?')[1] ?? '', Date.now()) as never;
    }),
  };
}

const flush = async () => {
  for (let i = 0; i < 5; i++) await Promise.resolve();
};

describe('SystemMonitor', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('does nothing without users', async () => {
    const c = client();
    new SystemMonitor(() => true);
    await vi.advanceTimersByTimeAsync(60_000);
    expect(c.calls).toEqual([]);
  });

  it('polls every 2 s while a live view is open, every 15 s for the chip only, and stops on release', async () => {
    const m = new SystemMonitor(() => true);
    const c = client();
    const chip = m.use('ssh:a', c, 'chip');
    await vi.advanceTimersByTimeAsync(0);
    expect(c.calls).toEqual(['/v1/system']);
    expect(m.current?.supported).toBe(true);
    expect(m.history).toHaveLength(300);
    await vi.advanceTimersByTimeAsync(CHIP_MS - 100);
    expect(c.calls).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(200);
    expect(c.calls).toHaveLength(2);
    expect(c.calls[1]).toMatch(/^\/v1\/system\?since=\d+$/);
    const live = m.use('ssh:a', c, 'live');
    await vi.advanceTimersByTimeAsync(0);
    expect(c.calls).toHaveLength(3);
    await vi.advanceTimersByTimeAsync(2000);
    expect(c.calls).toHaveLength(4);
    live();
    chip();
    await vi.advanceTimersByTimeAsync(60_000);
    expect(c.calls).toHaveLength(4);
  });

  it('pauses while hidden or unfocused', async () => {
    let visible = false;
    const m = new SystemMonitor(() => visible);
    const c = client();
    const off = m.use('local:', c, 'live');
    await vi.advanceTimersByTimeAsync(30_000);
    expect(c.calls).toEqual([]);
    expect(m.visible).toBe(false);
    off();
  });

  it('needs focus on desktop but only visibility on phones', () => {
    const doc = (visibilityState: DocumentVisibilityState, focus: boolean) => ({ visibilityState, hasFocus: () => focus });
    expect(pageVisible(undefined, false)).toBe(true);
    expect(pageVisible(doc('visible', true), false)).toBe(true);
    expect(pageVisible(doc('visible', false), false)).toBe(false);
    expect(pageVisible(doc('visible', false), true)).toBe(true);
    expect(pageVisible(doc('hidden', true), true)).toBe(false);
  });

  it('resets when the host changes', async () => {
    const m = new SystemMonitor(() => true);
    const a = client();
    const offA = m.use('ssh:a', a, 'live');
    await vi.advanceTimersByTimeAsync(0);
    expect(m.current).not.toBeNull();
    offA();
    const b = client(new Error('down'));
    const offB = m.use('ssh:b', b, 'live');
    expect(m.current).toBeNull();
    expect(m.history).toEqual([]);
    await vi.advanceTimersByTimeAsync(0);
    expect(b.calls).toEqual(['/v1/system']);
    await vi.advanceTimersByTimeAsync(RETRY_MS - 100);
    expect(b.calls).toHaveLength(1);
    await vi.advanceTimersByTimeAsync(200);
    expect(b.calls).toHaveLength(2);
    offB();
  });

  it('gives up quietly on a server without the route', async () => {
    const m = new SystemMonitor(() => true);
    const c = client(new DaemonError(404, 'no route'));
    const off = m.use('ssh:old', c, 'chip');
    await vi.advanceTimersByTimeAsync(0);
    await flush();
    expect(m.unavailable).toBe(true);
    await vi.advanceTimersByTimeAsync(120_000);
    expect(c.calls).toHaveLength(1);
    off();
  });
});
