import { describe, expect, it } from 'vitest';
import { declOf } from '../../../test/css';
import shell from './MobileShell.svelte?raw';

const SHORT = '@media (max-height: 500px) and (orientation: landscape)';

describe('CSS guard: phone shell in a short landscape viewport', () => {
  it('puts the nav labels beside the icons so the bar takes 40 px instead of 56', () => {
    expect(declOf(shell, '.bottom button', 'flex-direction', SHORT)).toBe('row');
    expect(declOf(shell, '.bottom button', 'min-height', SHORT)).toBe('40px');
  });

  it('drops the workspace subtitle there to give the chat the height', () => {
    expect(declOf(shell, '.subtitle', 'display', SHORT)).toBe('none');
  });

  it('keeps the portrait nav at 56 px', () => {
    expect(declOf(shell, '.bottom button', 'min-height')).toBe('56px');
  });
});
