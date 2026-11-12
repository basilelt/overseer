import { allTabs, closePane, type Layout } from '../workspace/workspace.svelte';

export const layoutKeyOf = (id: string) => (id === '' ? '#other' : id);

/** Tab order of a sidebar workspace: its live tabs when on screen, else its cached layout, else its saved tabs. */
export function tabOrder(id: string, activeId: string, live: string[], cached: ReadonlyMap<string, Layout>, saved: string[] | undefined): string[] {
  if (id === activeId) return live;
  const l = cached.get(layoutKeyOf(id));
  return l ? allTabs(l) : (saved ?? []);
}

/** Moves `key` next to `anchor`, into the anchor's pane; opens it there when it had no tab. */
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

/** The saved tab list with `key` next to `anchor`, or null when the anchor has no tab. */
export function moveSavedTab(tabs: string[], key: string, anchor: string, pos: 'before' | 'after'): string[] | null {
  if (key === anchor || !tabs.includes(anchor)) return null;
  const rest = tabs.filter((k) => k !== key);
  rest.splice(rest.indexOf(anchor) + (pos === 'after' ? 1 : 0), 0, key);
  return rest;
}
