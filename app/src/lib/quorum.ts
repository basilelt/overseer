import type { AgentInfo, Quorum, Task, TaskVotes } from './types';
import { isAgentId } from './agents';

export const ORCHESTRATORS = ['claude', 'codex'];
export const MAX_REVIEWERS = 2;

export const canOrchestrate = (agent: string): boolean => ORCHESTRATORS.includes(agent);

const installed = (agents: AgentInfo[]) => agents.filter((a) => a.installed && isAgentId(a.id)).map((a) => a.id);

export function defaultQuorum(agents: AgentInfo[], orchestrator: string): Quorum {
  const ids = installed(agents);
  const others = ids.filter((id) => id !== orchestrator);
  const workers = ids.length ? ids : [orchestrator];
  const reviewers = (others.length ? others : [orchestrator]).slice(0, MAX_REVIEWERS);
  return { workers, reviewers };
}

function flip(list: string[], id: string, max: number): string[] {
  if (list.includes(id)) return list.length > 1 ? list.filter((x) => x !== id) : list;
  return list.length < max ? [...list, id] : list;
}

export const toggleWorker = (q: Quorum, id: string): Quorum => ({ ...q, workers: flip(q.workers, id, Infinity) });
export const toggleReviewer = (q: Quorum, id: string): Quorum => ({ ...q, reviewers: flip(q.reviewers, id, MAX_REVIEWERS) });

export function quorumBody(on: boolean, agent: string, q: Quorum): { quorum?: Quorum } {
  return on && canOrchestrate(agent) ? { quorum: { workers: [...q.workers], reviewers: [...q.reviewers] } } : {};
}

export function approvals(v: TaskVotes): number {
  return v.votes.filter((x) => x.approve).length;
}

export function rejections(v: TaskVotes): number {
  return v.votes.length - approvals(v);
}

export type Tally = { key: 'accepted' | 'rejected' | 'voting'; approved: number; voters: number; round: number };

export function tally(v: TaskVotes): Tally {
  return { key: v.outcome ?? 'voting', approved: approvals(v), voters: v.voters, round: v.round };
}

export const isReviewer = (t: Task): boolean => t.origin === 'reviewer';

const OVERSEER_PREFIXES = ['Quorum: ', 'Overlap: task #'];

export const fromOverseer = (m: { from: string | null; body: string }): boolean =>
  m.from === null && OVERSEER_PREFIXES.some((p) => m.body.startsWith(p));
