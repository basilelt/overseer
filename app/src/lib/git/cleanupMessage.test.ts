import { describe, expect, it } from 'vitest';
import { cleanedMessage } from './cleanup';

describe('cleanedMessage', () => {
  it('uses the singular for one worktree', () => {
    expect(cleanedMessage(1)).toBe('1 worktree cleaned up');
  });

  it('uses the plural for several worktrees', () => {
    expect(cleanedMessage(3)).toBe('3 worktrees cleaned up');
  });
});
