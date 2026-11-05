import { describe, expect, it } from 'vitest';
import { age } from './time';

const now = Date.parse('2026-10-29T12:00:00Z');
const ago = (ms: number) => new Date(now - ms).toISOString();

describe('age', () => {
  it('formats minutes, hours and days', () => {
    expect(age(ago(10_000), now)).toBe('now');
    expect(age(ago(5 * 60_000), now)).toBe('5m');
    expect(age(ago(3 * 3600_000), now)).toBe('3h');
    expect(age(ago(50 * 3600_000), now)).toBe('2d');
  });

  it('is empty for bad dates', () => {
    expect(age('nope', now)).toBe('');
  });
});
