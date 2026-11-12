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

export function toggled(list: string[] | undefined, key: string): string[] {
  const cur = list ?? [];
  return cur.includes(key) ? cur.filter((k) => k !== key) : [...cur, key];
}
