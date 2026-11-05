import { describe, expect, it } from 'vitest';
import source from './TaskCard.svelte?raw';

const style = source.slice(source.indexOf('<style>'));

function rule(selector: string): string {
  const start = style.indexOf(`\n  ${selector} {`);
  expect(start).toBeGreaterThan(-1);
  return style.slice(start, style.indexOf('}', start));
}

describe('TaskCard hover actions layout', () => {
  it('keeps the desktop actions in the meta row flow so they never cover chips', () => {
    const actions = rule('.actions');
    expect(actions).not.toMatch(/position:\s*absolute/);
    expect(actions).toContain('flex: none');
  });

  it('keeps the effort chip from shrinking', () => {
    expect(rule('.effort')).toContain('flex: none');
  });

  it('only fades the actions in on hover and focus', () => {
    expect(rule('.actions')).toContain('opacity: 0');
    expect(style).toMatch(/\.card:hover \.actions/);
  });
});
