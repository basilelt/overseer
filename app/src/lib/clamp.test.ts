import { describe, expect, it, vi } from 'vitest';
import { clamp, clampFinite, commitNumber } from './clamp';

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

describe('commitNumber', () => {
  const field = (typed: string) => ({ value: typed, valueAsNumber: Number(typed) });
  const size = (v: number) => clampFinite(v, 8, 28);

  it('shows the stored value in the field when it differs from what was typed', () => {
    const input = field('99');
    const save = vi.fn();
    commitNumber(input, size, save);
    expect(save).toHaveBeenCalledWith(28);
    expect(input.value).toBe('28');
  });

  it('keeps an in-range value as typed and saves it', () => {
    const input = field('14');
    const save = vi.fn();
    commitNumber(input, size, save);
    expect(save).toHaveBeenCalledWith(14);
    expect(input.value).toBe('14');
  });

  it('falls back to the minimum when the field was emptied', () => {
    const input = { value: '', valueAsNumber: NaN };
    commitNumber(input, size, vi.fn());
    expect(input.value).toBe('8');
  });
});
