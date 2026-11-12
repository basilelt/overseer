import { describe, expect, it } from 'vitest';
import { agentName, isAgentId, isAgentItem, isShellId, isShellItem, SHORT_NAMES } from './agents';

describe('agent display names', () => {
  it('keeps the long names for the app and the short names for the system view', () => {
    expect(agentName([], 'claude')).toBe('Claude Code');
    expect(SHORT_NAMES.claude).toBe('Claude');
    expect(agentName([], 'gemini')).toBe('Gemini CLI');
    expect(SHORT_NAMES.gemini).toBe('Gemini');
    expect(agentName([], 'vibe')).toBe('Mistral Vibe');
    expect(SHORT_NAMES.vibe).toBe('Vibe');
  });
});

describe('shell versus agent', () => {
  it('tells a shell id from an agent id', () => {
    expect(isShellId('shell')).toBe(true);
    expect(isShellId('claude')).toBe(false);
    expect(isShellId(null)).toBe(false);
    expect(isAgentId('claude')).toBe(true);
    expect(isAgentId('shell')).toBe(false);
  });

  it('tells a shell session from an agent session by kind', () => {
    expect(isShellItem({ kind: 'shell' })).toBe(true);
    expect(isShellItem({ kind: 'codex' })).toBe(false);
    expect(isAgentItem({ kind: 'codex' })).toBe(true);
    expect(isAgentItem({ kind: 'shell' })).toBe(false);
  });
});
