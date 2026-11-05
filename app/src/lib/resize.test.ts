import { describe, expect, it } from 'vitest';
import { PANEL, SIDEBAR, clampWidth, dragWidth } from './resize';

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
