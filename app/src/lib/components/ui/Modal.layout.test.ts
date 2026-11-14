import { describe, expect, it } from 'vitest';
import { declOf } from '../../../test/css';
import modal from './Modal.svelte?raw';

describe('CSS guard: modal footer', () => {
  it('wraps its buttons instead of pushing the first one out of the dialog when a translation is long', () => {
    expect(declOf(modal, 'footer', 'flex-wrap')).toBe('wrap');
  });
});
