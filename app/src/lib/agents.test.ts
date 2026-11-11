import { describe, expect, it } from 'vitest';
import { adaptChoice, agentName, canInstall, efforts, hasModel, ordered, pickAgent, sessionsAvailable, splitInstalled, taskAgents } from './agents';
import { MOCK_AGENTS } from './mock';

const by = (id: string) => MOCK_AGENTS.find((a) => a.id === id)!;

describe('agents', () => {
  it('splits installed agents from missing ones in daemon order', () => {
    const { installed, missing } = splitInstalled(MOCK_AGENTS);
    expect(installed.every((a) => a.installed) && missing.every((a) => !a.installed)).toBe(true);
    expect([...installed, ...missing].map((a) => a.id)).toEqual(ordered(MOCK_AGENTS).map((a) => a.id));
    expect(missing.length).toBeGreaterThan(0);
    expect(splitInstalled(MOCK_AGENTS.filter((a) => a.installed)).missing).toEqual([]);
    expect(splitInstalled(MOCK_AGENTS.map((a) => ({ ...a, installed: false }))).installed).toEqual([]);
  });

  it('lists installed agents first in daemon order, tasks without the shell', () => {
    expect(ordered(MOCK_AGENTS).map((a) => a.id)).toEqual(['claude', 'codex', 'gemini', 'agy', 'copilot', 'shell', 'opencode', 'amp', 'cursor']);
    expect(taskAgents(MOCK_AGENTS).map((a) => a.id)).not.toContain('shell');
  });

  it('offers to install only an absent agent with a runnable install command', () => {
    expect(canInstall(by('amp'))).toBe(true);
    expect(canInstall(by('codex'))).toBe(false);
    expect(canInstall({ ...by('amp'), installable: undefined })).toBe(false);
    expect(canInstall({ ...by('amp'), install: undefined })).toBe(false);
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
