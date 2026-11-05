import { hostId } from './hooks';
import type { Key } from './i18n.svelte';
import type { ConnState, SwitchboardStatus } from './types';

export const DISMISS_KEY = 'overseer.switchboardImport';

export function shouldOffer(s: SwitchboardStatus | null, conn: ConnState, dismissed: boolean, offered: string[]): boolean {
  if (conn.phase !== 'connected' || !s || !s.found || s.imported || dismissed) return false;
  return !offered.includes(hostId(conn));
}

export function isDismissed(): boolean {
  try {
    return localStorage.getItem(DISMISS_KEY) === 'dismissed';
  } catch {
    return false;
  }
}

export function dismissForever() {
  try {
    localStorage.setItem(DISMISS_KEY, 'dismissed');
  } catch {
    return;
  }
}

export function applySettings(entries: Record<string, string> | undefined) {
  for (const [k, v] of Object.entries(entries ?? {})) {
    try {
      localStorage.setItem(k, v);
    } catch {
      return;
    }
  }
}

type Counts = { sessions: number; tasks: number; workspaces: number };

export function countKeys(r: Counts, tr: (key: Key, vars: { n: number }) => string): Record<keyof Counts, string> {
  const one = (k: keyof Counts) => tr(r[k] === 1 ? `switchboard.${k}One` : `switchboard.${k}`, { n: r[k] });
  return { sessions: one('sessions'), tasks: one('tasks'), workspaces: one('workspaces') };
}

export function canImport(s: SwitchboardStatus | null): boolean {
  return !!s?.found && !s.imported;
}
