import { describe, expect, it } from 'vitest';
import { menuRows, pickItem, type MenuItem } from './menu';

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

describe('pickItem', () => {
  it('closes the menu and gives focus back before running the entry, so an entry that moves focus keeps it', () => {
    const calls: string[] = [];
    pickItem({ label: 'Paste', run: () => calls.push('run') }, () => calls.push('close'), () => calls.push('restore'));
    expect(calls).toEqual(['close', 'restore', 'run']);
  });

  it('restores focus for an entry without an action', () => {
    const calls: string[] = [];
    pickItem({ label: 'Nothing' }, () => calls.push('close'), () => calls.push('restore'));
    expect(calls).toEqual(['close', 'restore']);
  });
});
