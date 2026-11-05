import { describe, expect, it } from 'vitest';
import type { ChatEntry, SubagentSummary } from '../types';
import { agentName, elapsedMs, formatElapsed, formatTokens, runningAgents } from './agents';

const sub = (o: Partial<SubagentSummary> = {}): SubagentSummary => ({
  agentId: 'a1',
  type: 'Explore',
  description: 'Map it',
  state: 'running',
  background: true,
  startedAt: '2026-10-30T10:00:00Z',
  endedAt: null,
  tokens: 0,
  toolCount: 0,
  lastActivity: null,
  ...o,
});

describe('subagent helpers', () => {
  it('measures elapsed time until now or the end', () => {
    const now = Date.parse('2026-10-30T10:04:02Z');
    expect(elapsedMs(sub(), now)).toBe(242_000);
    expect(elapsedMs(sub({ endedAt: '2026-10-30T10:00:30Z' }), now)).toBe(30_000);
    expect(elapsedMs(sub({ startedAt: null }), now)).toBe(0);
    expect(elapsedMs(sub({ startedAt: '2026-10-30T10:05:00Z' }), now)).toBe(0);
  });

  it('formats elapsed time like the CLI footer', () => {
    expect(formatElapsed(0)).toBe('0s');
    expect(formatElapsed(59_999)).toBe('59s');
    expect(formatElapsed(242_000)).toBe('4m 2s');
    expect(formatElapsed(3_725_000)).toBe('1h 2m');
  });

  it('formats token counts', () => {
    expect(formatTokens(950)).toBe('950');
    expect(formatTokens(88_300)).toBe('88.3k');
    expect(formatTokens(207_841)).toBe('208k');
    expect(formatTokens(1_250_000)).toBe('1.3M');
  });

  it('lists running agents in order and names them', () => {
    const e = (id: string, a?: SubagentSummary): ChatEntry => ({ id, seq: 0, rev: 0, at: null, kind: 'tool', tool: { name: 'Agent', kind: 'task', label: id, state: 'running', agent: a } });
    const list = runningAgents([e('t1', sub()), e('t2', sub({ state: 'done' })), e('t3'), e('t4', sub({ type: null }))]);
    expect(list.map((r) => r.id)).toEqual(['t1', 't4']);
    expect(agentName(list[0].agent)).toBe('Explore');
    expect(agentName(list[1].agent)).toBe('agent');
  });
});
