import { describe, expect, it } from 'vitest';
import { pinchFontSize } from '../mobile/gestures';
import { FONT_SIZE_MAX, FONT_SIZE_MIN, PINCH_FONT_MAX, PINCH_FONT_MIN } from './terminal-font';

describe('terminal font bounds', () => {
  it('keeps the setting bounds inside the wider pinch bounds', () => {
    expect(PINCH_FONT_MIN).toBeLessThanOrEqual(FONT_SIZE_MIN);
    expect(PINCH_FONT_MAX).toBeGreaterThanOrEqual(FONT_SIZE_MAX);
  });

  it('clamps a pinch to the pinch bounds', () => {
    expect(pinchFontSize(13, 100, 1)).toBe(PINCH_FONT_MIN);
    expect(pinchFontSize(13, 100, 10000)).toBe(PINCH_FONT_MAX);
  });
});
