import { describe, expect, it } from 'vitest';
import css from '../app.css?raw';

function z(name: string): number {
  const m = css.match(new RegExp(`--z-${name}:\\s*(\\d+);`));
  if (!m) throw new Error(`--z-${name} missing`);
  return Number(m[1]);
}

describe('z-index ladder', () => {
  it('keeps the stacking values the components rely on', () => {
    expect(['shell-scrim', 'shell', 'modal', 'popover-scrim', 'popover', 'picker', 'tour', 'tour-card', 'toast'].map(z)).toEqual([40, 41, 50, 59, 60, 70, 80, 81, 100]);
  });

  it('stacks scrims under what they dim and the toast above everything', () => {
    expect(z('popover-scrim')).toBeLessThan(z('popover'));
    expect(z('shell-scrim')).toBeLessThan(z('shell'));
    expect(z('tour')).toBeLessThan(z('tour-card'));
    expect(z('toast')).toBeGreaterThan(z('tour-card'));
  });
});
