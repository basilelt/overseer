import { describe, expect, it } from 'vitest';
import source from './TerminalPane.svelte?raw';

function rule(selector: string): string {
  const style = source.slice(source.indexOf('<style'));
  const at = style.search(new RegExp(`\\n\\s*${selector.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}\\s*\\{`));
  expect(at).toBeGreaterThan(-1);
  return style.slice(at, style.indexOf('}', at));
}

describe('terminal container caret', () => {
  it('hides the native caret of the contenteditable container ghostty-web creates', () => {
    expect(rule('.term')).toMatch(/caret-color:\s*transparent/);
  });

  it('hides the caret of the input textarea', () => {
    expect(rule('.term :global(textarea)')).toMatch(/caret-color:\s*transparent/);
  });
});
