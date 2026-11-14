import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { DaemonError } from '../daemon/client';
import type { DaemonEvent } from '../types';
import { CHIP_MIN_MS, DEBOUNCE_MS, PANEL_MIN_MS, RESET_MARGIN_MS, STALE_MS, UsageFeed, limitsRefreshIn } from './store.svelte';
import type { UsageResponse } from './types';
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
    await vi.advanceTimersByTimeAsync(STALE_MS - 10_000);
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
  it('refreshes plan limits on screen while nothing else happens', async () => {
    const s = setup();
    s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent', claudeLimits: true });
    await vi.runOnlyPendingTimersAsync();
    expect(s.calls.length).toBe(1);
    await vi.advanceTimersByTimeAsync(STALE_MS + 10);
    expect(s.calls.length).toBe(2);
    await vi.advanceTimersByTimeAsync(STALE_MS + 10);
    expect(s.calls.length).toBe(3);
  });

  it('refetches as soon as a shown limit window resets', async () => {
    const s = setup();
    const r = limited(Date.now() + 60_000);
    s.client.request.mockImplementation(async (_m: string, path: string) => {
      s.calls.push(path);
      return r as never;
    });
    s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent', claudeLimits: true });
    await vi.runOnlyPendingTimersAsync();
    await vi.advanceTimersByTimeAsync(59_000);
    expect(s.calls.length).toBe(1);
    await vi.advanceTimersByTimeAsync(3_000);
    expect(s.calls.length).toBe(2);
  });

  it('still follows usage.updated while a limits refresh is pending', async () => {
    const s = setup();
    s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent', claudeLimits: true });
    await vi.runOnlyPendingTimersAsync();
    await vi.advanceTimersByTimeAsync(PANEL_MIN_MS);
    s.emit('usage.updated');
    await vi.advanceTimersByTimeAsync(DEBOUNCE_MS + 10);
    expect(s.calls.length).toBe(2);
  });

  it('stays idle when no limit is shown', async () => {
    const s = setup();
    s.client.request.mockImplementation(async (_m: string, path: string) => {
      s.calls.push(path);
      return { ...limited(0), agents: [] } as never;
    });
    s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent' });
    await vi.runOnlyPendingTimersAsync();
    await vi.advanceTimersByTimeAsync(3 * STALE_MS);
    expect(s.calls.length).toBe(1);
  });

  it('skips the timed refresh while hidden', async () => {
    const vis = { v: true };
    const s = setup(vis);
    s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent', claudeLimits: true });
    await vi.runOnlyPendingTimersAsync();
    vis.v = false;
    s.feed.visible = false;
    await vi.advanceTimersByTimeAsync(STALE_MS + 10);
    expect(s.calls.length).toBe(1);
  });

  it('drops the timed refresh while unfocused and catches up on focus', async () => {
    const focus = { v: true };
    const s = setup();
    const feed = new UsageFeed(() => true, Date.now, () => 60, () => focus.v);
    feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent', claudeLimits: true });
    await vi.runOnlyPendingTimersAsync();
    focus.v = false;
    (feed as unknown as { onVisibility: () => void }).onVisibility();
    expect(vi.getTimerCount()).toBe(0);
    await vi.advanceTimersByTimeAsync(STALE_MS + 10);
    expect(s.calls.length).toBe(1);
    focus.v = true;
    (feed as unknown as { onVisibility: () => void }).onVisibility();
    await vi.advanceTimersByTimeAsync(10);
    expect(s.calls.length).toBe(2);
  });

  it('refetches stale data when the same view is reused', async () => {
    const s = setup();
    const stop = s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent' });
    await vi.runOnlyPendingTimersAsync();
    stop();
    await vi.advanceTimersByTimeAsync(STALE_MS + 10);
    s.feed.use('local:', s.client, s.subscribe, { range: 'today', by: 'agent' });
    await vi.advanceTimersByTimeAsync(10);
    expect(s.calls.length).toBe(2);
  });
});

describe('limitsRefreshIn', () => {
  const now = Date.parse('2026-11-06T12:00:00Z');
  const win = (resetsAt: string | null, stale = false) => ({ ...limited(0), agents: [{ ...limited(0).agents[0], limits: { source: 'claude', windows: [{ key: '5h', windowMinutes: 300, usedPercent: 1, resetsAt, stale }], hit: null } }] });

  it('waits for the nearest reset plus a margin', () => {
    expect(limitsRefreshIn(win(new Date(now + 30_000).toISOString()), now)).toBe(30_000 + RESET_MARGIN_MS);
  });

  it('caps the wait at the stale interval', () => {
    expect(limitsRefreshIn(win(new Date(now + 3_600_000).toISOString()), now)).toBe(STALE_MS);
    expect(limitsRefreshIn(win(null), now)).toBe(STALE_MS);
    expect(limitsRefreshIn(win(new Date(now - 1).toISOString()), now)).toBe(STALE_MS);
  });

  it('ignores reset windows and agents without limits', () => {
    expect(limitsRefreshIn(win(new Date(now + 30_000).toISOString(), true), now)).toBeNull();
    expect(limitsRefreshIn({ ...limited(0), agents: [] }, now)).toBeNull();
  });
});

function limited(resetsAt: number): UsageResponse {
  const base = mockUsage('range=today&by=agent&claudeLimits=1') as UsageResponse;
  return {
    ...base,
    indexing: false,
    agents: [
      {
        ...base.agents[0],
        limits: {
          source: 'claude',
          windows: [{ key: '5h', windowMinutes: 300, usedPercent: 40, resetsAt: new Date(resetsAt).toISOString(), stale: false }],
          hit: null,
        },
      },
    ],
  };
}
