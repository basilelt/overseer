import { describe, expect, it } from 'vitest';
import { adaptChoice, agentName, canInstall, efforts, hasModel, ordered, pickAgent, sessionsAvailable, shortVersion, splitInstalled, taskAgents } from './agents';
import { MOCK_AGENTS } from './daemon/mock';

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
    expect(ordered(MOCK_AGENTS).map((a) => a.id)).toEqual(['claude', 'codex', 'gemini', 'agy', 'copilot', 'vibe', 'shell', 'opencode', 'amp', 'cursor']);
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
    expect(agentName([], 'vibe')).toBe('Mistral Vibe');
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

describe('shortVersion', () => {
  it('keeps only the version number of what each CLI prints for --version', () => {
    expect(shortVersion('2.1.330 (Claude Code)')).toBe('2.1.330');
    expect(shortVersion('codex-cli 0.167.0')).toBe('0.167.0');
    expect(shortVersion('GitHub Copilot CLI 1.0.107.')).toBe('1.0.107');
    expect(shortVersion('vibe 3.1.3')).toBe('3.1.3');
    expect(shortVersion('0.0.1793868001-g0e9889 (released 2026-11-05T08:40:01.000Z, 16d ago)')).toBe('0.0.1793868001-g0e9889');
    expect(shortVersion('2026.11.05-86047df')).toBe('2026.11.05-86047df');
  });
  it('falls back to the first word, and to nothing without a version', () => {
    expect(shortVersion('nightly build')).toBe('nightly');
    expect(shortVersion('')).toBeNull();
    expect(shortVersion(null)).toBeNull();
    expect(shortVersion(undefined)).toBeNull();
  });
});
