import { clamp } from './clamp';

export interface Bounds {
  min: number;
  max: number;
  initial: number;
}

export const SIDEBAR: Bounds = { min: 200, max: 560, initial: 288 };
export const PANEL: Bounds = { min: 260, max: 760, initial: 340 };

export function clampWidth(w: number | undefined, b: Bounds): number {
  if (typeof w !== 'number' || !Number.isFinite(w)) return b.initial;
  return Math.round(clamp(w, b.min, b.max));
}

export function dragWidth(start: number, dx: number, side: 'left' | 'right', b: Bounds): number {
  return clampWidth(side === 'left' ? start + dx : start - dx, b);
}
