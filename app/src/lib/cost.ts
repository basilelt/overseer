import type { Task, TaskStatus, TokenSplit } from './types';

export type CostBy = 'profile' | 'ticket' | 'task';

export interface CostRow {
  key: string;
  label: string;
  agent?: string;
  taskId?: string;
  tasks: number;
  finished: number;
  cost: number | null;
  partial: boolean;
  perTask: number | null;
  tokens: number;
}

const FINISHED: readonly TaskStatus[] = ['in_testing', 'merged', 'completed'];
const MAX_ROWS = 50;

export function tokensOf(c: TokenSplit | null | undefined): number {
  return c ? c.input + c.cacheWrite + c.cacheRead + c.output : 0;
}

export function isFinished(t: Task): boolean {
  return FINISHED.includes(t.status);
}

export function costRows(tasks: readonly Task[], by: CostBy, profileName: (id: string) => string, noProfile: string): CostRow[] {
  const rows = new Map<string, CostRow & { finishedCost: number; finishedPriced: number }>();
  for (const t of tasks) {
    const c = t.cost;
    if (!c) continue;
    let key: string;
    let label: string;
    if (by === 'ticket') {
      if (!t.ticketKey) continue;
      key = label = t.ticketKey;
    } else if (by === 'task') {
      key = t.id;
      label = `#${t.number} ${t.title}`;
    } else {
      key = `${t.profile ?? ''}\u0000${t.agent}`;
      label = t.profile ? profileName(t.profile) : noProfile;
    }
    let r = rows.get(key);
    if (!r) {
      r = { key, label, tasks: 0, finished: 0, cost: null, partial: false, perTask: null, tokens: 0, finishedCost: 0, finishedPriced: 0 };
      if (by === 'profile') r.agent = t.agent;
      if (by === 'task') {
        r.agent = t.agent;
        r.taskId = t.id;
      }
      rows.set(key, r);
    }
    r.tasks += 1;
    r.tokens += tokensOf(c);
    r.partial ||= c.partial;
    if (c.cost != null) r.cost = (r.cost ?? 0) + c.cost;
    if (isFinished(t)) {
      r.finished += 1;
      if (c.cost != null) {
        r.finishedCost += c.cost;
        r.finishedPriced += 1;
      }
    }
  }
  return [...rows.values()]
    .map(({ finishedCost, finishedPriced, ...r }) => ({ ...r, perTask: finishedPriced ? finishedCost / finishedPriced : null }))
    .sort((a, b) => (b.cost ?? -1) - (a.cost ?? -1) || b.tokens - a.tokens || a.label.localeCompare(b.label))
    .slice(0, MAX_ROWS);
}
