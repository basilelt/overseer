import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { DaemonError } from '../client';
import type { DaemonEvent } from '../types';
import { CHIP_MIN_MS, DEBOUNCE_MS, PANEL_MIN_MS, UsageFeed } from './store.svelte';
import { mockUsage } from './mock';

function setup(visible = { v: true }) {
  const calls: string[] = [];
  let emit: (e: DaemonEvent) => void = () => {};
  const client = {
    request: vi.fn(async (_m: string, path: string) => {
      calls.push(path);
      return mockUsage(path.split('?')[1] ?? '') as never;
    }),
  };
  const subscribe = (fn: (e: DaemonEvent) => void) => {
    emit = fn;
    return () => (emit = () => {});
  };
  const feed = new UsageFeed(() => visible.v, Date.now, () => 60);
  return { feed, client, calls, subscribe, emit: (type: string) => emit({ id: '1', type: type as DaemonEvent['type'], at: '', data: {} }) };
}

describe('usage feed', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => vi.useRealTimers());

  it('fetches once, then only on usage.updated (throttled)', async () => {
    const s = setup();
    const stop = s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent' });
    await vi.runOnlyPendingTimersAsync();
    expect(s.calls).toEqual(['/v1/usage?range=today&by=agent&tzOffset=60']);
    expect(s.feed.data?.agents.length).toBe(2);
    s.emit('line.activity');
    s.emit('usage.updated');
    s.emit('usage.updated');
    await vi.advanceTimersByTimeAsync(PANEL_MIN_MS + 10);
    expect(s.calls.length).toBe(2);
    await vi.advanceTimersByTimeAsync(600_000);
    expect(s.calls.length).toBe(2);
    stop();
    s.emit('usage.updated');
    await vi.advanceTimersByTimeAsync(DEBOUNCE_MS + 10);
    expect(s.calls.length).toBe(2);
  });

  it('fetches at most once per interval while events keep coming (trailing)', async () => {
    const s = setup();
    s.feed.minInterval = CHIP_MIN_MS;
    s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent' });
    await vi.runOnlyPendingTimersAsync();
    expect(s.calls.length).toBe(1);
    for (let i = 0; i < 15; i++) {
      s.emit('usage.updated');
      await vi.advanceTimersByTimeAsync(2_000);
    }
    expect(s.calls.length).toBe(4);
    await vi.advanceTimersByTimeAsync(CHIP_MIN_MS);
    expect(s.calls.length).toBe(4);
    s.emit('usage.updated');
    await vi.advanceTimersByTimeAsync(DEBOUNCE_MS + 10);
    expect(s.calls.length).toBe(5);
  });

  it('lets an open panel refresh every 2 s at most', async () => {
    const s = setup();
    s.feed.use('local:', s.client, s.subscribe, { range: '7d', by: 'project' });
    await vi.runOnlyPendingTimersAsync();
    for (let i = 0; i < 10; i++) {
      s.emit('usage.updated');
      await vi.advanceTimersByTimeAsync(500);
    }
    expect(s.calls.length).toBeLessThanOrEqual(1 + Math.ceil(5_000 / PANEL_MIN_MS));
    expect(s.calls.length).toBeGreaterThan(1);
  });

  it('defers updates while hidden', async () => {
    const vis = { v: true };
    const s = setup(vis);
    s.feed.use('local:', s.client, s.subscribe, { range: '7d', by: 'project' });
    await vi.runOnlyPendingTimersAsync();
    vis.v = false;
    s.feed.visible = false;
    s.emit('usage.updated');
    await vi.advanceTimersByTimeAsync(DEBOUNCE_MS + 10);
    expect(s.calls.length).toBe(1);
  });

  it('goes quiet on servers without the route', async () => {
    const s = setup();
    s.client.request.mockRejectedValueOnce(new DaemonError(404, 'not found'));
    s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent' });
    await vi.runOnlyPendingTimersAsync();
    expect(s.feed.unavailable).toBe(true);
    s.emit('usage.updated');
    await vi.advanceTimersByTimeAsync(60_000);
    expect(s.calls.length).toBe(0);
  });
});
