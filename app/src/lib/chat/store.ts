import type { ChatEntry, ChatPage } from '../types';

export const KEEP = 600;

export interface ChatState {
  entries: ChatEntry[];
  cursor: number;
  more: boolean;
  model: string | null;
  loaded: boolean;
  // The newest entries were dropped to read further back; only a fresh tail repairs the window.
  cut?: boolean;
}

export const EMPTY_CHAT: ChatState = { entries: [], cursor: 0, more: false, model: null, loaded: false, cut: false };

function merge(base: ChatEntry[], incoming: ChatEntry[]): ChatEntry[] {
  if (!incoming.length) return base;
  const byId = new Map(base.map((e) => [e.id, e]));
  for (const e of incoming) {
    const prev = byId.get(e.id);
    if (!prev || prev.rev <= e.rev) byId.set(e.id, e);
  }
  return [...byId.values()].sort((a, b) => a.seq - b.seq);
}

export function cap(entries: ChatEntry[], keep = KEEP): { entries: ChatEntry[]; dropped: boolean } {
  return entries.length > keep ? { entries: entries.slice(entries.length - keep), dropped: true } : { entries, dropped: false };
}

export function applyTail(s: ChatState, p: ChatPage, keep = KEEP): ChatState {
  const c = cap(p.entries, keep);
  return { entries: c.entries, cursor: p.cursor, more: p.more || c.dropped, model: p.model ?? s.model, loaded: true, cut: false };
}

// A followed window drops its oldest entries; a scrolled-up one keeps what the user reads and drops the newest instead.
export function applyUpdate(s: ChatState, p: ChatPage, keep = KEEP, following = true): ChatState {
  if (p.reset) return applyTail(s, p, keep);
  const last = s.entries.at(-1)?.seq ?? Infinity;
  const incoming = s.cut ? p.entries.filter((e) => e.seq <= last) : p.entries;
  const merged = merge(s.entries, incoming);
  const base = { cursor: Math.max(s.cursor, p.cursor), model: p.model ?? s.model, loaded: true };
  if (following && !s.cut) {
    const c = cap(merged, keep);
    return { ...base, entries: c.entries, more: s.more || c.dropped, cut: false };
  }
  const max = keep * 2;
  return merged.length > max
    ? { ...base, entries: merged.slice(0, max), more: s.more, cut: true }
    : { ...base, entries: merged, more: s.more, cut: !!s.cut };
}

export function applyEarlier(s: ChatState, p: ChatPage, keep = KEEP * 2): ChatState {
  const first = s.entries[0]?.seq ?? Infinity;
  const older = p.entries.filter((e) => e.seq < first && !s.entries.some((x) => x.id === e.id));
  const all = [...older, ...s.entries];
  const cut = all.length > keep;
  return { ...s, entries: cut ? all.slice(0, keep) : all, more: p.more, model: s.model ?? p.model, cut: cut || !!s.cut };
}

export function needsFetch(s: ChatState, cursor: number): boolean {
  return !s.loaded || cursor === 0 || cursor > s.cursor;
}
