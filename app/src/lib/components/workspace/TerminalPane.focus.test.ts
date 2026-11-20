import { describe, expect, it } from 'vitest';
import source from './TerminalPane.svelte?raw';

describe('source guard: terminal input focus', () => {
  it('keeps focus on the textarea ghostty-web types through, not its contenteditable container', () => {
    expect(source).toMatch(/keepInputFocus\(el, area\)/);
  });

  it('does not pull focus back to the container when the textarea inside it already has it', () => {
    expect(source).toMatch(/!el\.contains\(document\.activeElement\)\) el\.focus/);
  });
});
