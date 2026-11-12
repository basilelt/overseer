import type { SidebarItem } from '../session/grouping';
import { followTerm, tabInfo } from './tabs';
import type { KnownTabs } from './known-tabs';
import { allTabs, dropKey, layoutFromTabs, renameTab, setRatio, type Layout } from '../workspace/workspace.svelte';
import { resolveTab } from '../workspace/workspaces';

export function layoutShape(l: Layout): Layout | undefined {
  return l.split === 'none' || l.panes.length < 2 ? undefined : l;
}

export function arrangePanes(base: Layout, rec: KnownTabs | null, items: SidebarItem[]): Layout {
  const shape = rec?.shape;
  if (!shape || shape.split === 'none' || !Array.isArray(shape.panes) || shape.panes.length < 2) return base;
  const tabs = allTabs(base);
  const left = new Set(tabs);
  const panes = shape.panes.map((p) => {
    const own = [...new Set((p.tabs ?? []).map((k) => resolveTab(k, items)))].filter((k): k is string => !!k && left.delete(k));
    const active = p.active ? resolveTab(p.active, items) : null;
    return { tabs: own, active: active && own.includes(active) ? active : (own[0] ?? null) };
  });
  const focused = Number.isInteger(shape.focused) && shape.focused >= 0 && shape.focused < panes.length ? shape.focused : 0;
  const rest = tabs.filter((k) => left.has(k));
  if (rest.length) {
    const p = panes[focused];
    panes[focused] = { tabs: [...p.tabs, ...rest], active: p.active ?? rest[0] };
  }
  return { split: shape.split, panes, focused, ratio: setRatio(base, typeof shape.ratio === 'number' ? shape.ratio : 0.5).ratio };
}

export function restoreLayout(rec: KnownTabs | null, items: SidebarItem[], fallback: string | null = null): Layout {
  const tabs = [...new Set((rec?.tabs ?? []).map((k) => resolveTab(k, items)).filter((k): k is string => !!k && !!tabInfo(k, items)))];
  const active = rec?.active ? resolveTab(rec.active, items) : null;
  return arrangePanes(layoutFromTabs(tabs, active && tabs.includes(active) ? active : fallback), rec, items);
}

export function staleAfterReset(stale: Set<string>, seen: number, epoch: number, current: string): number {
  if (epoch !== seen) stale.add(current);
  return epoch;
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

export function reconcileLayout(layout: Layout, items: SidebarItem[], termOf: Map<string, string>, selected: string | null, pending: ReadonlySet<string> = new Set()): Reconciled {
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
    } else if (!r && !tabInfo(k, items) && !(k.startsWith('term:') && pending.has(k.slice(5)))) {
      l = dropKey(l, k);
      forget.push(k);
    }
  }
  return { layout: l, selected: sel, forget };
}
