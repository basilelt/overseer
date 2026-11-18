import { describe, expect, it } from 'vitest';
import source from './FileViewer.svelte?raw';

describe('FileViewer', () => {
  it('lets the user select the text of the opened file, which the body turns off', () => {
    const rule = /\n {2}\.file-viewer \{([^}]*)\}/.exec(source)?.[1] ?? '';
    expect(rule).toContain('user-select: text;');
    expect(rule).toContain('-webkit-user-select: text;');
  });
});
