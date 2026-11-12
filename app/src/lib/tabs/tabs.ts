import type { Component } from 'svelte';
import type { SidebarItem } from '../session/grouping';
import type { Term } from '../types';
import { REVIEW_PREFIX } from '../review/keys';
import { isShellItem } from '../agents';

export type TabKind = 'agent' | 'chat' | 'shell' | 'browser' | 'review' | 'graph' | 'worktrees' | 'system' | 'usage' | 'prs';

export interface TabInfo {
  key: string;
  kind: TabKind;
  arg: string;
  item?: SidebarItem;
}

export type TabComponent = Component<{ tabKey: string; tab: TabInfo; focused: boolean }>;

export interface TabKindDef {
  prefix: string;
  load?: () => Promise<TabComponent | null>;
}

export const TAB_KINDS: Partial<Record<TabKind, TabKindDef>> = {
  browser: { prefix: 'browser:' },
};

export function registerTabKind(kind: TabKind, def: TabKindDef) {
  TAB_KINDS[kind] = def;
}

registerTabKind('review', {
  prefix: REVIEW_PREFIX,
  load: () => import('../components/review/ReviewTab.svelte').then((m) => m.default as TabComponent),
});

registerTabKind('graph', {
  prefix: 'graph:',
  load: () => import('../git/GraphTabKind.svelte').then((m) => m.default as TabComponent),
});

registerTabKind('worktrees', {
  prefix: 'worktrees:',
  load: () => import('../git/WorktreesTabKind.svelte').then((m) => m.default as TabComponent),
});

registerTabKind('system', {
  prefix: 'system:',
  load: () => import('../components/system/SystemTab.svelte').then((m) => m.default as TabComponent),
});

registerTabKind('usage', {
  prefix: 'usage:',
  load: () => import('../components/usage/UsageTab.svelte').then((m) => m.default as TabComponent),
});

registerTabKind('prs', {
  prefix: 'prs:',
  load: () => import('../components/github/PrReviewsTab.svelte').then((m) => m.default as TabComponent),
});

export function tabInfo(key: string, items: SidebarItem[], mode?: (item: SidebarItem) => 'chat' | 'terminal'): TabInfo | null {
  for (const [kind, def] of Object.entries(TAB_KINDS) as [TabKind, TabKindDef][]) {
    if (key.startsWith(def.prefix)) return { key, kind, arg: key.slice(def.prefix.length) };
  }
  const item = items.find((i) => i.key === key);
  if (!item) return null;
  const kind: TabKind = isShellItem(item) ? 'shell' : mode?.(item) === 'chat' ? 'chat' : 'agent';
  return { key, kind, arg: item.lineId ?? item.termId ?? '', item };
}

export function rememberTerms(termOf: Map<string, string>, items: SidebarItem[], terms: Term[]) {
  const bound = new Set(terms.filter((t) => t.lineId).map((t) => `${t.lineId}|${t.id}`));
  for (const i of items) if (i.lineId && i.termId && bound.has(`${i.lineId}|${i.termId}`)) termOf.set(i.key, i.termId);
}

export function followTerm(key: string, items: SidebarItem[], termOf: Map<string, string>): string | null {
  const id = key.startsWith('line:') ? termOf.get(key) : undefined;
  return (id && items.find((i) => i.termId === id && !i.lineId)?.key) || null;
}

export function newTabKey(kind: 'browser' | 'review' | 'graph' | 'worktrees' | 'system' | 'usage' | 'prs', arg: string): string {
  return `${TAB_KINDS[kind]!.prefix}${arg}`;
}

export function tabTitle(t: TabInfo, label: (kind: TabKind, arg: string) => string): string {
  return t.item?.title ?? label(t.kind, t.arg);
}

export type TabShortcut = { type: 'index'; index: number } | { type: 'prev' } | { type: 'next' };

export function applyShortcut(tabs: string[], active: string | null, s: TabShortcut): string | null {
  if (!tabs.length) return null;
  if (s.type === 'index') return s.index === 8 ? tabs[tabs.length - 1] : (tabs[s.index] ?? null);
  const i = active ? tabs.indexOf(active) : -1;
  const step = s.type === 'next' ? 1 : -1;
  return tabs[(i + step + tabs.length) % tabs.length];
}

export function stripScroll(view: { left: number; width: number; scroll: number }, tab: { left: number; width: number }): number | null {
  const start = tab.left - view.left + view.scroll;
  const end = start + tab.width;
  if (start < view.scroll) return Math.max(0, start);
  if (end > view.scroll + view.width) return end - view.width;
  return null;
}

export function exitedTerm(key: string, terms: { id: string; exited: boolean }[]): string | null {
  if (!key.startsWith('term:')) return null;
  const id = key.slice(5);
  return terms.some((t) => t.id === id && t.exited) ? id : null;
}

export function endTarget(key: string, items: SidebarItem[]): { line: string } | { term: string } | null {
  const it = items.find((i) => i.key === key);
  if (it?.lineId) return { line: it.lineId };
  if (it?.termId && key.startsWith('term:')) return { term: it.termId };
  return null;
}

export function focusSession(pane: Element | null | undefined): boolean {
  const target = pane?.querySelector<HTMLElement>('.chat .composer textarea') ?? pane?.querySelector<HTMLElement>('.keep:not(.hidden) [data-terminal]') ?? pane?.querySelector<HTMLElement>('.review');
  if (!target || !target.isConnected) return false;
  target.focus({ preventScroll: true });
  return document.activeElement === target;
}

export const coarsePointer = () => typeof matchMedia === 'function' && matchMedia('(pointer: coarse)').matches;
