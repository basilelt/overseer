import { describe, expect, it } from 'vitest';
import { declOf } from '../../../test/css';
import scope from './ReviewScope.svelte?raw';

describe('CSS guard: review worktree scope', () => {
  it('clips its row on the phone, where WebKit counts the long option text of the select as page overflow', () => {
    expect(declOf(scope, ':global(.phone) .scope', 'overflow')).toBe('hidden');
  });
});
