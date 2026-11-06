import { afterEach, describe, expect, it, vi } from 'vitest';
import type { SidebarItem } from './grouping';
import { loadKnown, saveKnown } from './known-tabs';
import { reconcileLayout, restoreLayout, savedActive } from './layout-sync';
import { rememberTerms } from './tabs';
import type { Term } from './types';
import { layoutFromTabs } from './workspace.svelte';

const item = (key: string, over: Partial<SidebarItem> = {}): SidebarItem => ({ key, kind: 'claude', group: 'running', state: 'running', title: key, at: '2026-10-29T10:00:00Z', ...over });

function memoryStorage() {
  const data = new Map<string, string>();
  return { getItem: (k: string) => data.get(k) ?? null, setItem: (k: string, v: string) => void data.set(k, v) };
}

afterEach(() => vi.unstubAllGlobals());

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
