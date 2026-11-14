import { describe, expect, it } from 'vitest';
import { findHit, viewportFor } from './termfind';

const lines = ['make build', 'error: one', 'ok', 'Error: two', ''];

describe('terminal find', () => {
  it('starts from the bottom going up and wraps', () => {
    const a = findHit(lines, 'error', null, true);
    expect(a).toEqual({ row: 3, col: 0 });
    const b = findHit(lines, 'error', a, true);
    expect(b).toEqual({ row: 1, col: 0 });
    expect(findHit(lines, 'error', b, true)).toEqual({ row: 3, col: 0 });
    expect(findHit(lines, 'error', b, false)).toEqual({ row: 3, col: 0 });
    expect(findHit(lines, 'error', { row: 3, col: 0 }, false)).toEqual({ row: 1, col: 0 });
  });

  it('finds several hits on one line and returns null when absent', () => {
    const l = ['a-a-a'];
    expect(findHit(l, 'a', { row: 0, col: 2 }, false)).toEqual({ row: 0, col: 4 });
    expect(findHit(l, 'a', { row: 0, col: 2 }, true)).toEqual({ row: 0, col: 0 });
    expect(findHit(lines, 'nope', null, true)).toBeNull();
    expect(findHit(lines, '', null, true)).toBeNull();
  });

  it('centres the hit in the viewport, clamped to the buffer', () => {
    expect(viewportFor(100, 200, 20)).toEqual({ offset: 110, row: 10 });
    expect(viewportFor(215, 200, 20)).toEqual({ offset: 0, row: 15 });
    expect(viewportFor(2, 200, 20)).toEqual({ offset: 200, row: 2 });
  });
});
