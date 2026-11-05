export const LONG_PRESS_MS = 450;
export const SWIPE_MIN_PX = 56;
export const MOVE_TOLERANCE_PX = 10;

export function swipeDirection(dx: number, dy: number, min = SWIPE_MIN_PX): -1 | 0 | 1 {
  if (Math.abs(dx) < min || Math.abs(dx) < Math.abs(dy) * 1.5) return 0;
  return dx < 0 ? 1 : -1;
}

export function movedTooFar(dx: number, dy: number, tolerance = MOVE_TOLERANCE_PX): boolean {
  return Math.hypot(dx, dy) > tolerance;
}

export const FONT_MIN = 8;
export const FONT_MAX = 28;

export function pinchFontSize(start: number, startDistance: number, distance: number, min = FONT_MIN, max = FONT_MAX): number {
  if (startDistance <= 0 || distance <= 0) return start;
  const next = Math.round(start * (distance / startDistance));
  return Math.max(min, Math.min(max, next));
}

export function touchDistance(a: { clientX: number; clientY: number }, b: { clientX: number; clientY: number }): number {
  return Math.hypot(a.clientX - b.clientX, a.clientY - b.clientY);
}
