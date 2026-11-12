import { describe, expect, it } from 'vitest';
import { agentName, isAgentId, isAgentItem, isShellId, isShellItem, SHORT_NAMES } from './agents';

describe('agent display names', () => {
  it('abbreviates the long name of each agent in the short name for the system view', () => {
    for (const [id, short] of Object.entries(SHORT_NAMES)) {
      const long = agentName([], id);
      expect(long, id).toContain(short);
      expect(short.length, id).toBeLessThanOrEqual(long.length);
    }
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
