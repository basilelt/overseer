import { closedShells, popReopenable, pushClosed } from '../tabs/closed-tabs';

export type SplitDir = 'none' | 'horizontal' | 'vertical';

export interface Pane {
  tabs: string[];
  active: string | null;
}

export interface Layout {
  split: SplitDir;
  panes: Pane[];
  focused: number;
  ratio: number;
}

export const EMPTY_LAYOUT: Layout = { split: 'none', panes: [{ tabs: [], active: null }], focused: 0, ratio: 0.5 };

const withPane = (l: Layout, i: number, p: Pane): Layout => ({ ...l, panes: l.panes.map((x, j) => (j === i ? p : x)) });

export function activeKey(l: Layout): string | null {
  return l.panes[l.focused]?.active ?? null;
}

export function openInFocused(l: Layout, key: string): Layout {
  const at = l.panes.findIndex((p) => p.tabs.includes(key));
  if (at >= 0) return withPane({ ...l, focused: at }, at, { ...l.panes[at], active: key });
  const p = l.panes[l.focused];
  return withPane(l, l.focused, { tabs: [...p.tabs, key], active: key });
}

export function focusPane(l: Layout, i: number): Layout {
  return i >= 0 && i < l.panes.length ? { ...l, focused: i } : l;
}

export function activateTab(l: Layout, i: number, key: string): Layout {
  const p = l.panes[i];
  if (!p?.tabs.includes(key)) return l;
  return withPane({ ...l, focused: i }, i, { ...p, active: key });
}

export function closeTab(l: Layout, i: number, key: string): Layout {
  const p = l.panes[i];
  const idx = p?.tabs.indexOf(key) ?? -1;
  if (idx < 0) return l;
  const tabs = p.tabs.filter((k) => k !== key);
  if (!tabs.length && l.panes.length > 1) return closePane(l, i);
  const active = p.active === key ? (tabs[Math.min(idx, tabs.length - 1)] ?? null) : p.active;
  return withPane(l, i, { tabs, active });
}

export function split(l: Layout, dir: Exclude<SplitDir, 'none'>): Layout {
  if (l.panes.length > 1) return { ...l, split: dir };
  return { ...l, split: dir, panes: [...l.panes, { tabs: [], active: null }], focused: 1 };
}

export function closePane(l: Layout, i: number): Layout {
  if (l.panes.length < 2) return l;
  return { ...l, split: 'none', panes: l.panes.filter((_, j) => j !== i), focused: 0 };
}

export function toggleSplit(l: Layout): Layout {
  return l.panes.length > 1 ? closePane(l, 1) : split(l, 'horizontal');
}

export type Direction = 'left' | 'right' | 'up' | 'down';

export function paneInDirection(l: Layout, dir: Direction): number | null {
  if (l.panes.length < 2) return null;
  const row = l.split !== 'vertical';
  const back = row ? dir === 'left' : dir === 'up';
  const forward = row ? dir === 'right' : dir === 'down';
  const to = back ? l.focused - 1 : forward ? l.focused + 1 : -1;
  return to >= 0 && to < l.panes.length ? to : null;
}

export function setRatio(l: Layout, r: number): Layout {
  return { ...l, ratio: Math.min(0.85, Math.max(0.15, r)) };
}

export function dropKey(l: Layout, key: string): Layout {
  let out = l;
  for (let i = out.panes.length - 1; i >= 0; i--) if (out.panes[i].tabs.includes(key)) out = closeTab(out, i, key);
  return out;
}

export const workspace = $state<{ layout: Layout }>({ layout: EMPTY_LAYOUT });

export function update(fn: (l: Layout) => Layout) {
  workspace.layout = fn(workspace.layout);
}

export function allTabs(l: Layout): string[] {
  return l.panes.flatMap((p) => p.tabs);
}

export function layoutFromTabs(tabs: string[], active: string | null = null): Layout {
  return { ...EMPTY_LAYOUT, panes: [{ tabs, active: active && tabs.includes(active) ? active : (tabs[0] ?? null) }] };
}

export function addTabs(l: Layout, keys: string[]): Layout {
  const have = new Set(allTabs(l));
  const fresh = keys.filter((k) => !have.has(k));
  if (!fresh.length) return l;
  const p = l.panes[l.focused];
  return withPane(l, l.focused, { tabs: [...p.tabs, ...fresh], active: p.active ?? fresh[0] });
}

export function renameTab(l: Layout, from: string, to: string): Layout {
  if (from === to || !allTabs(l).includes(from)) return l;
  const has = allTabs(l).includes(to);
  return {
    ...l,
    panes: l.panes.map((p) => {
      const tabs = has ? p.tabs.filter((k) => k !== from) : p.tabs.map((k) => (k === from ? to : k));
      return { tabs, active: p.active === from ? to : p.active };
    }),
  };
}

export function reorderTab(l: Layout, i: number, key: string, to: number): Layout {
  const p = l.panes[i];
  if (!p?.tabs.includes(key)) return l;
  const from = p.tabs.indexOf(key);
  const tabs = p.tabs.filter((k) => k !== key);
  tabs.splice(Math.max(0, Math.min(to > from ? to - 1 : to, tabs.length)), 0, key);
  return withPane(l, i, { ...p, tabs });
}

export function activeTabs(l: Layout): string[] {
  return l.panes[l.focused]?.tabs ?? [];
}

const saved = new Map<string, Layout>();
const closed = new Map<string, string[]>();
let layoutId = '#status';

export function switchLayout(from: string, to: string, init: () => Layout) {
  saved.set(from, workspace.layout);
  workspace.layout = saved.get(to) ?? init();
  layoutId = to;
}

/** Layouts of the workspaces not on screen, by layout id. */
export const savedLayouts: Map<string, Layout> = saved;

export function forgetLayouts() {
  saved.clear();
  closed.clear();
  closedShells.clear();
}

let tabClosed: ((key: string, end: boolean) => void) | null = null;

export function onTabClosed(fn: (key: string, end: boolean) => void) {
  tabClosed = fn;
}

export function closeTabAt(i: number, key: string, end = true) {
  if (!workspace.layout.panes[i]?.tabs.includes(key)) return;
  update((l) => closeTab(l, i, key));
  closed.set(layoutId, pushClosed(closed.get(layoutId) ?? [], key));
  tabClosed?.(key, end);
}

export function closePaneAt(i: number) {
  const tabs = workspace.layout.panes[i]?.tabs;
  if (!tabs || workspace.layout.panes.length < 2) return;
  let stack = closed.get(layoutId) ?? [];
  for (const key of tabs) stack = pushClosed(stack, key);
  closed.set(layoutId, stack);
  update((l) => closePane(l, i));
}

export function takeClosed(reopen: (key: string) => string | null): string | null {
  const r = popReopenable(closed.get(layoutId) ?? [], reopen);
  closed.set(layoutId, r.rest);
  return r.key;
}
