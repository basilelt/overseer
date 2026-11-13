import { describe, expect, it } from 'vitest';
import { declOf } from '../../../test/css';
import scope from './ReviewScope.svelte?raw';

describe('CSS guard: review worktree scope', () => {
  it('keeps the long option text of the select from widening the page in WebKit phones', () => {
    expect(declOf(scope, ':global(.phone) select', 'contain')).toBe('paint');
  });

  it('lets the select shrink inside its row and keeps its text off the arrow', () => {
    expect(declOf(scope, ':global(.phone) select', 'min-width')).toBe('0');
    expect(declOf(scope, ':global(.phone) select', 'max-width')).toBe('100%');
    expect(declOf(scope, ':global(.phone) select', 'padding-right')).toBe('28px');
    expect(declOf(scope, ':global(.phone) select', 'text-overflow')).toBe('ellipsis');
  });
});
