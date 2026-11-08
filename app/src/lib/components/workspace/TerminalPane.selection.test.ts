import { describe, expect, it } from 'vitest';
import source from './TerminalPane.svelte?raw';

describe('TerminalPane interrupted selection', () => {
  it('finishes terminal selection on focus loss or a canceled pointer gesture', () => {
    expect(source).toContain('finishSelectionOnInterruption(window, el, document)');
    expect(source).toMatch(/return \(\) => \{\s*stopInterruptedSelection\(\);/);
  });
});
