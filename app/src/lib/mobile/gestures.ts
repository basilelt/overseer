import { clamp } from '../clamp';
import { PINCH_FONT_MAX, PINCH_FONT_MIN } from '../terminal-font';

export const LONG_PRESS_MS = 450;
export const CHIP_PRESS_MS = 500;
export const TOUCH_MENU_MS = 500;
export const SWIPE_MIN_PX = 56;
export const MOVE_TOLERANCE_PX = 10;

export function swipeDirection(dx: number, dy: number, min = SWIPE_MIN_PX): -1 | 0 | 1 {
  if (Math.abs(dx) < min || Math.abs(dx) < Math.abs(dy) * 1.5) return 0;
  return dx < 0 ? 1 : -1;
}

export function movedTooFar(dx: number, dy: number, tolerance = MOVE_TOLERANCE_PX): boolean {
  return Math.hypot(dx, dy) > tolerance;
}

export function pressTimer(ms: number) {
  let timer: ReturnType<typeof setTimeout> | undefined;
  let origin: { x: number; y: number } | null = null;
  const cancel = () => {
    clearTimeout(timer);
    timer = undefined;
    origin = null;
  };
  return {
    start(x: number, y: number, onFire: () => void) {
      cancel();
      origin = { x, y };
      timer = setTimeout(() => {
        timer = undefined;
        origin = null;
        onFire();
      }, ms);
    },
    move(x: number, y: number) {
      if (origin && movedTooFar(x - origin.x, y - origin.y)) cancel();
    },
    cancel,
  };
}

export function pinchFontSize(start: number, startDistance: number, distance: number, min = PINCH_FONT_MIN, max = PINCH_FONT_MAX): number {
  if (startDistance <= 0 || distance <= 0) return start;
  const next = Math.round(start * (distance / startDistance));
  return clamp(next, min, max);
}

export function touchDistance(a: { clientX: number; clientY: number }, b: { clientX: number; clientY: number }): number {
  return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
}
