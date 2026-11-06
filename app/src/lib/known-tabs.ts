import type { SidebarItem } from './grouping';

export interface KnownTabs {
  known: string[];
  tabs?: string[];
  active?: string | null;
}

const KEY = 'overseer.knownTabs';

function loadAll(): Record<string, KnownTabs> {
  try {
    const raw = globalThis.localStorage?.getItem(KEY);
    const v = raw ? JSON.parse(raw) : null;
    return v && typeof v === 'object' ? v : {};
  } catch {
    return {};
  }
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
    globalThis.localStorage?.setItem(KEY, JSON.stringify(Object.fromEntries([...kept, [id, rec]])));
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
