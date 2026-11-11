import { describe, expect, it } from 'vitest';
import { layoutKeyOf, moveSavedTab, moveTab, tabOrder } from './tab-order';
import { EMPTY_LAYOUT, type Layout } from './workspace.svelte';

const one = (tabs: string[], active: string | null = tabs[0] ?? null): Layout => ({ ...EMPTY_LAYOUT, panes: [{ tabs, active }] });
const two = (a: string[], b: string[]): Layout => ({ split: 'horizontal', panes: [{ tabs: a, active: a[0] ?? null }, { tabs: b, active: b[0] ?? null }], focused: 0, ratio: 0.5 });

describe('layoutKeyOf', () => {
  it('maps the "Other" row to its layout key and keeps workspace ids', () => {
    expect(layoutKeyOf('')).toBe('#other');
    expect(layoutKeyOf('w1')).toBe('w1');
  });
});

describe('tabOrder', () => {
  it('uses the live tabs for the workspace on screen', () => {
    expect(tabOrder('w1', 'w1', ['b', 'a'], new Map([['w1', one(['a', 'b'])]]), ['a', 'b'])).toEqual(['b', 'a']);
  });

  it('uses the cached layout of a workspace off screen, all panes in order, before its saved tabs', () => {
    expect(tabOrder('w2', 'w1', [], new Map([['w2', two(['c'], ['a', 'b'])]]), ['a', 'b', 'c'])).toEqual(['c', 'a', 'b']);
  });

  it('falls back to the saved tabs, then to none', () => {
    expect(tabOrder('w2', 'w1', [], new Map(), ['b', 'a'])).toEqual(['b', 'a']);
    expect(tabOrder('', 'w1', [], new Map(), undefined)).toEqual([]);
  });

  it('reads the cached layout of "Other" under its layout key', () => {
    expect(tabOrder('', 'w1', [], new Map([['#other', one(['x', 'y'])]]), undefined)).toEqual(['x', 'y']);
  });
});

describe('moveTab', () => {
  it('moves a tab before or after another one of the same pane and keeps the active tab', () => {
    const l = one(['a', 'b', 'c'], 'c');
    expect(moveTab(l, 'c', 'a', 'before').panes[0]).toEqual({ tabs: ['c', 'a', 'b'], active: 'c' });
    expect(moveTab(l, 'a', 'b', 'after').panes[0]).toEqual({ tabs: ['b', 'a', 'c'], active: 'c' });
  });

  it('opens a session without a tab next to the anchor, without activating it', () => {
    expect(moveTab(one(['a', 'b'], 'a'), 'x', 'b', 'before').panes[0]).toEqual({ tabs: ['a', 'x', 'b'], active: 'a' });
  });

  it('does nothing when the anchor has no tab or is the moved tab', () => {
    const l = one(['a', 'b']);
    expect(moveTab(l, 'a', 'zz', 'before')).toBe(l);
    expect(moveTab(l, 'a', 'a', 'after')).toBe(l);
  });

  it('moves a tab into the pane of the anchor and hands the old pane its next tab', () => {
    const l = two(['a', 'b'], ['c']);
    const out = moveTab(l, 'a', 'c', 'after');
    expect(out.panes).toEqual([
      { tabs: ['b'], active: 'b' },
      { tabs: ['c', 'a'], active: 'c' },
    ]);
  });

  it('closes a split pane left without tabs', () => {
    const out = moveTab(two(['a'], ['c']), 'a', 'c', 'before');
    expect(out.panes).toEqual([{ tabs: ['a', 'c'], active: 'c' }]);
    expect(out.split).toBe('none');
  });
});

describe('moveSavedTab', () => {
  it('moves or inserts a key next to the anchor', () => {
    expect(moveSavedTab(['a', 'b', 'c'], 'c', 'a', 'before')).toEqual(['c', 'a', 'b']);
    expect(moveSavedTab(['a', 'b'], 'x', 'a', 'after')).toEqual(['a', 'x', 'b']);
  });

  it('gives null when the anchor has no tab or is the moved key', () => {
    expect(moveSavedTab(['a'], 'a', 'zz', 'before')).toBeNull();
    expect(moveSavedTab(['a'], 'a', 'a', 'before')).toBeNull();
  });
});
