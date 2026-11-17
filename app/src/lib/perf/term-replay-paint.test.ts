import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { FrameScheduler, type Clock, type PaintTarget } from '../terminal/termrender';
import { REPLAY_FRAME, replayPaint } from '../terminal/termreplay';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function mount(frames: number, hold: boolean) {
  const queue: ((now: number) => void)[] = [];
  const clock: Clock = { frame: (cb) => queue.push(cb), every: () => 0, cancel: () => {} };
  const s = new FrameScheduler(clock, () => ({ visible: true, focused: true }));
  let now = 0;
  const frame = () => {
    now += 40;
    for (const f of queue.splice(0)) f(now);
  };
  const t = {
    viewportY: 0,
    scrollbarOpacity: 0,
    lastCursorY: 0,
    isDisposed: false,
    isOpen: true,
    wasmTerm: { getCursor: () => ({ y: 0 }) },
    renderer: { cursorVisible: true, stopCursorBlink: () => {}, render: () => {} },
  } satisfies PaintTarget;
  s.add(t);
  const p = replayPaint(
    () => hold && s.hold(t),
    () => s.release(t),
  );
  p.begin();
  frame();
  p.up();
  for (let i = 0; i < frames; i++) {
    s.invalidate(t);
    p.chunk(i === frames - 1 ? 512 : REPLAY_FRAME);
    frame();
  }
  vi.advanceTimersByTime(2000);
  frame();
  return s.paints;
}

describe('terminal replay paints', () => {
  it('paints a mounted terminal once whatever the number of replayed frames', () => {
    expect(mount(2, true)).toBe(1);
    expect(mount(32, true)).toBe(1);
  });

  it('would paint once per replayed frame without the hold', () => {
    expect(mount(32, false)).toBe(33);
  });
});
