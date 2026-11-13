import { describe, expect, it, vi } from 'vitest';
import { guardAutoScroll } from './selection-scroll';

function host(scrollback: number, viewportY: number) {
  return {
    startAutoScroll: vi.fn(),
    stopAutoScroll: vi.fn(),
    wasmTerm: { getScrollbackLength: () => scrollback },
    terminal: { getViewportY: () => viewportY },
  };
}

describe('guardAutoScroll', () => {
  it('does not scroll up when there is no scrollback', () => {
    const h = host(0, 0);
    const start = h.startAutoScroll;
    guardAutoScroll(h);
    h.startAutoScroll(-1);
    expect(start).not.toHaveBeenCalled();
    expect(h.stopAutoScroll).toHaveBeenCalled();
  });

  it('does not scroll down while already at the bottom', () => {
    const h = host(40, 0);
    const start = h.startAutoScroll;
    guardAutoScroll(h);
    h.startAutoScroll(1);
    expect(start).not.toHaveBeenCalled();
    expect(h.stopAutoScroll).toHaveBeenCalled();
  });

  it('scrolls up when scrollback is above the viewport', () => {
    const h = host(40, 10);
    const start = h.startAutoScroll;
    guardAutoScroll(h);
    h.startAutoScroll(-1);
    expect(start).toHaveBeenCalledWith(-1);
  });

  it('scrolls down when the viewport is scrolled back', () => {
    const h = host(40, 10);
    const start = h.startAutoScroll;
    guardAutoScroll(h);
    h.startAutoScroll(1);
    expect(start).toHaveBeenCalledWith(1);
  });

  it('does not scroll up past the oldest line', () => {
    const h = host(40, 40);
    const start = h.startAutoScroll;
    guardAutoScroll(h);
    h.startAutoScroll(-1);
    expect(start).not.toHaveBeenCalled();
  });

  it('ignores a missing or incomplete selection manager', () => {
    expect(() => guardAutoScroll(undefined)).not.toThrow();
    expect(() => guardAutoScroll({})).not.toThrow();
  });
});
