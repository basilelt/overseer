import { describe, expect, it } from 'vitest';
import { activateTab, activeKey, closePane, closeTab, dropKey, EMPTY_LAYOUT, focusPane, openInFocused, setRatio, split, toggleSplit } from './workspace.svelte';

describe('workspace layout', () => {
  it('opens tabs in the focused pane and reuses existing ones', () => {
    let l = openInFocused(EMPTY_LAYOUT, 'a');
    l = openInFocused(l, 'b');
    expect(l.panes[0]).toEqual({ tabs: ['a', 'b'], active: 'b' });
    l = openInFocused(l, 'a');
    expect(l.panes[0].tabs).toEqual(['a', 'b']);
    expect(activeKey(l)).toBe('a');
  });

  it('splits into an empty focused pane and fills it from the selection', () => {
    let l = split(openInFocused(EMPTY_LAYOUT, 'a'), 'vertical');
    expect(l.split).toBe('vertical');
    expect(l.focused).toBe(1);
    l = openInFocused(l, 'b');
    expect(l.panes.map((p) => p.active)).toEqual(['a', 'b']);
    l = openInFocused(l, 'a');
    expect(l.focused).toBe(0);
    expect(split(l, 'horizontal').panes.length).toBe(2);
  });

  it('closes tabs, picking a neighbour, and collapses empty panes', () => {
    let l = ['a', 'b', 'c'].reduce(openInFocused, EMPTY_LAYOUT);
    l = activateTab(l, 0, 'b');
    l = closeTab(l, 0, 'b');
    expect(l.panes[0]).toEqual({ tabs: ['a', 'c'], active: 'c' });
    l = openInFocused(split(l, 'horizontal'), 'd');
    l = closeTab(l, 1, 'd');
    expect(l.split).toBe('none');
    expect(l.panes.length).toBe(1);
    expect(closeTab(l, 0, 'zz')).toBe(l);
  });

  it('closes panes, toggles split, clamps ratio and drops keys everywhere', () => {
    let l = toggleSplit(openInFocused(EMPTY_LAYOUT, 'a'));
    expect(l.panes.length).toBe(2);
    expect(focusPane(l, 5)).toBe(l);
    l = openInFocused(l, 'b');
    expect(closePane(l, 0).panes[0].tabs).toEqual(['b']);
    expect(toggleSplit(l).panes.length).toBe(1);
    expect(setRatio(l, 2).ratio).toBe(0.85);
    expect(setRatio(l, -1).ratio).toBe(0.15);
    expect(dropKey(l, 'b').panes.length).toBe(1);
  });
});
