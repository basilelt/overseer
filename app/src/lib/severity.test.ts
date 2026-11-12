import { describe, expect, it } from 'vitest';
import { level, worst } from './severity';

describe('severity', () => {
  it('rates a percentage against the warn and crit thresholds', () => {
    expect(level(79.9)).toBe('ok');
    expect(level(80)).toBe('warn');
    expect(level(94.9)).toBe('warn');
    expect(level(95)).toBe('crit');
    expect(level(150)).toBe('crit');
  });

  it('treats a missing or non-finite percentage as ok', () => {
    expect(level(null)).toBe('ok');
    expect(level(undefined)).toBe('ok');
    expect(level(NaN)).toBe('ok');
    expect(level(Infinity)).toBe('ok');
  });

  it('takes the worst of several levels', () => {
    expect(worst([])).toBe('ok');
    expect(worst(['ok', 'warn'])).toBe('warn');
    expect(worst(['warn', 'crit', 'ok'])).toBe('crit');
  });
});
