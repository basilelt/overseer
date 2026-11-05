import { afterEach, describe, expect, it } from 'vitest';
import type { SidebarItem } from './grouping';
import { applyShortcut, newTabKey, registerTabKind, stripScroll, TAB_KINDS, tabInfo, tabTitle, type TabKindDef } from './tabs';
import { addTabs, layoutFromTabs, renameTab, reorderTab, split, switchLayout, workspace } from './workspace.svelte';

describe('tab shortcuts', () => {
  it('selects by index, wraps prev/next and sends 9 to the last tab', () => {
    const tabs = ['a', 'b', 'c'];
    expect(applyShortcut(tabs, 'a', { type: 'index', index: 1 })).toBe('b');
    expect(applyShortcut(tabs, 'a', { type: 'index', index: 5 })).toBeNull();
    expect(applyShortcut(tabs, 'a', { type: 'index', index: 8 })).toBe('c');
    expect(applyShortcut(tabs, 'a', { type: 'prev' })).toBe('c');
    expect(applyShortcut(tabs, 'c', { type: 'next' })).toBe('a');
    expect(applyShortcut([], null, { type: 'next' })).toBeNull();
  });
});

describe('tab kinds', () => {
  const items: SidebarItem[] = [
    { key: 'line:a', kind: 'codex', group: 'idle', title: 'A', at: '', lineId: 'a' },
    { key: 'term:s', kind: 'shell', group: 'working', title: 'zsh', at: '', termId: 's' },
  ];

  it('resolves agent, shell, browser and review tabs', () => {
    expect(tabInfo('line:a', items)).toMatchObject({ kind: 'agent', arg: 'a' });
    expect(tabInfo('term:s', items)).toMatchObject({ kind: 'shell', arg: 's' });
    expect(tabInfo('line:a', items, () => 'chat')).toMatchObject({ kind: 'chat', key: 'line:a', arg: 'a' });
    expect(tabInfo('term:s', items, () => 'chat')).toMatchObject({ kind: 'shell' });
    expect(tabInfo(newTabKey('browser', 'https://x.dev/a'), items)).toMatchObject({ kind: 'browser', arg: 'https://x.dev/a' });
    expect(tabInfo(newTabKey('review', 'a'), items)).toMatchObject({ kind: 'review', arg: 'a' });
    expect(tabInfo('line:zz', items)).toBeNull();
  });
});

describe('workspace layouts', () => {
  it('appends new tabs, renames a term tab to its line and reorders within a pane', () => {
    let l = layoutFromTabs(['a', 'b'], 'b');
    expect(l.panes[0]).toEqual({ tabs: ['a', 'b'], active: 'b' });
    l = addTabs(l, ['b', 'c']);
    expect(l.panes[0].tabs).toEqual(['a', 'b', 'c']);
    l = renameTab(l, 'b', 'line:b');
    expect(l.panes[0]).toEqual({ tabs: ['a', 'line:b', 'c'], active: 'line:b' });
    l = reorderTab(l, 0, 'c', 0);
    expect(l.panes[0].tabs).toEqual(['c', 'a', 'line:b']);
    l = split(l, 'horizontal');
    l = addTabs(l, ['d']);
    expect(l.panes[1].tabs).toEqual(['d']);
  });

  it('keeps one layout per workspace', () => {
    workspace.layout = layoutFromTabs(['x']);
    switchLayout('w1', 'w2', () => layoutFromTabs(['y']));
    expect(workspace.layout.panes[0].tabs).toEqual(['y']);
    switchLayout('w2', 'w1', () => layoutFromTabs([]));
    expect(workspace.layout.panes[0].tabs).toEqual(['x']);
  });
});

describe('project tab kinds', () => {
  it('opens a graph and a worktrees tab per project path', () => {
    const p = '/home/u/src/x';
    expect(tabInfo(newTabKey('graph', p), [])).toMatchObject({ kind: 'graph', arg: p });
    expect(tabInfo(newTabKey('worktrees', p), [])).toMatchObject({ kind: 'worktrees', arg: p });
    expect(newTabKey('graph', p)).not.toBe(newTabKey('worktrees', p));
    expect(typeof TAB_KINDS.graph?.load).toBe('function');
    expect(typeof TAB_KINDS.worktrees?.load).toBe('function');
  });
});

describe('stripScroll', () => {
  const view = { left: 100, width: 300, scroll: 50 };
  it('leaves a visible tab alone', () => {
    expect(stripScroll(view, { left: 150, width: 80 })).toBeNull();
  });
  it('scrolls left to reveal a tab cut off at the start', () => {
    expect(stripScroll(view, { left: 80, width: 80 })).toBe(30);
    expect(stripScroll({ ...view, scroll: 10 }, { left: 40, width: 80 })).toBe(0);
  });
  it('scrolls right just enough to show a tab past the end', () => {
    expect(stripScroll(view, { left: 380, width: 80 })).toBe(110);
  });
});

describe('tab titles', () => {
  const label = (kind: string, arg: string) => `${kind}:${arg}`;

  it('prefers the sidebar title and falls back to a label for the kind', () => {
    const item: SidebarItem = { key: 'line:a', kind: 'codex', group: 'idle', title: 'Fix login', at: '', lineId: 'a' };
    expect(tabTitle(tabInfo('line:a', [item])!, label)).toBe('Fix login');
    expect(tabTitle(tabInfo(newTabKey('system', 'cpu'), [])!, label)).toBe('system:cpu');
  });
});

describe('registered tab kinds', () => {
  const original = TAB_KINDS.usage as TabKindDef;
  afterEach(() => registerTabKind('usage', original));

  it('knows system, usage and PR review tabs, each with a lazy view', () => {
    for (const kind of ['system', 'usage', 'prs', 'review'] as const) {
      expect(tabInfo(newTabKey(kind, 'x'), [])).toMatchObject({ kind, arg: 'x' });
      expect(typeof TAB_KINDS[kind]?.load).toBe('function');
    }
    expect(TAB_KINDS.browser?.load).toBeUndefined();
  });

  it('lets a kind be re-registered with a new prefix', () => {
    registerTabKind('usage', { prefix: 'stats:' });
    expect(newTabKey('usage', 'today')).toBe('stats:today');
    expect(tabInfo('stats:today', [])).toMatchObject({ kind: 'usage', arg: 'today' });
    expect(tabInfo('usage:today', [])).toBeNull();
  });

  it('keeps an argument that itself contains the prefix', () => {
    expect(tabInfo(newTabKey('browser', 'browser:x'), [])).toMatchObject({ kind: 'browser', arg: 'browser:x' });
  });
});
