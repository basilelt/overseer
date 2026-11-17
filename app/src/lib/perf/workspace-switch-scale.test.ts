import { describe, expect, it } from 'vitest';
import type { SidebarItem } from '../session/grouping';
import { reconcileLayout, restoreLayout } from '../tabs/layout-sync';
import { followTerm, tabInfo } from '../tabs/tabs';
import { itemWorktree, worktreeOf, type Known } from '../review/keys';
import { layoutFromTabs } from '../workspace/workspace.svelte';
import { resolveTab, workspaceTabs } from '../workspace/workspaces';
import { growth } from './work';

// A switch touches the tabs of the target workspace; the sessions of every other workspace
// are already indexed (the index is built once per items list), so they must not add work.
const TABS = 30;

function sessions(n: number): SidebarItem[] {
  return Array.from({ length: n }, (_, i) =>
    i % 4 === 0
      ? ({ key: `term:t${i}`, termId: `t${i}`, kind: 'shell', group: 'ready', title: `sh ${i}` } as SidebarItem)
      : ({ key: `line:l${i}`, lineId: `l${i}`, termId: `t${i}`, kind: 'claude', group: 'ready', title: `s ${i}` } as SidebarItem),
  );
}

function known(n: number): Known {
  return {
    lines: Array.from({ length: n }, (_, i) => ({ line: { id: `l${i}`, worktreePath: `/w/${i}` } })),
    tasks: Array.from({ length: n }, (_, i) => ({ id: `k${i}`, number: i, title: `t${i}`, lineId: `l${i}` })),
  };
}

// The target workspace's tabs sit at the end of the list: a scan visits every other session first.
const tabsOf = (items: SidebarItem[]) => items.slice(-TABS).map((i) => i.key);

function perSwitch(n: number, run: (items: SidebarItem[], tabs: string[]) => void): () => () => void {
  return () => {
    const items = sessions(n);
    const tabs = tabsOf(items);
    run(items, tabs);
    return () => run(items, tabs);
  };
}

describe('workspace switch work does not grow with sessions outside the target workspace', () => {
  it('reconcileLayout over the target tabs', () => {
    const g = growth(
      perSwitch(1_000, (items, tabs) => reconcileLayout(layoutFromTabs(tabs), items, new Map(), tabs[0]!, new Set())),
      perSwitch(10_000, (items, tabs) => reconcileLayout(layoutFromTabs(tabs), items, new Map(), tabs[0]!, new Set())),
    );
    expect(g.ratio, `work ${g.small} -> ${g.big}`).toBeLessThan(2);
  });

  it('restoreLayout and workspaceTabs over the saved tabs', () => {
    const run = (items: SidebarItem[], tabs: string[]) => {
      restoreLayout({ known: tabs, tabs, active: tabs[1]! }, items);
      workspaceTabs(tabs, [], items);
    };
    const g = growth(perSwitch(1_000, run), perSwitch(10_000, run));
    expect(g.ratio, `work ${g.small} -> ${g.big}`).toBeLessThan(2);
  });

  it('resolveTab, tabInfo and followTerm per tab', () => {
    const run = (items: SidebarItem[], tabs: string[]) => {
      const termOf = new Map(tabs.map((k) => [k, k.replace('line:l', 't')]));
      for (const k of tabs) {
        resolveTab(k.startsWith('line:') ? `term:${k.replace('line:l', 't')}` : k, items);
        tabInfo(k, items);
        followTerm(k, items, termOf);
      }
    };
    const g = growth(perSwitch(1_000, run), perSwitch(10_000, run));
    expect(g.ratio, `work ${g.small} -> ${g.big}`).toBeLessThan(2);
  });

  it('worktreeOf for each tab of the target workspace', () => {
    const at = (n: number) => () => {
      const k = known(n);
      const run = () => {
        for (let i = n - TABS; i < n; i++) {
          worktreeOf({ lineId: `l${i}` }, k);
          worktreeOf({ taskId: `k${i}` }, k);
          itemWorktree(`line:l${i}`, k);
        }
      };
      run();
      return run;
    };
    const g = growth(at(1_000), at(10_000));
    expect(g.ratio, `work ${g.small} -> ${g.big}`).toBeLessThan(2);
  });
});
