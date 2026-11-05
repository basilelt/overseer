import type { ChatEntry, SubagentSummary } from '../types';

export interface RunningAgent {
  id: string;
  agent: SubagentSummary;
}

export function elapsedMs(a: SubagentSummary, now: number): number {
  const start = a.startedAt ? Date.parse(a.startedAt) : NaN;
  if (Number.isNaN(start)) return 0;
  const end = a.endedAt ? Date.parse(a.endedAt) : now;
  return Math.max(0, (Number.isNaN(end) ? now : end) - start);
}

export function formatElapsed(ms: number): string {
  const s = Math.floor(ms / 1000);
  if (s < 60) return `${s}s`;
  const m = Math.floor(s / 60);
  if (m < 60) return `${m}m ${s % 60}s`;
  return `${Math.floor(m / 60)}h ${m % 60}m`;
}

export function formatTokens(n: number): string {
  if (n < 1000) return String(n);
  if (n < 1_000_000) return `${(n / 1000).toFixed(n < 100_000 ? 1 : 0)}k`;
  return `${(n / 1_000_000).toFixed(1)}M`;
}

export function runningAgents(entries: ChatEntry[]): RunningAgent[] {
  const out: RunningAgent[] = [];
  for (const e of entries) {
    const a = e.tool?.agent;
    if (a?.state === 'running') out.push({ id: e.id, agent: a });
  }
  return out;
}

export function agentName(a: SubagentSummary): string {
  return a.type || 'agent';
}
