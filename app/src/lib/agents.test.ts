import { describe, expect, it } from 'vitest';
import { adaptChoice, agentName, efforts, hasModel, ordered, pickAgent, sessionsAvailable, taskAgents } from './agents';
import { MOCK_AGENTS } from './mock';

const by = (id: string) => MOCK_AGENTS.find((a) => a.id === id)!;

describe('agents', () => {
  it('lists installed agents first in daemon order, tasks without the shell', () => {
    expect(ordered(MOCK_AGENTS).map((a) => a.id)).toEqual(['claude', 'codex', 'gemini', 'agy', 'copilot', 'shell', 'opencode', 'amp', 'cursor']);
    expect(taskAgents(MOCK_AGENTS).map((a) => a.id)).not.toContain('shell');
  });

  it('names agents from the daemon, then known ids, then the id', () => {
    expect(agentName(MOCK_AGENTS, 'gemini')).toBe('Gemini CLI');
    expect(agentName([], 'cursor')).toBe('Cursor Agent');
    expect(agentName([], 'mystery')).toBe('mystery');
  });

  it('distinguishes free text, suggestions and no model field', () => {
    expect(hasModel(by('codex'))).toBe(true);
    expect(hasModel(by('claude'))).toBe(true);
    expect(hasModel(by('amp'))).toBe(false);
    expect(hasModel(undefined)).toBe(true);
    expect(efforts(by('gemini'))).toEqual([]);
    expect(efforts(by('agy'))).toEqual(['low', 'medium', 'high', 'xhigh', 'max']);
  });

  it('picks the preferred agent when installed, else the first installed one', () => {
    expect(pickAgent(MOCK_AGENTS, 'codex')).toBe('codex');
    expect(pickAgent(MOCK_AGENTS, 'amp')).toBe('claude');
    expect(pickAgent(MOCK_AGENTS, 'shell')).toBe('claude');
    expect(pickAgent(MOCK_AGENTS, 'shell', false)).toBe('shell');
  });

  it('resets model and effort sensibly when the agent changes', () => {
    expect(adaptChoice(by('codex'), { model: 'opus', effort: 'max' })).toEqual({ model: '', effort: 'max' });
    expect(adaptChoice(by('claude'), { model: 'sonnet', effort: 'low' })).toEqual({ model: 'sonnet', effort: 'low' });
    expect(adaptChoice(by('gemini'), { model: 'opus', effort: 'max' })).toEqual({ model: '' });
    expect(adaptChoice(by('copilot'), { model: '' }, 'medium')).toEqual({ model: '', effort: 'medium' });
  });

  it('knows which agents have an activity panel', () => {
    expect(sessionsAvailable(MOCK_AGENTS, 'gemini')).toBe(true);
    expect(sessionsAvailable(MOCK_AGENTS, 'agy')).toBe(false);
    expect(sessionsAvailable([], 'claude')).toBe(true);
  });
});
