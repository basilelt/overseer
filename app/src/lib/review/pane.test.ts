import { describe, expect, it } from 'vitest';
import { EMPTY_LAYOUT, type Layout } from '../workspace/workspace.svelte';
import { closeReviews, follow, openBeside, reviewMode } from './pane';

const one = (tabs: string[], active = tabs[0] ?? null): Layout => ({ ...EMPTY_LAYOUT, panes: [{ tabs, active }] });
const two = (a: string[], b: string[], split: Layout['split'] = 'horizontal'): Layout => ({ split, panes: [{ tabs: a, active: a[0] ?? null }, { tabs: b, active: b[0] ?? null }], focused: 0, ratio: 0.5 });

describe('openBeside', () => {
  it('splits right and keeps the agent pane focused', () => {
    const l = openBeside(one(['line:a', 'line:b'], 'line:a'), 0, 'review:line:a');
    expect(l.split).toBe('horizontal');
    expect(l.panes[1]).toEqual({ tabs: ['review:line:a'], active: 'review:line:a' });
    expect(l.panes[0].active).toBe('line:a');
    expect(l.focused).toBe(0);
  });

  it('reuses an existing split and replaces the side review in place', () => {
    const l = openBeside(two(['line:a'], ['browser:x', 'review:line:a']), 0, 'review:line:b', 'review:line:a');
    expect(l.panes[1]).toEqual({ tabs: ['browser:x', 'review:line:b'], active: 'review:line:b' });
    expect(l.panes.length).toBe(2);
    expect(l.focused).toBe(0);
  });

  it('adds to the other pane when there is no side review, and opens left of an agent on the right', () => {
    const l = openBeside({ ...two(['browser:x'], ['line:a']), focused: 1 }, 1, 'review:line:a');
    expect(l.panes[0]).toEqual({ tabs: ['browser:x', 'review:line:a'], active: 'review:line:a' });
    expect(l.focused).toBe(1);
  });

  it('activates a review that is already open instead of duplicating it', () => {
    const l = openBeside(two(['line:a'], ['review:line:a', 'browser:x']), 0, 'review:line:a');
    expect(l.panes[1].tabs).toEqual(['review:line:a', 'browser:x']);
    expect(l.panes[1].active).toBe('review:line:a');
  });
});

describe('follow', () => {
  const base = two(['line:a', 'line:b'], ['review:line:a']);

  it('swaps the side review to the selected agent in place', () => {
    const l = follow(base, 'review:line:a', false, 'review:line:b');
    expect(l.panes[1]).toEqual({ tabs: ['review:line:b'], active: 'review:line:b' });
    expect(l.panes[0]).toBe(base.panes[0]);
  });

  it('does nothing when pinned, unchanged, or the side review was closed', () => {
    expect(follow(base, 'review:line:a', true, 'review:line:b')).toBe(base);
    expect(follow(base, 'review:line:a', false, 'review:line:a')).toBe(base);
    expect(follow(base, 'review:line:z', false, 'review:line:b')).toBe(base);
    expect(follow(base, null, false, 'review:line:b')).toBe(base);
  });

  it('keeps one tab when the wanted review is already there, and never duplicates across panes', () => {
    const l = follow(two(['line:a'], ['review:line:a', 'review:line:b']), 'review:line:a', false, 'review:line:b');
    expect(l.panes[1]).toEqual({ tabs: ['review:line:b'], active: 'review:line:b' });
    const other = two(['line:a', 'review:line:b'], ['review:line:a']);
    expect(follow(other, 'review:line:a', false, 'review:line:b')).toBe(other);
  });
});

describe('reviewMode', () => {
  it('reports side, tab or off', () => {
    const l = two(['line:a', 'review:line:c'], ['review:line:a']);
    expect(reviewMode(l, 'review:line:a', 'review:line:b')).toBe('side');
    expect(reviewMode(l, null, 'review:line:c')).toBe('tab');
    expect(reviewMode(l, 'review:line:z', 'review:line:b')).toBe('off');
  });

  it('closing the side review collapses the split', () => {
    const l = closeReviews(two(['line:a'], ['review:line:a']), ['review:line:a', null]);
    expect(l.panes.length).toBe(1);
    expect(l.panes[0].tabs).toEqual(['line:a']);
  });
});
