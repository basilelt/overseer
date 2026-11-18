import { describe, expect, it } from 'vitest';
import { indexBy } from './index-by';

describe('indexBy', () => {
  it('maps each key to its first element', () => {
    const list = [{ id: 'a', v: 1 }, { id: 'b', v: 2 }, { id: 'a', v: 3 }];
    const m = indexBy(list, 'id', (x) => x.id);
    expect(m.get('a')?.v).toBe(1);
    expect(m.get('b')?.v).toBe(2);
    expect(m.size).toBe(2);
  });

  it('skips elements without a key', () => {
    const list = [{ id: null }, { id: '' }, { id: 'x' }];
    expect([...indexBy(list, 'id', (x) => x.id).keys()]).toEqual(['x']);
  });

  it('reuses the map for the same list and name', () => {
    const list = [{ id: 'a' }];
    expect(indexBy(list, 'id', (x) => x.id)).toBe(indexBy(list, 'id', (x) => x.id));
  });

  it('keeps one map per name on the same list', () => {
    const list = [{ id: 'a', other: 'z' }];
    expect(indexBy(list, 'id', (x) => x.id).has('a')).toBe(true);
    expect(indexBy(list, 'other', (x) => x.other).has('z')).toBe(true);
  });

  it('rebuilds when the list grows', () => {
    const list = [{ id: 'a' }];
    indexBy(list, 'id', (x) => x.id);
    list.push({ id: 'b' });
    expect(indexBy(list, 'id', (x) => x.id).has('b')).toBe(true);
  });

  it('builds a separate map for another list', () => {
    expect(indexBy([{ id: 'a' }], 'id', (x) => x.id).has('a')).toBe(true);
    expect(indexBy([{ id: 'b' }], 'id', (x) => x.id).has('a')).toBe(false);
  });
});
