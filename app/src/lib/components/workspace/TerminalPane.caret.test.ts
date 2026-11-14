import { describe, expect, it } from 'vitest';
import { declOf } from '../../../test/css';
import source from './TerminalPane.svelte?raw';

describe('CSS guard: terminal caret', () => {
  it('hides the native caret of the contenteditable container ghostty-web creates', () => {
    expect(declOf(source, '.term', 'caret-color')).toBe('transparent');
  });

  it('hides the caret of the input textarea', () => {
    expect(declOf(source, '.term :global(textarea)', 'caret-color')).toBe('transparent');
  });
});
