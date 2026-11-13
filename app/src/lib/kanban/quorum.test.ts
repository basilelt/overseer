import { describe, expect, it } from 'vitest';
import { FALLBACK_AGENTS } from '../agents';
import { approvals, canOrchestrate, defaultQuorum, fromOverseer, isReviewer, quorumBody, rejections, tally, toggleReviewer, toggleWorker } from './quorum';
import type { AgentInfo, Task, TaskVotes } from '../types';

const agent = (id: string, installed = true): AgentInfo => ({ id, name: id, installed, efforts: [], resume: false, verified: true, sessions: false, turns: false });
const fleet = [agent('claude'), agent('codex'), agent('gemini'), agent('amp', false), agent('shell')];

const votes = (approve: boolean[], outcome: TaskVotes['outcome'] = null): TaskVotes => ({
  round: 2,
  voters: 3,
  needed: 2,
  outcome,
  votes: approve.map((a, i) => ({ from: `t${i}`, number: i, agent: 'codex', role: 'reviewer', approve: a, reason: '' })),
});

describe('quorum orchestrator', () => {
  it('only lets claude and codex orchestrate', () => {
    expect(['claude', 'codex', 'gemini', 'shell'].map(canOrchestrate)).toEqual([true, true, false, false]);
  });
});

describe('defaultQuorum', () => {
  it('uses every installed agent as a worker and two others as reviewers', () => {
    expect(defaultQuorum(fleet, 'claude')).toEqual({ workers: ['claude', 'codex', 'gemini'], reviewers: ['codex', 'gemini'] });
  });

  it('keeps the orchestrator out of the reviewers when another agent exists', () => {
    expect(defaultQuorum(fleet, 'codex').reviewers).toEqual(['claude', 'gemini']);
  });

  it('falls back to the orchestrator when it is the only agent', () => {
    expect(defaultQuorum([agent('claude'), agent('shell')], 'claude')).toEqual({ workers: ['claude'], reviewers: ['claude'] });
    expect(defaultQuorum([], 'codex')).toEqual({ workers: ['codex'], reviewers: ['codex'] });
    expect(defaultQuorum(FALLBACK_AGENTS, 'claude').reviewers).toEqual(['codex']);
  });
});

describe('toggles', () => {
  const q = { workers: ['claude', 'codex'], reviewers: ['codex'] };

  it('adds and removes workers but keeps one', () => {
    expect(toggleWorker(q, 'gemini').workers).toEqual(['claude', 'codex', 'gemini']);
    expect(toggleWorker(q, 'claude').workers).toEqual(['codex']);
    expect(toggleWorker({ ...q, workers: ['claude'] }, 'claude').workers).toEqual(['claude']);
  });

  it('caps reviewers at two and keeps one', () => {
    const two = toggleReviewer(q, 'gemini');
    expect(two.reviewers).toEqual(['codex', 'gemini']);
    expect(toggleReviewer(two, 'claude').reviewers).toEqual(['codex', 'gemini']);
    expect(toggleReviewer(two, 'codex').reviewers).toEqual(['gemini']);
    expect(toggleReviewer(q, 'codex').reviewers).toEqual(['codex']);
  });

  it('does not mutate its input', () => {
    toggleWorker(q, 'gemini');
    expect(q.workers).toEqual(['claude', 'codex']);
  });
});

describe('quorumBody', () => {
  const q = { workers: ['claude'], reviewers: ['codex'] };

  it('sends a copy of the quorum only when on and the agent can orchestrate', () => {
    expect(quorumBody(true, 'claude', q)).toEqual({ quorum: q });
    expect(quorumBody(true, 'claude', q).quorum).not.toBe(q);
    expect(quorumBody(false, 'claude', q)).toEqual({});
    expect(quorumBody(true, 'gemini', q)).toEqual({});
  });
});

describe('tally', () => {
  it('counts approvals and rejections', () => {
    expect(approvals(votes([true, false, true]))).toBe(2);
    expect(rejections(votes([true, false, true]))).toBe(1);
  });

  it('is voting until the daemon decides', () => {
    expect(tally(votes([true]))).toEqual({ key: 'voting', approved: 1, voters: 3, round: 2 });
    expect(tally(votes([true, true], 'accepted')).key).toBe('accepted');
    expect(tally(votes([false, false], 'rejected')).key).toBe('rejected');
  });

  it('spots reviewer tasks', () => {
    expect(isReviewer({ origin: 'reviewer' } as Task)).toBe(true);
    expect(isReviewer({ origin: 'agent' } as Task)).toBe(false);
  });
});

describe('fromOverseer', () => {
  it('marks the daemon quorum and overlap notices as sent by Overseer', () => {
    expect(fromOverseer({ from: null, body: 'Quorum: subtask #2 "x" is accepted' })).toBe(true);
    expect(fromOverseer({ from: null, body: 'Overlap: task #3 "a" and task #4 "b" both change: x' })).toBe(true);
  });

  it('keeps the user messages and the task messages as they are', () => {
    expect(fromOverseer({ from: null, body: 'please add tests' })).toBe(false);
    expect(fromOverseer({ from: 'T1', body: 'Quorum: from an agent' })).toBe(false);
  });
});
