import { Lru } from '../lru';
import type { ChatEntry } from '../types';
import type { ChatState } from './store';

export interface Anchor {
  id: string;
  offset: number;
}

export interface ChatSnapshot {
  state: ChatState;
  anchor: Anchor | null;
  following: boolean;
  shown: number;
}

export const CACHE_LINES = 12;
export const CACHE_ENTRIES = 240;
export const CACHE_CHARS = 2_000_000;

function chars(e: ChatEntry): number {
  const t = e.tool;
  const d = t?.diff ? ('patch' in t.diff ? t.diff.patch.length : t.diff.old.length + t.diff.new.length) : 0;
  return 64 + (e.text?.length ?? 0) + (t ? (t.output?.length ?? 0) + (t.command?.length ?? 0) + (t.detail?.length ?? 0) + d : 0);
}

export function weigh(s: ChatSnapshot): number {
  let n = 0;
  for (const e of s.state.entries) n += chars(e);
  return n;
}

export function trimmed(s: ChatSnapshot, keep = CACHE_ENTRIES): ChatSnapshot {
  const n = s.state.entries.length;
  if (n <= keep) return s;
  return { ...s, state: { ...s.state, entries: s.state.entries.slice(n - keep), more: true }, shown: Math.min(s.shown, keep), following: true, anchor: null };
}

type Box = { getBoundingClientRect(): { top: number; bottom: number } };

export function anchorOf(list: Box & { querySelectorAll(sel: string): Iterable<Box & { dataset: { id?: string } }> }): Anchor | null {
  const top = list.getBoundingClientRect().top;
  for (const el of list.querySelectorAll('[data-id]')) {
    const r = el.getBoundingClientRect();
    if (r.bottom > top && el.dataset.id) return { id: el.dataset.id, offset: r.top - top };
  }
  return null;
}

export function chatCache(lines = CACHE_LINES, budget = CACHE_CHARS) {
  const lru = new Lru<string, ChatSnapshot>(lines, budget, weigh);
  return {
    get: (key: string) => lru.get(key),
    put: (key: string, s: ChatSnapshot) => (s.state.loaded ? lru.set(key, trimmed(s)) : false),
    drop: (key: string) => lru.delete(key),
    get size() {
      return lru.size;
    },
  };
}

export const chats = chatCache();
