import { describe, expect, it } from 'vitest';
import source from './EmptyPane.svelte?raw';
import { declOf } from '../../../test/css';

describe('CSS guard: empty pane in a narrow pane', () => {
  it('keeps the action row inside the pane padding and lets each button shrink', () => {
    expect(declOf(source, '.actions', 'max-width')).toBe('100%');
    expect(declOf(source, '.actions .btn', 'min-width')).toBe('0');
    expect(declOf(source, '.actions .btn', 'max-width')).toBe('100%');
  });

  it('cuts a long button label with an ellipsis instead of pushing it past the edge', () => {
    expect(declOf(source, '.lbl', 'overflow')).toBe('hidden');
    expect(declOf(source, '.lbl', 'text-overflow')).toBe('ellipsis');
    expect(declOf(source, '.lbl', 'min-width')).toBe('0');
  });

  it('keeps horizontal padding on the pane', () => {
    expect(declOf(source, '.empty-pane', 'padding')).toBe('24px 16px');
  });
});
