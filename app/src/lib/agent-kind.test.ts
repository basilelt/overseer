import { describe, expect, it } from 'vitest';
import { isAgentId, isAgentItem, isShellId, isShellItem } from './agents';

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
