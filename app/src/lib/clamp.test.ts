import { describe, expect, it } from 'vitest';
import { clamp, clampFinite } from './clamp';

describe('clamp', () => {
  it('keeps a value inside the bounds', () => {
    expect(clamp(5, 1, 10)).toBe(5);
    expect(clamp(0, 1, 10)).toBe(1);
    expect(clamp(11, 1, 10)).toBe(10);
  });

  it('falls back to the lower bound for a non-finite value with clampFinite', () => {
    expect(clampFinite(NaN, 9, 24)).toBe(9);
    expect(clampFinite(Infinity, 9, 24)).toBe(9);
    expect(clampFinite(30, 9, 24)).toBe(24);
    expect(clampFinite(12, 9, 24)).toBe(12);
  });
});
