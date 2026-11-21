import { describe, expect, it } from 'vitest';
import { PANEL, RAIL, RAIL_W, SIDEBAR, clampWidth, dragWidth, leftBarBounds, leftBarDrag, leftBarMode, leftBarWidth } from './resize';

describe('pane widths', () => {
  it('falls back to the default for missing or broken values', () => {
    expect(clampWidth(undefined, SIDEBAR)).toBe(288);
    expect(clampWidth(Number.NaN, PANEL)).toBe(340);
  });

  it('clamps to the bounds', () => {
    expect(clampWidth(50, SIDEBAR)).toBe(SIDEBAR.min);
    expect(clampWidth(5000, PANEL)).toBe(PANEL.max);
    expect(clampWidth(300.6, SIDEBAR)).toBe(301);
  });

  it('grows the sidebar when dragged right and the panel when dragged left', () => {
    expect(dragWidth(288, 40, 'left', SIDEBAR)).toBe(328);
    expect(dragWidth(340, -60, 'right', PANEL)).toBe(400);
    expect(dragWidth(340, 400, 'right', PANEL)).toBe(PANEL.min);
  });
});

describe('collapsed left bar (live review rail)', () => {
  it('has the rail width and bounds while collapsed, the sidebar ones while open', () => {
    expect(leftBarWidth(false, 288)).toBe(RAIL_W);
    expect(leftBarWidth(true, 320)).toBe(320);
    expect(leftBarBounds(false)).toBe(RAIL);
    expect(leftBarBounds(true)).toBe(SIDEBAR);
  });

  it('stays collapsed at the rail width when dragged within it', () => {
    expect(leftBarDrag(false, RAIL_W, 288)).toEqual({ open: false, width: RAIL_W });
    expect(leftBarDrag(false, RAIL_W - 40, 288)).toEqual({ open: false, width: RAIL_W });
  });

  it('expands into the sidebar when dragged past the rail width, clamped to the sidebar bounds', () => {
    expect(leftBarDrag(false, RAIL_W + 1, 288)).toEqual({ open: true, width: SIDEBAR.min });
    expect(leftBarDrag(false, 400, 288)).toEqual({ open: true, width: 400 });
    expect(leftBarDrag(false, 9999, 288)).toEqual({ open: true, width: SIDEBAR.max });
  });

  it('keeps resizing the sidebar while it is open', () => {
    expect(leftBarDrag(true, 300, 300)).toEqual({ open: true, width: 300 });
    expect(leftBarDrag(true, 10, 300)).toEqual({ open: true, width: 10 });
  });
});

describe('left bar mode', () => {
  it('follows the sidebar setting outside a live review', () => {
    expect(leftBarMode(true, false, false)).toBe('sidebar');
    expect(leftBarMode(false, false, true)).toBe('none');
  });

  it('is the rail during a live review until it is expanded, whatever the sidebar setting', () => {
    expect(leftBarMode(true, true, false)).toBe('rail');
    expect(leftBarMode(false, true, false)).toBe('rail');
    expect(leftBarMode(false, true, true)).toBe('sidebar');
  });
});
