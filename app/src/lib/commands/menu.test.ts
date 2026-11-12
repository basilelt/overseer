import { describe, expect, it } from 'vitest';
import { menuRows, type MenuItem } from './menu';

const items: MenuItem[] = [{ label: 'a' }, { label: 'more', more: [{ label: 'x', disabled: true }, { label: 'y', disabled: true }] }, { label: '', separator: true }, { label: 'b' }];
const labels = (exp: number[]) => menuRows(items, new Set(exp)).map((r) => `${r.child ? '  ' : ''}${r.item.label}`);

describe('menuRows', () => {
  it('keeps a group folded until its row is expanded', () => {
    expect(labels([])).toEqual(['a', 'more', '', 'b']);
  });

  it('unfolds the group children right under their row', () => {
    expect(labels([1])).toEqual(['a', 'more', '  x', '  y', '', 'b']);
    expect(menuRows(items, new Set([1]))[2]).toMatchObject({ index: 1, child: true });
  });

  it('drops a group with no children', () => {
    expect(menuRows([{ label: 'a' }, { label: 'none', more: [] }], new Set([1])).map((r) => r.item.label)).toEqual(['a']);
  });
});
