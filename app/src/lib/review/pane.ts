import { allTabs, dropKey, split, type Layout, type Pane } from '../workspace/workspace.svelte';

export type ReviewMode = 'side' | 'tab' | 'off';

const setPane = (l: Layout, i: number, p: Pane): Layout => ({ ...l, panes: l.panes.map((x, j) => (j === i ? p : x)) });

export function sideIndex(l: Layout, from: number): number {
  return from === 0 ? 1 : 0;
}

export function openBeside(l: Layout, from: number, key: string, side: string | null = null): Layout {
  const at = Math.min(Math.max(from, 0), l.panes.length - 1);
  if (l.panes.length < 2) {
    const s = split(l, 'horizontal');
    return { ...setPane(s, 1, { tabs: [key], active: key }), focused: at };
  }
  const has = l.panes.findIndex((p) => p.tabs.includes(key));
  if (has >= 0) return { ...setPane(l, has, { ...l.panes[has], active: key }), focused: at };
  const i = sideIndex(l, at);
  const p = l.panes[i];
  const tabs = side && p.tabs.includes(side) ? p.tabs.map((k) => (k === side ? key : k)) : [...p.tabs, key];
  return { ...setPane(l, i, { tabs, active: key }), focused: at };
}

export function follow(l: Layout, side: string | null, pinned: boolean, want: string | null): Layout {
  if (!side || pinned || !want || side === want) return l;
  const i = l.panes.findIndex((p) => p.tabs.includes(side));
  if (i < 0) return l;
  const p = l.panes[i];
  if (p.tabs.includes(want)) return setPane(l, i, { tabs: p.tabs.filter((k) => k !== side), active: want });
  if (allTabs(l).includes(want)) return l;
  return setPane(l, i, { tabs: p.tabs.map((k) => (k === side ? want : k)), active: p.active === side ? want : p.active });
}

export function reviewMode(l: Layout, side: string | null, key: string | null): ReviewMode {
  const tabs = allTabs(l);
  if (side && tabs.includes(side)) return 'side';
  return key && tabs.includes(key) ? 'tab' : 'off';
}

export function closeReviews(l: Layout, keys: (string | null)[]): Layout {
  return keys.reduce((out, k) => (k ? dropKey(out, k) : out), l);
}
