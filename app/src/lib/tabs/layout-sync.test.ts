import { describe, expect, it, vi } from 'vitest';
import type { SidebarItem } from '../session/grouping';
import { loadKnown, saveKnown } from './known-tabs';
import { arrangePanes, layoutShape, reconcileLayout, restoreLayout, savedActive, staleAfterReset } from './layout-sync';
import { rememberTerms } from './tabs';
import type { Term } from '../types';
import { layoutFromTabs } from '../workspace/workspace.svelte';

const item = (key: string, over: Partial<SidebarItem> = {}): SidebarItem => ({ key, kind: 'claude', group: 'running', state: 'running', title: key, at: '2026-10-29T10:00:00Z', ...over });

function memoryStorage() {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
}

describe('closing a session opened from the status list', () => {
  const line = item('line:a', { lineId: 'a', termId: 't1' });
  const shell = item('term:t1', { kind: 'shell', termId: 't1' });
  const bound = [{ id: 't1', lineId: 'a' } as Term];

  it('keeps the tab and the selection on the shell left in its term', () => {
    const termOf = new Map<string, string>();
    rememberTerms(termOf, [line], bound);
    const out = reconcileLayout(layoutFromTabs(['line:a'], 'line:a'), [shell], termOf, 'line:a');
    expect(out.layout.panes[0]).toEqual({ tabs: ['term:t1'], active: 'term:t1' });
    expect(out.selected).toBe('term:t1');
    expect(out.forget).toEqual(['line:a']);
  });

  it('closes the tab of a session whose term is gone', () => {
    const out = reconcileLayout(layoutFromTabs(['line:a', 'term:z'], 'line:a'), [], new Map(), 'line:a');
    expect(out.layout.panes[0].tabs).toEqual([]);
  });

  it('leaves live tabs and other tab kinds untouched', () => {
    const l = layoutFromTabs(['line:a', 'browser:x'], 'browser:x');
    const out = reconcileLayout(l, [line], new Map(), 'browser:x');
    expect(out.layout).toBe(l);
    expect(out.selected).toBe('browser:x');
    expect(out.forget).toEqual([]);
  });
});

describe('a term linked to a line the app has not received yet', () => {
  const older = item('line:old', { lineId: 'old', termId: 't0' });

  it('keeps its tab and its selection until the line arrives', () => {
    const l = layoutFromTabs(['line:old', 'term:t1'], 'term:t1');
    const out = reconcileLayout(l, [older], new Map(), 'term:t1', new Set(['t1']));
    expect(out.layout).toBe(l);
    expect(out.selected).toBe('term:t1');
    expect(out.forget).toEqual([]);
  });

  it('moves the tab to the line once it arrives', () => {
    const fresh = item('line:new', { lineId: 'new', termId: 't1' });
    const out = reconcileLayout(layoutFromTabs(['line:old', 'term:t1'], 'term:t1'), [older, fresh], new Map(), 'term:t1', new Set(['t1']));
    expect(out.layout.panes[0]).toEqual({ tabs: ['line:old', 'line:new'], active: 'line:new' });
    expect(out.selected).toBe('line:new');
  });
});

