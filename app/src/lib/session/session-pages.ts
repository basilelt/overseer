import { readJson, writeJson, KEY_SESSION_PAGES } from '../storage';

export interface Page {
  id: string;
  url: string;
  title?: string;
}

export interface SessionWeb {
  pages: Page[];
  active: string | null;
  open: boolean;
  ratio: number;
}

export type PagesByKey = Record<string, SessionWeb>;

export const MAX_PAGES = 8;
export const MAX_SESSIONS = 32;
export const MAX_HOSTS = 8;
export const MAX_LIVE = 4;
export const DEFAULT_RATIO = 0.5;

const KEY = KEY_SESSION_PAGES;
const ID = /^[A-Za-z0-9_-]{1,32}$/;

const empty = (): SessionWeb => ({ pages: [], active: null, open: true, ratio: DEFAULT_RATIO });

function put(s: PagesByKey, key: string, web: SessionWeb): PagesByKey {
  const { [key]: _, ...rest } = s;
  if (!web.pages.length) return rest;
  const all = Object.entries({ ...rest, [key]: web });
  return Object.fromEntries(all.slice(-MAX_SESSIONS));
}

export function attach(s: PagesByKey, key: string, url: string, newId: () => string): { state: PagesByKey; page: Page } {
  const cur = s[key] ?? empty();
  const hit = cur.pages.find((p) => p.url === url);
  const page = hit ?? { id: newId(), url };
  const pages = hit ? cur.pages : [...cur.pages, page].slice(-MAX_PAGES);
  return { state: put(s, key, { ...cur, pages, active: page.id, open: true }), page };
}

export function closePage(s: PagesByKey, key: string, id: string): PagesByKey {
  const cur = s[key];
  if (!cur) return s;
  const at = cur.pages.findIndex((p) => p.id === id);
  if (at < 0) return s;
  const pages = cur.pages.filter((p) => p.id !== id);
  const active = cur.active === id ? (pages[Math.min(at, pages.length - 1)]?.id ?? null) : cur.active;
  return put(s, key, { ...cur, pages, active, open: cur.open && pages.length > 0 });
}

export function select(s: PagesByKey, key: string, id: string): PagesByKey {
  const cur = s[key];
  if (!cur?.pages.some((p) => p.id === id)) return s;
  return { ...s, [key]: { ...cur, active: id, open: true } };
}

export function setOpen(s: PagesByKey, key: string, open: boolean): PagesByKey {
  const cur = s[key];
  return cur && cur.open !== open ? { ...s, [key]: { ...cur, open } } : s;
}

export function setRatio(s: PagesByKey, key: string, ratio: number): PagesByKey {
  const cur = s[key];
  return cur ? { ...s, [key]: { ...cur, ratio: Math.min(0.8, Math.max(0.2, ratio)) } } : s;
}

export function patchPage(s: PagesByKey, id: string, patch: Partial<Omit<Page, 'id'>>): PagesByKey {
  for (const [key, cur] of Object.entries(s)) {
    const p = cur.pages.find((x) => x.id === id);
    if (!p) continue;
    const next = { ...p, ...patch };
    if (next.url === p.url && next.title === p.title) return s;
    return { ...s, [key]: { ...cur, pages: cur.pages.map((x) => (x.id === id ? next : x)) } };
  }
  return s;
}

export function prune(s: PagesByKey, alive: ReadonlySet<string>): PagesByKey {
  const kept = Object.entries(s).filter(([k]) => alive.has(k));
  return kept.length === Object.keys(s).length ? s : Object.fromEntries(kept);
}

export function pageIds(s: PagesByKey): Set<string> {
  return new Set(Object.values(s).flatMap((w) => w.pages.map((p) => p.id)));
}

export function touch(live: readonly string[], id: string, max = MAX_LIVE): { live: string[]; evicted: string[] } {
  const next = [...live.filter((x) => x !== id), id];
  const cut = Math.max(0, next.length - Math.max(1, max));
  return { live: next.slice(cut), evicted: next.slice(0, cut) };
}

function validWeb(v: unknown): SessionWeb | null {
  if (!v || typeof v !== 'object') return null;
  const w = v as Partial<SessionWeb>;
  const pages = (Array.isArray(w.pages) ? w.pages : [])
    .filter((p): p is Page => !!p && typeof p.id === 'string' && ID.test(p.id) && typeof p.url === 'string' && /^https?:\/\//i.test(p.url))
    .slice(-MAX_PAGES)
    .map((p) => (typeof p.title === 'string' ? { id: p.id, url: p.url, title: p.title } : { id: p.id, url: p.url }));
  if (!pages.length) return null;
  const active = pages.some((p) => p.id === w.active) ? w.active! : pages[pages.length - 1].id;
  const ratio = typeof w.ratio === 'number' && Number.isFinite(w.ratio) ? Math.min(0.8, Math.max(0.2, w.ratio)) : DEFAULT_RATIO;
  return { pages, active, open: w.open !== false, ratio };
}

export function parsePages(v: unknown): PagesByKey {
  if (!v || typeof v !== 'object') return {};
  const out: [string, SessionWeb][] = [];
  for (const [k, w] of Object.entries(v)) {
    const web = validWeb(w);
    if (web) out.push([k, web]);
  }
  return Object.fromEntries(out.slice(-MAX_SESSIONS));
}

function loadAll(): Record<string, unknown> {
  const v = readJson(KEY);
  return v && typeof v === 'object' ? (v as Record<string, unknown>) : {};
}

export function loadPages(host: string): PagesByKey {
  return parsePages(loadAll()[host]);
}

export function savePages(host: string, s: PagesByKey) {
  try {
    const { [host]: _, ...rest } = loadAll();
    const kept = Object.entries(rest).slice(-(MAX_HOSTS - 1));
    const all = Object.keys(s).length ? [...kept, [host, s]] : kept;
    writeJson(KEY, Object.fromEntries(all));
  } catch {}
}
