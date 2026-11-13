import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SidebarItem } from '../session/grouping';
import type { Term } from '../types';
import { applyShortcut, coarsePointer, focusSession, followTerm, newTabKey, refocusReview, rememberTerms, registerTabKind, stripScroll, TAB_KINDS, tabInfo, tabTitle, type TabKindDef } from './tabs';
import { addTabs, layoutFromTabs, renameTab, reorderTab, split, switchLayout, workspace } from '../workspace/workspace.svelte';

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
    { key: 'line:a', kind: 'codex', group: 'ready', state: 'open', title: 'A', at: '', lineId: 'a' },
    { key: 'term:s', kind: 'shell', group: 'running', state: 'running', title: 'zsh', at: '', termId: 's' },
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
    const item: SidebarItem = { key: 'line:a', kind: 'codex', group: 'ready', state: 'open', title: 'Fix login', at: '', lineId: 'a' };
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

describe('followTerm', () => {
  const line = { key: 'line:a', lineId: 'a', termId: 't1', kind: 'agent', group: 'active', title: '', at: '' } as unknown as SidebarItem;
  const bound = [{ id: 't1', lineId: 'a' } as Term];
  const shell = { key: 'term:t1', termId: 't1', kind: 'shell', group: 'active', title: '', at: '' } as unknown as SidebarItem;

  it('moves a closed session tab to the shell left in its term', () => {
    const termOf = new Map<string, string>();
    rememberTerms(termOf, [line], bound);
    expect(followTerm('line:a', [shell], termOf)).toBe('term:t1');
  });

  it('follows the shell even while the held line is still listed', () => {
    const termOf = new Map<string, string>();
    rememberTerms(termOf, [line], bound);
    expect(followTerm('line:a', [{ ...line, group: 'paused', state: 'paused' }, shell], termOf)).toBe('term:t1');
  });

  it('does not remember a line whose term was already detached', () => {
    const termOf = new Map<string, string>();
    rememberTerms(termOf, [line], [{ id: 't1' } as Term]);
    expect(followTerm('line:a', [line, shell], termOf)).toBeNull();
  });

  it('finds nothing when the term is gone or still bound to a line', () => {
    const termOf = new Map<string, string>();
    rememberTerms(termOf, [line], bound);
    expect(followTerm('line:a', [], termOf)).toBeNull();
    expect(followTerm('line:a', [{ ...line, key: 'line:b', lineId: 'b' }], termOf)).toBeNull();
    expect(followTerm('line:z', [shell], termOf)).toBeNull();
    expect(followTerm('term:t1', [shell], termOf)).toBeNull();
  });
});

describe('focusSession', () => {
  type Fake = { id: string; isConnected: boolean; focus: (o?: unknown) => void };
  const fake = (id: string, connected = true): Fake => ({ id, isConnected: connected, focus() { active.el = this; } });
  const active: { el: unknown } = { el: null };
  const pane = (found: Record<string, Fake | null>) => ({ querySelector: (sel: string) => found[sel] ?? null }) as unknown as Element;
  const COMPOSER = '.chat .composer textarea';
  const TERM = '.keep:not(.hidden) [data-terminal]';

  it('moves focus from the clicked tab to the visible terminal', () => {
    const term = fake('term');
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(focusSession(pane({ [TERM]: term }))).toBe(true);
    expect(active.el).toBe(term);
  });

  it('prefers the chat composer in the chat view', () => {
    const box = fake('box');
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(focusSession(pane({ [COMPOSER]: box, [TERM]: fake('term') }))).toBe(true);
    expect(active.el).toBe(box);
  });

  it('moves focus from the clicked tab into a review, so J and K work', () => {
    const review = fake('review');
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(focusSession(pane({ '.review': review }))).toBe(true);
    expect(active.el).toBe(review);
  });

  it('leaves focus alone when the pane has no terminal or composer, or it is detached', () => {
    active.el = 'tab';
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(focusSession(pane({}))).toBe(false);
    expect(focusSession(pane({ [TERM]: fake('gone', false) }))).toBe(false);
    expect(focusSession(null)).toBe(false);
    expect(active.el).toBe('tab');
  });
});

describe('refocusReview', () => {
  type Box = { id: string; isConnected: boolean; focus: (o?: unknown) => void; contains: (n: unknown) => boolean };
  const active: { el: unknown } = { el: null };
  const review = (): Box => ({ id: 'review', isConnected: true, focus() { active.el = this; }, contains(n) { return n === this; } });
  const pane = (r: Box | null) => ({ querySelector: (sel: string) => (sel === '.review' ? r : null) }) as unknown as Element;
  const target = (hits: string[]) => ({ closest: (sel: string) => (hits.some((h) => sel.includes(h)) ? {} : null) }) as unknown as Element;

  it('focuses the review when the click lands on blank space outside it, such as the pane toolbar', () => {
    const r = review();
    active.el = 'body';
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(refocusReview(pane(r), target([]))).toBe(true);
    expect(active.el).toBe(r);
  });

  it('leaves a click on a control that takes focus itself alone', () => {
    const r = review();
    active.el = 'button';
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(refocusReview(pane(r), target(['button']))).toBe(false);
    expect(refocusReview(pane(r), target(['textarea']))).toBe(false);
    expect(active.el).toBe('button');
  });

  it('does nothing when focus is already inside the review or the pane shows no review', () => {
    const r = review();
    active.el = r;
    vi.stubGlobal('document', { get activeElement() { return active.el; } });
    expect(refocusReview(pane(r), target([]))).toBe(false);
    active.el = 'body';
    expect(refocusReview(pane(null), target([]))).toBe(false);
    expect(refocusReview(null, target([]))).toBe(false);
    expect(refocusReview(pane(r), null)).toBe(false);
    expect(active.el).toBe('body');
  });
});

describe('coarsePointer', () => {
  it('is true on a touch device and false with a mouse', () => {
    vi.stubGlobal('matchMedia', (q: string) => ({ matches: q === '(pointer: coarse)' }));
    expect(coarsePointer()).toBe(true);
    vi.stubGlobal('matchMedia', () => ({ matches: false }));
    expect(coarsePointer()).toBe(false);
  });

  it('is false without matchMedia', () => {
    vi.stubGlobal('matchMedia', undefined);
    expect(coarsePointer()).toBe(false);
  });
});