describe('restoring tabs after a reload', () => {
  const items = [item('line:a'), item('line:b', { termId: 't2' }), item('term:t3', { kind: 'shell', termId: 't3' })];

  it('brings back the status tabs and the selected one', () => {
    vi.stubGlobal('localStorage', memoryStorage());
    saveKnown('local', '#status', { known: [], tabs: ['line:b', 'term:t3', 'browser:x'], active: 'term:t3' });
    const l = restoreLayout(loadKnown('local', '#status'), items);
    expect(l.panes[0]).toEqual({ tabs: ['line:b', 'term:t3', 'browser:x'], active: 'term:t3' });
  });

  it('drops tabs that no longer exist and falls back when the selection is gone', () => {
    const l = restoreLayout({ known: [], tabs: ['line:gone', 'line:a'], active: 'line:gone' }, items);
    expect(l.panes[0]).toEqual({ tabs: ['line:a'], active: 'line:a' });
  });

  it('opens nothing without a saved record', () => {
    expect(restoreLayout(null, items).panes[0]).toEqual({ tabs: [], active: null });
    expect(restoreLayout(null, items, 'line:a').panes[0].active).toBeNull();
  });

  it('follows a saved term tab that became a line', () => {
    const l = restoreLayout({ known: [], tabs: ['term:t2'], active: 'term:t2' }, items);
    expect(l.panes[0]).toEqual({ tabs: ['line:b'], active: 'line:b' });
  });

  it('only returns the saved selection when it is an open tab', () => {
    const layout = layoutFromTabs(['line:a', 'line:b']);
    expect(savedActive({ known: [], active: 'line:b' }, layout, items)).toBe('line:b');
    expect(savedActive({ known: [], active: 'line:zz' }, layout, items)).toBeNull();
    expect(savedActive({ known: [], active: 'term:t3' }, layout, items)).toBeNull();
    expect(savedActive(null, layout, items)).toBeNull();
  });

  it('keeps the selection in the record next to the known tabs', () => {
    vi.stubGlobal('localStorage', memoryStorage());
    saveKnown('local', 'ws1', { known: ['line:a'], active: 'line:a' });
    expect(loadKnown('local', 'ws1')).toEqual({ known: ['line:a'], active: 'line:a' });
  });
});

describe('restoring a split after a reload', () => {
  const items = [item('line:a'), item('line:b'), item('line:c'), item('line:d')];
  const split = { split: 'vertical' as const, panes: [{ tabs: ['line:a', 'line:c'], active: 'line:c' }, { tabs: ['line:b'], active: 'line:b' }], focused: 1, ratio: 0.3 };

  it('saves nothing for a single pane', () => {
    expect(layoutShape(layoutFromTabs(['line:a']))).toBeUndefined();
  });

  it('round-trips panes, sizes, focus and the active tab of each pane', () => {
    const shape = layoutShape({ ...split, panes: split.panes });
    const l = restoreLayout({ known: [], tabs: ['line:a', 'line:b', 'line:c'], active: 'line:b', shape }, items);
    expect(l).toEqual(split);
  });

  it('puts each tab back in its pane for a workspace and appends unplaced tabs to the focused pane', () => {
    const l = arrangePanes(layoutFromTabs(['line:a', 'line:b', 'line:c', 'line:d'], 'line:b'), { known: [], shape: split }, items);
    expect(l.panes.map((p) => p.tabs)).toEqual([['line:a', 'line:c'], ['line:b', 'line:d']]);
    expect(l.split).toBe('vertical');
    expect(l.ratio).toBe(0.3);
  });

  it('drops tabs that are gone and keeps the split even when a pane empties', () => {
    const l = arrangePanes(layoutFromTabs(['line:a', 'line:c'], 'line:a'), { known: [], shape: split }, items);
    expect(l.panes.map((p) => p.tabs)).toEqual([['line:a', 'line:c'], []]);
    expect(l.panes[1].active).toBeNull();
  });

  it('ignores a malformed shape', () => {
    const flat = layoutFromTabs(['line:a'], 'line:a');
    expect(arrangePanes(flat, { known: [], shape: { split: 'vertical', panes: [] } as never }, items)).toBe(flat);
    expect(arrangePanes(flat, { known: [] }, items)).toBe(flat);
  });

  it('survives the localStorage round trip', () => {
    vi.stubGlobal('localStorage', memoryStorage());
    saveKnown('local', '#status', { known: [], tabs: ['line:a', 'line:b'], active: 'line:a', shape: split });
    expect(loadKnown('local', '#status')?.shape).toEqual(split);
  });
});

describe('staleAfterReset', () => {
  it('marks the shown layout stale once when the layouts were forgotten', () => {
    const stale = new Set<string>();
    const seen = staleAfterReset(stale, 0, 1, 'ws1');
    expect([...stale]).toEqual(['ws1']);
    expect(seen).toBe(1);
    stale.delete('ws1');
    expect(staleAfterReset(stale, seen, 1, 'ws1')).toBe(1);
    expect(stale.size).toBe(0);
  });
});
