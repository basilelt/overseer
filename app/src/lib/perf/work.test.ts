import { describe, expect, it } from 'vitest';
import { countWork, counted, growth } from './work';

const nums = (n: number) => Array.from({ length: n }, (_, i) => i);

describe('work counter', () => {
  it('counts nothing outside a measured run and restores the built-ins afterwards', () => {
    const find = Array.prototype.find;
    const indexOf = String.prototype.indexOf;
    const a = nums(10);
    const n = countWork(() => void a.find((x) => x === 9));
    expect(n).toBeGreaterThanOrEqual(10);
    expect(n).toBeLessThan(13);
    expect(Array.prototype.find).toBe(find);
    expect(String.prototype.indexOf).toBe(indexOf);
    expect(countWork(() => {})).toBe(0);
  });

  it('restores the built-ins when the measured run throws', () => {
    const map = Map.prototype.get;
    expect(() => countWork(() => { throw new Error('boom'); })).toThrow('boom');
    expect(Map.prototype.get).toBe(map);
  });

  it('sees a linear scan per item as quadratic and one pass as linear', () => {
    const quad = (n: number) => () => { const a = nums(n); return () => void a.forEach((x) => a.indexOf(x)); };
    const lin = (n: number) => () => { const a = nums(n); return () => void a.forEach((x) => x + 1); };
    expect(growth(quad(100), quad(1000)).ratio).toBeGreaterThan(50);
    expect(growth(lin(100), lin(1000)).ratio).toBeLessThan(11);
  });

  it('counts for-of steps, Map lookups, string scans and reads on a counted input', () => {
    expect(countWork(() => { for (const x of nums(5)) void x; })).toBeGreaterThanOrEqual(5);
    expect(countWork(() => { const m = new Map([[1, 2]]); m.get(1); m.has(1); })).toBeGreaterThanOrEqual(2);
    expect(countWork(() => void 'a\nb\nc'.split('\n'))).toBe(6);
    const raw = { a: { b: 1 } };
    const o = counted(raw);
    expect(countWork(() => void o.a.b)).toBe(2);
    expect(counted(raw)).toBe(o);
  });
});
