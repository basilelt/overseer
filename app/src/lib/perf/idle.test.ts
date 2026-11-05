import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { setPresence, type Presence } from '../presence';
import { tickWhile } from '../chat/clock.svelte';
import { SystemMonitor } from '../system/monitor.svelte';
import { mockSystem } from '../system/mock';
import { FrameScheduler, type PaintTarget } from '../termrender';
import { UsageFeed } from '../usage/store.svelte';
import { mockUsage } from '../usage/mock';

const MINUTE = 60_000;

function paintTarget(): PaintTarget {
  return {
    viewportY: 0,
    scrollbarOpacity: 0,
    lastCursorY: 0,
    isDisposed: false,
    isOpen: true,
    wasmTerm: { getCursor: () => ({ y: 0 }) },
    renderer: { cursorVisible: true, stopCursorBlink: () => {}, render: () => {} },
  };
}

function idleApp() {
  const p: Presence = { visible: true, focused: true };
  const seen = { requests: 0, frames: 0, timeouts: 0, intervals: 0 };
  const realTimeout = globalThis.setTimeout;
  const realInterval = globalThis.setInterval;
  vi.stubGlobal('setTimeout', ((fn: () => void, ms?: number) =>
    realTimeout(() => {
      seen.timeouts++;
      fn();
    }, ms)) as typeof setTimeout);
  vi.stubGlobal('setInterval', ((fn: () => void, ms?: number) =>
    realInterval(() => {
      seen.intervals++;
      fn();
    }, ms)) as typeof setInterval);
  vi.stubGlobal('requestAnimationFrame', (cb: (t: number) => void) => {
    seen.frames++;
    return realTimeout(() => cb(Date.now()), 16) as unknown as number;
  });
  const client = {
    request: vi.fn(async (_m: string, path: string) => {
      seen.requests++;
      const qs = path.split('?')[1] ?? '';
      return (path.startsWith('/v1/system') ? mockSystem(qs, Date.now()) : mockUsage(qs)) as never;
    }),
  };
  const visible = () => p.visible;
  const active = () => p.visible && p.focused;
  const system = new SystemMonitor(active);
  const usage = new UsageFeed(visible);
  const chip = new UsageFeed(visible);
  const terms = new FrameScheduler(undefined, () => p);
  const shells = [paintTarget(), paintTarget(), paintTarget()];
  const stops = [
    system.use('local:', client, 'chip'),
    usage.use('local:', client, () => () => {}, { range: 'today', by: 'project' }),
    chip.use('local:', client, () => () => {}, { range: 'today', by: 'agent' }),
  ];
  stops.push(tickWhile());
  for (const t of shells) terms.add(t);
  terms.setFocus(shells[0]);
  const presence = (next: Partial<Presence>) => {
    Object.assign(p, next);
    setPresence(next);
    terms.presenceChanged();
    for (const s of [system, usage, chip] as unknown as { onVisibility: () => void }[]) s.onVisibility();
  };
  const reset = () => Object.assign(seen, { requests: 0, frames: 0, timeouts: 0, intervals: 0 });
  return { seen, presence, reset, stop: () => stops.forEach((s) => s()) };
}

describe('idle budget', () => {
  beforeEach(() => vi.useFakeTimers());
  afterEach(() => {
    vi.unstubAllGlobals();
    vi.useRealTimers();
  });

  it('only the cursor blink, the system chip and a running sub-agent clock wake an idle, focused window', async () => {
    const a = idleApp();
    await vi.advanceTimersByTimeAsync(5_000);
    a.reset();
    await vi.advanceTimersByTimeAsync(MINUTE);
    expect(a.seen.requests).toBeLessThanOrEqual(4);
    expect(a.seen.intervals).toBeLessThanOrEqual(160);
    expect(a.seen.frames).toBeLessThanOrEqual(100);
    expect(a.seen.timeouts).toBeLessThanOrEqual(a.seen.frames + a.seen.requests + 2);
    a.stop();
  });

  it('nothing runs while the window is unfocused or hidden', async () => {
    const a = idleApp();
    await vi.advanceTimersByTimeAsync(5_000);
    for (const state of [{ focused: false }, { visible: false }]) {
      a.presence(state);
      await vi.advanceTimersByTimeAsync(1_000);
      a.reset();
      await vi.advanceTimersByTimeAsync(10 * MINUTE);
      expect(a.seen, JSON.stringify(state)).toEqual({ requests: 0, frames: 0, timeouts: 0, intervals: 0 });
      expect(vi.getTimerCount()).toBe(0);
    }
    a.presence({ visible: true, focused: true });
    await vi.advanceTimersByTimeAsync(1_000);
    expect(a.seen.requests).toBeGreaterThan(0);
    a.stop();
  });
});
