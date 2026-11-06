import { describe, expect, it } from 'vitest';
import { closedShells, MAX_CLOSED, popReopenable, pushClosed, rememberShell } from './closed-tabs';
import { closePaneAt, closeTabAt, forgetLayouts, layoutFromTabs, paneInDirection, split, switchLayout, takeClosed, workspace } from './workspace.svelte';

describe('closed tab stack', () => {
  it('keeps the last ten, newest first, without duplicates', () => {
    let s: string[] = [];
    for (let i = 0; i < 12; i++) s = pushClosed(s, `k${i}`);
    expect(s).toHaveLength(MAX_CLOSED);
    expect(s[0]).toBe('k11');
    expect(s.at(-1)).toBe('k2');
    expect(pushClosed(['a', 'b', 'c'], 'b')).toEqual(['b', 'a', 'c']);
  });

  it('skips tabs that cannot come back and maps renamed ones', () => {
    const r = popReopenable(['term:gone', 'term:x', 'line:a'], (k) => (k === 'term:gone' ? null : k === 'term:x' ? 'line:x' : k));
    expect(r).toEqual({ key: 'line:x', rest: ['line:a'] });
    expect(popReopenable(['a'], () => null)).toEqual({ key: null, rest: [] });
  });

  it('records closes per workspace layout', () => {
    forgetLayouts();
    switchLayout('#status', 'ws1', () => layoutFromTabs(['line:a', 'term:b'], 'term:b'));
    closeTabAt(0, 'term:b');
    closeTabAt(0, 'line:missing');
    expect(workspace.layout.panes[0]).toEqual({ tabs: ['line:a'], active: 'line:a' });
    closeTabAt(0, 'line:a');
    expect(workspace.layout.panes[0]).toEqual({ tabs: [], active: null });
    switchLayout('ws1', 'ws2', () => layoutFromTabs(['x']));
    expect(takeClosed((k) => k)).toBeNull();
    switchLayout('ws2', 'ws1', () => layoutFromTabs([]));
    expect(takeClosed((k) => k)).toBe('line:a');
    expect(takeClosed((k) => k)).toBe('term:b');
    expect(takeClosed((k) => k)).toBeNull();
  });
});

describe('closing a pane', () => {
  it('records every tab of the pane so they can be reopened', () => {
    forgetLayouts();
    switchLayout('#status', 'ws1', () => ({ ...layoutFromTabs(['a']), split: 'horizontal', panes: [{ tabs: ['a'], active: 'a' }, { tabs: ['b', 'c'], active: 'c' }], focused: 1 }));
    closePaneAt(1);
    expect(workspace.layout.panes).toEqual([{ tabs: ['a'], active: 'a' }]);
    expect(takeClosed((k) => k)).toBe('c');
    expect(takeClosed((k) => k)).toBe('b');
    expect(takeClosed((k) => k)).toBeNull();
  });

  it('keeps the last pane open', () => {
    forgetLayouts();
    switchLayout('#status', 'ws1', () => layoutFromTabs(['a']));
    closePaneAt(0);
    expect(workspace.layout.panes).toEqual([{ tabs: ['a'], active: 'a' }]);
    expect(takeClosed((k) => k)).toBeNull();
  });
});

describe('closed shells', () => {
  it('remembers the latest folder per tab and stays bounded', () => {
    forgetLayouts();
    rememberShell('term:a', '/one');
    rememberShell('term:a', '/two');
    expect(closedShells.get('term:a')).toBe('/two');
    for (let i = 0; i < MAX_CLOSED * 6; i++) rememberShell(`term:${i}`, '/x');
    expect(closedShells.size).toBe(MAX_CLOSED * 5);
    expect(closedShells.has('term:a')).toBe(false);
    forgetLayouts();
    expect(closedShells.size).toBe(0);
  });
});

describe('pane focus by direction', () => {
  it('moves along the split axis only', () => {
    const one = layoutFromTabs(['a']);
    expect(paneInDirection(one, 'right')).toBeNull();
    const row = split(one, 'horizontal');
    expect(row.focused).toBe(1);
    expect(paneInDirection(row, 'left')).toBe(0);
    expect(paneInDirection(row, 'right')).toBeNull();
    expect(paneInDirection(row, 'up')).toBeNull();
    const col = { ...split(one, 'vertical'), focused: 0 };
    expect(paneInDirection(col, 'down')).toBe(1);
    expect(paneInDirection(col, 'left')).toBeNull();
  });
});
