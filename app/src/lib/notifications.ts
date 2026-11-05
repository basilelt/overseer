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
  count?: number;
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

export const MAX_ENTRIES = 200;
export const COALESCE_MS = 10 * 60_000;
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
    return Array.isArray(raw) ? raw.filter(valid).slice(0, MAX_ENTRIES) : [];
  } catch {
    return [];
  }
}

export function saveEntries(store: KV | undefined, host: string, list: Entry[]): void {
  try {
    store?.setItem(storageKey(host), JSON.stringify(list));
  } catch {}
}

let seq = 0;

export function addEntry(list: Entry[], input: EntryInput, now: number): Entry[] | null {
  const { coalesce, quiet, ...rest } = input;
  const id = input.id ?? coalesce ?? `${now.toString(36)}-${(seq++).toString(36)}`;
  if (input.id && list.some((e) => e.id === input.id)) return null;
  const i = coalesce ? list.findIndex((e) => e.id === coalesce && now - e.at < COALESCE_MS) : -1;
  if (i >= 0) {
    const prev = list[i];
    const changed = prev.kind !== input.kind;
    const next: Entry = { ...prev, ...rest, id, at: now, read: !!quiet || (prev.read && !changed), count: (prev.count ?? 1) + 1 };
    return [next, ...list.slice(0, i), ...list.slice(i + 1)];
  }
  const fresh = list.filter((e) => e.id !== id);
  return [{ ...rest, id, at: now, read: !!quiet }, ...fresh].slice(0, MAX_ENTRIES);
}

export function unreadCount(list: Entry[]): number {
  return list.reduce((n, e) => n + (e.read ? 0 : 1), 0);
}
