import type { ConnState, HookAgentStatus, HooksRun, HooksStatus } from './types';

export function hostId(conn: ConnState): string {
  return `${conn.target.kind}:${conn.target.host ?? ''}`;
}

export function hookable(s: HooksStatus | null): HookAgentStatus[] {
  return s?.agents.filter((a) => a.mechanism !== 'polling') ?? [];
}

export function hooksOffer(s: HooksStatus | null, conn: ConnState, never: string[], later: string[]): HookAgentStatus[] | null {
  if (conn.phase !== 'connected' || !s) return null;
  const id = hostId(conn);
  if (never.includes(id) || later.includes(id)) return null;
  const missing = hookable(s).filter((a) => a.configured && !a.installed);
  return missing.length ? missing : null;
}

export interface HooksOutcome {
  done: string[];
  failed: { agent: string; error: string }[];
  codexTrust: boolean;
}

export function outcome(run: HooksRun, names: (id: string) => string): HooksOutcome {
  const failed = run.results.filter((r) => r.error).map((r) => ({ agent: names(r.agent), error: r.error! }));
  const done = run.results.filter((r) => !r.error && !r.skipped).map((r) => names(r.agent));
  const codexTrust = run.results.some((r) => r.agent === 'codex' && r.written && !!r.note);
  return { done, failed, codexTrust };
}
