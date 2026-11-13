import type { SidebarItem } from '../session/grouping';
import type { Layout } from '../workspace/workspace.svelte';
import { readJson, writeJson, KEY_KNOWN_TABS } from '../storage';

export interface KnownTabs {
  known: string[];
  tabs?: string[];
  active?: string | null;
  shape?: Layout;
}

const KEY = KEY_KNOWN_TABS;

function loadAll(): Record<string, KnownTabs> {
  const v = readJson(KEY);
  return v && typeof v === 'object' ? (v as Record<string, KnownTabs>) : {};
}

export function loadKnown(host: string, layout: string): KnownTabs | null {
  return loadAll()[`${host}|${layout}`] ?? null;
}

export const MAX_KNOWN = 64;

export function saveKnown(host: string, layout: string, rec: KnownTabs) {
  try {
    const all = loadAll();
    const id = `${host}|${layout}`;
    delete all[id];
    const kept = Object.entries(all).slice(-(MAX_KNOWN - 1));
    writeJson(KEY, Object.fromEntries([...kept, [id, rec]]));
  } catch {}
}

export function isKnown(m: SidebarItem, known: Set<string>): boolean {
  return known.has(m.key) || (!!m.termId && known.has(`term:${m.termId}`));
}

export function startKnown(rec: KnownTabs | null, saved: string[], members: SidebarItem[]): Set<string> | undefined {
  if (rec) return new Set(rec.known);
  return saved.length ? new Set(members.map((m) => m.key)) : undefined;
}

export const MAX_KEYS = 200;

export function mergeKnown(known: Set<string>, members: SidebarItem[], items: SidebarItem[]): Set<string> {
  const alive = new Set(items.flatMap((i) => (i.termId ? [i.key, `term:${i.termId}`] : [i.key])));
  const kept = [...known].filter((k) => alive.has(k) || k.startsWith('line:'));
  return new Set([...new Set([...kept, ...members.map((m) => m.key)])].slice(-MAX_KEYS));
}
