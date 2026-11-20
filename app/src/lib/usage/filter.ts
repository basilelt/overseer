import { isAgentId } from '../agents';
import type { AgentInfo } from '../types';
import type { UsageAgent, UsageGroup, UsageLimits, UsageTotals } from './types';

export const EMPTY_TOTALS: UsageTotals = {
  input: 0,
  output: 0,
  cacheRead: 0,
  cacheWrite: 0,
  total: 0,
  cost: 0,
  costPartial: false,
  messages: 0,
  activeMs: 0,
};

export const EMPTY_LIMITS: UsageLimits = { source: null, windows: [], hit: null };

export interface UsageAgentCard {
  id: string;
  name: string;
  installed: boolean;
  usage: UsageAgent;
}

function emptyUsage(id: string): UsageAgent {
  return { agent: id, today: EMPTY_TOTALS, limits: EMPTY_LIMITS };
}

/**
 * One card per CLI Overseer supports (in the order the daemon lists them), each
 * merged with its usage/limits from the daemon when present, minus the ids the
 * user hid in the Usage view. Usage entries for unknown ids are kept at the end.
 */
export function usageAgentCards(
  agents: AgentInfo[],
  usage: UsageAgent[] | null | undefined,
  hidden: string[] | null | undefined,
): UsageAgentCard[] {
  const hide = new Set(hidden ?? []);
  const byId = new Map((usage ?? []).map((u) => [u.agent, u]));
  const cards: UsageAgentCard[] = [];
  for (const a of agents) {
    if (!isAgentId(a.id) || hide.has(a.id)) continue;
    const u = byId.get(a.id) ?? emptyUsage(a.id);
    byId.delete(a.id);
    cards.push({ id: a.id, name: a.name, installed: a.installed, usage: u });
  }
  for (const u of byId.values()) {
    if (hide.has(u.agent)) continue;
    cards.push({ id: u.agent, name: u.agent, installed: false, usage: u });
  }
  return cards;
}

/** Breakdown rows whose agent is hidden are dropped; rows without an agent stay. */
export function visibleUsageGroups(groups: UsageGroup[], hidden: string[] | null | undefined): UsageGroup[] {
  const hide = new Set(hidden ?? []);
  return groups.filter((g) => !g.agent || !hide.has(g.agent));
}

/** Toggles an id in the hidden list, returning a sorted copy for storage. */
export function toggleUsageAgentHidden(hidden: string[] | null | undefined, id: string): string[] {
  const next = new Set(hidden ?? []);
  if (!next.delete(id)) next.add(id);
  return [...next].sort();
}
