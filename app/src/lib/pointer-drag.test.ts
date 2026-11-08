import { describe, expect, it, vi } from 'vitest';
import { releaseCapturedPointer } from './pointer-drag';

describe('releaseCapturedPointer', () => {
  it('releases the pointer held by the drag target', () => {
    const target = { hasPointerCapture: vi.fn(() => true), releasePointerCapture: vi.fn() };
    expect(releaseCapturedPointer(target, 7)).toBe(true);
    expect(target.hasPointerCapture).toHaveBeenCalledWith(7);
    expect(target.releasePointerCapture).toHaveBeenCalledWith(7);
  });

  it('does nothing when the target has no capture to release', () => {
    const target = { hasPointerCapture: vi.fn(() => false), releasePointerCapture: vi.fn() };
    expect(releaseCapturedPointer(target, 7)).toBe(false);
    expect(target.releasePointerCapture).not.toHaveBeenCalled();
    expect(releaseCapturedPointer(null, 7)).toBe(false);
    expect(releaseCapturedPointer(target, null)).toBe(false);
  });

  it('tolerates capture already being released by the browser', () => {
    const target = { hasPointerCapture: () => true, releasePointerCapture: () => { throw new Error('not captured'); } };
    expect(releaseCapturedPointer(target, 7)).toBe(false);
  });
});
