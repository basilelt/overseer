import { describe, expect, it } from 'vitest';
import { declOf } from '../../../test/css';
import toast from './Toast.svelte?raw';

describe('CSS guard: toast position', () => {
  it('clears the composer on every layout', () => {
    expect(declOf(toast, '.toast', 'bottom')).toBe('calc(16px + var(--composer-h, 0px))');
  });
});
