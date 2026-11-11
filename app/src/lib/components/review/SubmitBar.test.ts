import { describe, expect, it } from 'vitest';
import bar from './SubmitBar.svelte?raw';

describe('review submit bar', () => {
  it('wraps its buttons in a narrow pane instead of running under the next panel', () => {
    const style = bar.slice(bar.indexOf('<style'));
    const at = style.indexOf('\n  .row {');
    expect(style.slice(at, style.indexOf('}', at))).toMatch(/flex-wrap:\s*wrap;/);
  });
});
