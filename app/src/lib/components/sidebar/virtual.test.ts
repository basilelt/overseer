import { describe, expect, it } from 'vitest';
import { groupItems, type SidebarItem } from '../../grouping';
import { CARD_H, flatten, HEADER_H, moveSelection, windowRows } from './virtual';

const item = (key: string, group: SidebarItem['group'], title = key): SidebarItem => ({ key, kind: 'claude', group, title, at: '2026-10-29T10:00:00Z' });
const groups = groupItems([item('a', 'working'), item('b', 'working', 'Bravo'), item('c', 'held')]);

describe('flatten', () => {
  it('skips empty groups and hides collapsed cards', () => {
    const rows = flatten(groups, new Set(['held']));
    expect(rows.map((r) => (r.type === 'header' ? `#${r.id}:${r.count}` : r.item.key))).toEqual(['#working:2', 'a', 'b', '#held:1']);
  });

  it('filters by text', () => {
    const rows = flatten(groups, new Set(), 'brav');
    expect(rows.map((r) => (r.type === 'header' ? r.id : r.item.key))).toEqual(['working', 'b']);
  });
});

describe('windowRows', () => {
  const many = groupItems(Array.from({ length: 200 }, (_, i) => item(`k${i}`, 'idle')));
  const rows = flatten(many, new Set());

  it('computes offsets and total height', () => {
    const w = windowRows(rows, 0, 300, 0);
    expect(w.total).toBe(HEADER_H + 200 * CARD_H);
    expect(w.offsets[2]).toBe(HEADER_H + CARD_H);
    expect(w.start).toBe(0);
    expect(w.end).toBe(1 + Math.ceil((300 - HEADER_H) / CARD_H));
  });

  it('renders only the visible slice plus overscan', () => {
    const top = HEADER_H + 100 * CARD_H;
    const w = windowRows(rows, top, 300, 2);
    expect(w.start).toBe(101 - 2);
    expect(w.end - w.start).toBeLessThan(12);
  });
});

describe('moveSelection', () => {
  const rows = flatten(groups, new Set());
  it('walks cards and clamps at the ends', () => {
    expect(moveSelection(rows, null, 1)).toBe('a');
    expect(moveSelection(rows, null, -1)).toBe('c');
    expect(moveSelection(rows, 'a', 1)).toBe('b');
    expect(moveSelection(rows, 'b', 1)).toBe('c');
    expect(moveSelection(rows, 'c', 1)).toBe('c');
    expect(moveSelection(rows, 'a', -1)).toBe('a');
    expect(moveSelection([], 'a', 1)).toBeNull();
  });
});
