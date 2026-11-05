import { describe, expect, it } from 'vitest';
import type { SidebarItem } from './grouping';
import { reconcileLayout } from './layout-sync';
import { rememberTerms } from './tabs';
import type { Term } from './types';
import { layoutFromTabs } from './workspace.svelte';

const item = (key: string, over: Partial<SidebarItem> = {}): SidebarItem => ({ key, kind: 'claude', group: 'working', title: key, at: '2026-10-29T10:00:00Z', ...over });

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
