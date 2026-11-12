import { describe, expect, it } from 'vitest';
import { Lru } from './lru';

describe('Lru', () => {
  it('evicts the least recently used entry past the bound', () => {
    const c = new Lru<string, number>(2);
    c.set('a', 1);
    c.set('b', 2);
    expect(c.get('a')).toBe(1);
    c.set('c', 3);
    expect(c.has('b')).toBe(false);
    expect(c.get('a')).toBe(1);
    expect(c.get('c')).toBe(3);
    expect(c.size).toBe(2);
  });

  it('deletes entries', () => {
    const c = new Lru<string, number>(4);
    c.set('a', 1);
    c.delete('a');
    expect(c.get('a')).toBeUndefined();
    expect(c.size).toBe(0);
  });

  it('keeps the total weight within the budget', () => {
    const c = new Lru<string, string>(10, 10, (v) => v.length);
    c.set('a', 'xxxx');
    c.set('b', 'yyyy');
    c.set('c', 'zzzz');
    expect(c.has('a')).toBe(false);
    expect(c.weight).toBe(8);
  });

  it('refuses a value heavier than the whole budget and drops the stale one', () => {
    const c = new Lru<string, string>(10, 5, (v) => v.length);
    c.set('a', 'abc');
    expect(c.set('a', 'too long value')).toBe(false);
    expect(c.has('a')).toBe(false);
    expect(c.weight).toBe(0);
  });

  it('replaces a key without double counting', () => {
    const c = new Lru<string, string>(10, 100, (v) => v.length);
    c.set('a', 'abc');
    c.set('a', 'abcdef');
    expect(c.size).toBe(1);
    expect(c.weight).toBe(6);
    c.delete('a');
    expect(c.weight).toBe(0);
  });
});
