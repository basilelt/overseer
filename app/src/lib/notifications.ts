import type { KV } from './notify';

export type EntryKind = 'needsInput' | 'taskTesting' | 'finished' | 'failed' | 'allIdle' | 'portForwarded' | 'portClosed' | 'update' | 'upload' | 'error';

export type EntryAction = { kind: 'line'; key: string } | { kind: 'url'; url: string } | { kind: 'update' };

export interface Entry {
  id: string;
  kind: EntryKind;
  title: string;
  body?: string;
  source?: string;
  at: number;
  read: boolean;
  ev?: string;
  action?: EntryAction;
}

export interface EntryInput {
  id?: string;
  kind: EntryKind;
  title: string;
  body?: string;
  source?: string;
  action?: EntryAction;
  coalesce?: string;
  quiet?: boolean;
}

export const MAX_ENTRIES = 100;
export const SUPERSEDE_MS = 60_000;
const PREFIX = 'overseer.notifications.';

export function storageKey(host: string): string {
  return PREFIX + host;
}

function valid(e: unknown): e is Entry {
  const x = e as Entry;
  return !!x && typeof x.id === 'string' && typeof x.kind === 'string' && typeof x.title === 'string' && typeof x.at === 'number';
}

export function loadEntries(store: KV | undefined, host: string): Entry[] {
  try {
    const raw = JSON.parse(store?.getItem(storageKey(host)) ?? '[]');
    return Array.isArray(raw) ? compact(raw.filter(valid)).slice(0, MAX_ENTRIES) : [];
  } catch {
    return [];
  }
}

export function saveEntries(store: KV | undefined, host: string, list: Entry[]): void {
  try {
    store?.setItem(storageKey(host), JSON.stringify(list));
  } catch {}
}

function subjectOf(e: Entry): string {
  return e.action?.kind === 'line' ? e.action.key : e.id.startsWith('allIdle|') ? `allIdle|${e.title}` : e.id;
}

function compact(list: Entry[]): Entry[] {
  const seen = new Set<string>();
  const out: Entry[] = [];
  for (const e of list) {
    const subject = subjectOf(e);
    if (seen.has(subject)) continue;
    seen.add(subject);
    const { count: _, ...rest } = e as Entry & { count?: number };
    out.push(e.action?.kind === 'line' && e.id !== subject ? { ...rest, id: subject, ev: e.ev ?? e.id } : rest);
  }
  return out;
}

const outranks: Partial<Record<EntryKind, EntryKind[]>> = { taskTesting: ['finished'], failed: ['finished'] };

let seq = 0;

export function addEntry(list: Entry[], input: EntryInput, now: number): Entry[] | null {
  const { coalesce, quiet, ...rest } = input;
  if (input.id && list.some((e) => e.id === input.id || e.ev === input.id)) return null;
  const id = coalesce ?? input.id ?? `${now.toString(36)}-${(seq++).toString(36)}`;
  const ev = coalesce ? input.id : undefined;
  const i = coalesce ? list.findIndex((e) => e.id === coalesce) : -1;
  if (i >= 0) {
    const prev = list[i];
    if (now - prev.at < SUPERSEDE_MS && outranks[prev.kind]?.includes(input.kind)) return null;
    const next: Entry = { ...rest, id, ev, at: now, read: quiet ? prev.read : false };
    return [next, ...list.slice(0, i), ...list.slice(i + 1)];
  }
  return [{ ...rest, id, ev, at: now, read: !!quiet }, ...list].slice(0, MAX_ENTRIES);
}

export function markSubjectRead(list: Entry[], key: string): Entry[] | null {
  if (!list.some((e) => !e.read && subjectOf(e) === key)) return null;
  return list.map((e) => (!e.read && subjectOf(e) === key ? { ...e, read: true } : e));
}

export function unreadCount(list: Entry[]): number {
  return list.reduce((n, e) => n + (e.read ? 0 : 1), 0);
}
