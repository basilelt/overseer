import { clamp } from '../clamp';
import { FONT_SIZE_MAX, FONT_SIZE_MIN } from '../terminal/terminal-font';

export const LONG_PRESS_MS = 500;
export const SWIPE_MIN_PX = 56;
export const MOVE_TOLERANCE_PX = 10;

export function swipeDirection(dx: number, dy: number, min = SWIPE_MIN_PX): -1 | 0 | 1 {
  if (Math.abs(dx) < min || Math.abs(dx) < Math.abs(dy) * 1.5) return 0;
  return dx < 0 ? 1 : -1;
}

export function movedTooFar(dx: number, dy: number, tolerance = MOVE_TOLERANCE_PX): boolean {
  return Math.hypot(dx, dy) > tolerance;
}

function swallowRelease(target: EventTarget) {
  const ctl = new AbortController();
  const opts = { capture: true, signal: ctl.signal };
  target.addEventListener(
    'click',
    (e) => {
      if ((e as MouseEvent).detail === 0) return;
      ctl.abort();
      e.preventDefault();
      e.stopImmediatePropagation();
    },
    opts,
  );
  target.addEventListener('pointerdown', () => ctl.abort(), opts);
}

export function pressTimer(ms: number, opts: { swallow?: boolean; target?: EventTarget } = {}) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let ox = NaN;
  let oy = NaN;
  const cancel = () => {
    clearTimeout(timer);
    ox = NaN;
  };
  return {
    start(x: number, y: number, onFire: () => void) {
      cancel();
      ox = x;
      oy = y;
      timer = setTimeout(() => {
        ox = NaN;
        const target = opts.target ?? (typeof window === 'undefined' ? undefined : window);
        if (opts.swallow && target) swallowRelease(target);
        onFire();
      }, ms);
    },
    move(x: number, y: number) {
      if (movedTooFar(x - ox, y - oy)) cancel();
    },
    cancel,
  };
}

export function pinchFontSize(start: number, startDistance: number, distance: number, min = FONT_SIZE_MIN, max = FONT_SIZE_MAX): number {
  if (startDistance <= 0 || distance <= 0) return start;
  const next = Math.round(start * (distance / startDistance));
  return clamp(next, min, max);
}

export function touchDistance(a: { clientX: number; clientY: number }, b: { clientX: number; clientY: number }): number {
  return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
}
