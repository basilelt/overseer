import { t } from './i18n.svelte';
import type { HostTarget } from './types';
import { hostKey } from './window';

export type HostWindows = 'separate' | 'shared';
export type HostChoice = 'add' | 'window';
/** replace: this window's host becomes the target (one window per host); add: shown beside the others; switch: already here. */
export type ConnectHow = 'replace' | 'add' | 'window' | 'ask' | 'switch';

export interface HostPrefs {
  hostWindows: HostWindows;
  hostAsk: boolean;
  hostChoice: Record<string, HostChoice>;
}

export interface WindowHosts {
  /** The host the main area shows, connected or not. */
  active: string;
  /** Every host of the window, the active one included. */
  keys: string[];
  /** The active host is reachable, so adding one keeps something on screen. */
  live: boolean;
}

export function connectHow(p: HostPrefs, target: Pick<HostTarget, 'kind' | 'host'>, w: WindowHosts, forced?: HostChoice): ConnectHow {
  const key = hostKey(target);
  if (key === w.active) return 'replace';
  if (w.keys.includes(key)) return 'switch';
  if (forced) return forced;
  if (!w.live) return 'replace';
  const kept = p.hostChoice[key];
  if (kept) return kept;
  if (p.hostAsk) return 'ask';
  return p.hostWindows === 'shared' || w.keys.length > 1 ? 'add' : 'replace';
}

/** The button the "ask" step puts first. */
export function preferredChoice(p: Pick<HostPrefs, 'hostWindows'>): HostChoice {
  return p.hostWindows === 'shared' ? 'add' : 'window';
}

// Lane 5 is reddish and would read as an error next to a status dot.
const LANES = 4;

/** Local keeps the "ready" green; every SSH host gets one of the lane colours, the same each launch. */
export function hostColor(key: string): string {
  if (key === 'local') return 'var(--st-input)';
  let h = 0;
  for (let i = 0; i < key.length; i++) h = (h * 31 + key.charCodeAt(i)) | 0;
  return `var(--lane-${(Math.abs(h) % LANES) + 1})`;
}

/** Hosts to bring back at launch beside the first one, without duplicates. */
export function restoreHosts(saved: HostTarget[] | undefined, first: HostTarget): HostTarget[] {
  const seen = new Set([hostKey(first)]);
  const out: HostTarget[] = [];
  for (const h of saved ?? []) {
    if (h.kind === 'direct') continue;
    const k = hostKey(h);
    if (seen.has(k)) continue;
    seen.add(k);
    out.push(h);
  }
  return out;
}

/** Keeps sections in the order hosts joined the window, whichever is active. */
export function ordered<T>(order: string[], items: T[], key: (t: T) => string): T[] {
  const at = (k: string) => {
    const i = order.indexOf(k);
    return i < 0 ? order.length : i;
  };
  return items.slice().sort((a, b) => at(key(a)) - at(key(b)));
}

export function toggled(list: string[] | undefined, key: string): string[] {
  const cur = list ?? [];
  return cur.includes(key) ? cur.filter((k) => k !== key) : [...cur, key];
}

export function hostName(target: HostTarget): string {
  return target.kind === 'local' ? t('host.local') : target.label || target.host || '';
}
