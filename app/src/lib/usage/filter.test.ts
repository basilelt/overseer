import { describe, expect, it } from 'vitest';
import type { AgentInfo } from '../types';
import { toggleUsageAgentHidden, usageAgentCards, visibleUsageGroups } from './filter';
import type { UsageAgent, UsageGroup } from './types';

const agents: AgentInfo[] = [
  { id: 'claude', name: 'Claude Code', installed: true, efforts: [], resume: true, verified: true, sessions: true, turns: true },
  { id: 'codex', name: 'Codex', installed: true, efforts: [], resume: true, verified: false, sessions: true, turns: true },
  { id: 'vibe', name: 'Mistral Vibe', installed: true, efforts: [], resume: true, verified: true, sessions: true, turns: true },
  { id: 'amp', name: 'Amp', installed: false, efforts: [], resume: true, verified: false, sessions: true, turns: true },
  { id: 'shell', name: 'Shell', installed: true, efforts: [], resume: false, verified: true, sessions: false, turns: false },
];

const usageOf = (agent: string, total: number): UsageAgent => ({
  agent,
  today: { input: 0, output: 0, cacheRead: 0, cacheWrite: 0, total, cost: 0, costPartial: false, messages: 1, activeMs: 1 },
  limits: { source: null, windows: [], hit: null },
});

describe('usageAgentCards', () => {
  it('lists every supported CLI except the shell, in agent order, with empty data when the daemon has none', () => {
    const cards = usageAgentCards(agents, [usageOf('claude', 42)], undefined);
    expect(cards.map((c) => c.id)).toEqual(['claude', 'codex', 'vibe', 'amp']);
    expect(cards[0].usage.today.total).toBe(42);
    expect(cards[2].name).toBe('Mistral Vibe');
    expect(cards[2].usage.today.total).toBe(0);
    expect(cards[2].usage.limits.windows).toEqual([]);
    expect(cards[3].installed).toBe(false);
  });

  it('hides the agents the user turned off', () => {
    const cards = usageAgentCards(agents, [usageOf('codex', 7)], ['codex', 'amp']);
    expect(cards.map((c) => c.id)).toEqual(['claude', 'vibe']);
  });

  it('keeps usage entries for agents missing from the agent list at the end', () => {
    const cards = usageAgentCards(agents, [usageOf('other', 5)], ['vibe']);
    expect(cards.map((c) => c.id)).toEqual(['claude', 'codex', 'amp', 'other']);
    expect(cards[3].usage.today.total).toBe(5);
  });

  it('works with no usage data at all', () => {
    const cards = usageAgentCards(agents, null, []);
    expect(cards).toHaveLength(4);
    expect(cards.every((c) => c.usage.today.total === 0)).toBe(true);
  });
});

describe('visibleUsageGroups', () => {
  const groups: UsageGroup[] = [
    { key: 'claude', agent: 'claude', usage: usageOf('claude', 1).today },
    { key: 'vibe:s1', agent: 'vibe', sessionId: 's1', usage: usageOf('vibe', 2).today },
    { key: 'proj', project: 'proj', usage: usageOf('x', 3).today },
  ];

  it('drops rows whose agent is hidden and keeps rows without an agent', () => {
    expect(visibleUsageGroups(groups, ['vibe']).map((g) => g.key)).toEqual(['claude', 'proj']);
  });

  it('keeps everything when nothing is hidden', () => {
    expect(visibleUsageGroups(groups, undefined)).toHaveLength(3);
    expect(visibleUsageGroups(groups, [])).toHaveLength(3);
  });
});

describe('toggleUsageAgentHidden', () => {
  it('adds a missing id and removes a present one, sorted', () => {
    expect(toggleUsageAgentHidden(undefined, 'vibe')).toEqual(['vibe']);
    expect(toggleUsageAgentHidden(['amp', 'vibe'], 'vibe')).toEqual(['amp']);
    expect(toggleUsageAgentHidden(['vibe'], 'amp')).toEqual(['amp', 'vibe']);
    expect(toggleUsageAgentHidden([], 'x')).toEqual(['x']);
  });
});
