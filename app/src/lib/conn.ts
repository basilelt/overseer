import type { ConnPhase, ConnState, HostTarget } from './types';

export type StepId = 'connecting' | 'checking' | 'downloading' | 'installing' | 'starting' | 'connected';
export type StepStatus = 'pending' | 'active' | 'done' | 'failed' | 'skipped';

export interface Step {
  id: StepId;
  status: StepStatus;
}

const ALL: StepId[] = ['connecting', 'checking', 'downloading', 'installing', 'starting', 'connected'];

export function steps(s: ConnState, reached: ConnPhase[] = [s.phase]): Step[] {
  const order = reached.includes('downloading') || s.phase === 'downloading' ? ALL : ALL.filter((id) => id !== 'downloading');
  const index = (phase: ConnPhase) => order.indexOf(phase as StepId);
  const installed = reached.includes('installing');
  const furthest = Math.max(-1, ...reached.map(index));
  if (s.phase === 'connected') {
    return order.map((id) => ({ id, status: id === 'installing' && !installed ? 'skipped' : 'done' }));
  }
  const cur = s.phase === 'error' || s.phase === 'reconnecting' ? furthest : index(s.phase);
  return order.map((id, i) => {
    if (id === 'installing' && !installed && cur > i) return { id, status: 'skipped' };
    if (i < cur) return { id, status: 'done' };
    if (i === cur) return { id, status: s.phase === 'error' ? 'failed' : 'active' };
    if (s.phase === 'error' && cur < 0 && i === 0) return { id, status: 'failed' };
    return { id, status: 'pending' };
  });
}

export function isBusy(phase: ConnPhase): boolean {
  return phase === 'connecting' || phase === 'checking' || phase === 'downloading' || phase === 'installing' || phase === 'starting' || phase === 'reconnecting';
}

export function backoffMs(attempt: number, rand: () => number = Math.random): number {
  const base = Math.min(30_000, 500 * 2 ** Math.max(0, attempt));
  return Math.round(base / 2 + rand() * (base / 2));
}

export const ATTACH_TRIES = 6;
export const ATTACH_STABLE_MS = 10_000;

export function attachRetryMs(attempt: number, exited: boolean, error?: unknown, rand: () => number = Math.random): number | null {
  if (exited || attempt >= ATTACH_TRIES) return null;
  if (error !== undefined && /\b404\b/.test(String((error as Error)?.message ?? error))) return null;
  return backoffMs(attempt, rand);
}

export function attemptAfterDetach(attempt: number, upMs: number): number {
  return upMs >= ATTACH_STABLE_MS ? 0 : attempt;
}

export interface Startup {
  mobile: boolean;
  lastHost?: HostTarget;
  lastDirect?: HostTarget;
  explicitlyDisconnected?: boolean;
  savedHosts: { id: string; label: string }[];
  mock: boolean;
  secondary?: boolean;
}

export const LOCAL_TARGET: HostTarget = { kind: 'local', label: 'Local' };

export function startupTarget(s: Startup): HostTarget | null {
  if (!s.mobile) {
    if (s.secondary) return null;
    if (s.explicitlyDisconnected || s.lastHost?.kind !== 'ssh' || !s.lastHost.host) return LOCAL_TARGET;
    return s.lastHost;
  }
  if (s.explicitlyDisconnected) return null;
  const want = s.lastHost?.kind === 'direct' ? s.lastHost.host : s.lastDirect?.host;
  const last = want ? s.savedHosts.find((h) => h.id === want) : undefined;
  const pick = last ?? (s.mock ? s.savedHosts[0] : undefined);
  return pick ? { kind: 'direct', host: pick.id, label: pick.label } : null;
}
