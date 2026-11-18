import { allTabs, closePane, type Layout } from '../workspace/workspace.svelte';

export const layoutKeyOf = (id: string) => (id === '' ? '#other' : id);

export function tabOrder(id: string, activeId: string, live: string[], cached: ReadonlyMap<string, Layout>, saved: string[] | undefined): string[] {
  if (id === activeId) return live;
  const l = cached.get(layoutKeyOf(id));
  return l ? allTabs(l) : (saved ?? []);
}

// The array handed out last time for `id` when the order is the same, so that what is derived
// from it (the session rows of that workspace) is not computed again.
export function stableTabs(memo: Map<string, string[]>, id: string, next: string[]): string[] {
  const prev = memo.get(id);
  if (prev && prev.length === next.length && prev.every((k, i) => k === next[i])) return prev;
  memo.set(id, next);
  return next;
}

export function moveTab(l: Layout, key: string, anchor: string, pos: 'before' | 'after'): Layout {
  const to = l.panes.findIndex((p) => p.tabs.includes(anchor));
  if (key === anchor || to < 0) return l;
  const from = l.panes.findIndex((p) => p.tabs.includes(key));
  const panes = l.panes.map((p, i) => {
    const tabs = p.tabs.filter((k) => k !== key);
    if (i === to) tabs.splice(tabs.indexOf(anchor) + (pos === 'after' ? 1 : 0), 0, key);
    return { tabs, active: p.active === key && i !== to ? (tabs[0] ?? null) : p.active };
  });
  const out = { ...l, panes };
  return from >= 0 && from !== to && !panes[from].tabs.length ? closePane(out, from) : out;
}

export function moveSavedTab(tabs: string[], key: string, anchor: string, pos: 'before' | 'after'): string[] | null {
  if (key === anchor || !tabs.includes(anchor)) return null;
  const rest = tabs.filter((k) => k !== key);
  rest.splice(rest.indexOf(anchor) + (pos === 'after' ? 1 : 0), 0, key);
  return rest;
}
