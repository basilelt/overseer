import { clamp } from './clamp';

export interface Bounds {
  min: number;
  max: number;
  initial: number;
}

export const SIDEBAR: Bounds = { min: 200, max: 560, initial: 288 };
export const PANEL: Bounds = { min: 260, max: 760, initial: 340 };

// While a live review is on, the left bar collapses to a rail of session icons.
export const RAIL_W = 52;
export const RAIL: Bounds = { min: RAIL_W, max: SIDEBAR.max, initial: SIDEBAR.initial };

export function clampWidth(w: number | undefined, b: Bounds): number {
  if (typeof w !== 'number' || !Number.isFinite(w)) return b.initial;
  return Math.round(clamp(w, b.min, b.max));
}

export function dragWidth(start: number, dx: number, side: 'left' | 'right', b: Bounds): number {
  return clampWidth(side === 'left' ? start + dx : start - dx, b);
}

export interface LeftBar {
  open: boolean;
  width: number;
}

export function leftBarWidth(open: boolean, sidebarW: number): number {
  return open ? sidebarW : RAIL_W;
}

export function leftBarBounds(open: boolean): Bounds {
  return open ? SIDEBAR : RAIL;
}

// A drag on the left bar either resizes the open sidebar or, when it starts on the
// collapsed rail, expands past the rail width back into the sidebar.
export function leftBarDrag(open: boolean, w: number, sidebarW: number): LeftBar {
  if (open) return { open: true, width: w };
  return w > RAIL_W ? { open: true, width: clampWidth(w, SIDEBAR) } : { open: false, width: RAIL_W };
}

export type LeftBarMode = 'sidebar' | 'rail' | 'none';

export function leftBarMode(sidebarOpen: boolean, live: boolean, expanded: boolean): LeftBarMode {
  if (live) return expanded ? 'sidebar' : 'rail';
  return sidebarOpen ? 'sidebar' : 'none';
}
