import type { SidebarItem } from './grouping';
import { followTerm, tabInfo } from './tabs';
import type { KnownTabs } from './known-tabs';
import { allTabs, dropKey, layoutFromTabs, renameTab, type Layout } from './workspace.svelte';
import { resolveTab } from './workspaces';

export function restoreLayout(rec: KnownTabs | null, items: SidebarItem[], fallback: string | null = null): Layout {
  const tabs = [...new Set((rec?.tabs ?? []).map((k) => resolveTab(k, items)).filter((k): k is string => !!k && !!tabInfo(k, items)))];
  const active = rec?.active ? resolveTab(rec.active, items) : null;
  return layoutFromTabs(tabs, active && tabs.includes(active) ? active : fallback);
}

export function savedActive(rec: KnownTabs | null, layout: Layout, items: SidebarItem[]): string | null {
  const active = rec?.active ? resolveTab(rec.active, items) : null;
  return active && allTabs(layout).includes(active) ? active : null;
}

export interface Reconciled {
  layout: Layout;
  selected: string | null;
  forget: string[];
}

export function reconcileLayout(layout: Layout, items: SidebarItem[], termOf: Map<string, string>, selected: string | null): Reconciled {
  let l = layout;
  let sel = selected;
  const forget: string[] = [];
  for (const k of allTabs(layout)) {
    if (!k.startsWith('term:') && !k.startsWith('line:')) continue;
    const r = followTerm(k, items, termOf) ?? resolveTab(k, items);
    if (r && r !== k) {
      l = renameTab(l, k, r);
      forget.push(k);
      if (sel === k) sel = r;
    } else if (!r && !tabInfo(k, items)) {
      l = dropKey(l, k);
      forget.push(k);
    }
  }
  return { layout: l, selected: sel, forget };
}
