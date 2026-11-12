import { afterEach, describe, expect, it } from 'vitest';
import { command } from '../commands/commands';
import { pinchFontSize } from '../mobile/gestures';
import { saveSettings, settings } from '../settings/settings.svelte';
import { FONT_SIZE_MAX, FONT_SIZE_MIN } from './terminal-font';

const saved = settings.fontSize;
afterEach(() => saveSettings({ fontSize: saved }));

describe('terminal font bounds', () => {
  it('are 8 and 28 for every way of changing the size', () => {
    expect([FONT_SIZE_MIN, FONT_SIZE_MAX]).toEqual([8, 28]);
  });

  it('clamps a pinch to the bounds', () => {
    expect(pinchFontSize(13, 100, 1)).toBe(FONT_SIZE_MIN);
    expect(pinchFontSize(13, 100, 10000)).toBe(FONT_SIZE_MAX);
  });

  it('lets the keyboard commands reach both bounds and stop there', () => {
    saveSettings({ fontSize: FONT_SIZE_MAX - 1 });
    command('font-up')!.run();
    command('font-up')!.run();
    expect(settings.fontSize).toBe(28);
    saveSettings({ fontSize: FONT_SIZE_MIN + 1 });
    command('font-down')!.run();
    command('font-down')!.run();
    expect(settings.fontSize).toBe(8);
  });

  it('accepts a stored size that only the new bounds allow', () => {
    saveSettings({ fontSize: 27 });
    command('font-down')!.run();
    expect(settings.fontSize).toBe(26);
  });
});
