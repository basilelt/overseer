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

  it('never pulls the actions outside the meta row with a negative margin', () => {
    expect(rule('.actions')).not.toMatch(/margin[^:]*:\s*-/);
  });

  it('wraps the age and the actions together, pushed to the right edge', () => {
    const tail = rule('.tail');
    expect(tail).toContain('margin-left: auto');
    expect(tail).toContain('flex: none');
    expect(source).toMatch(/<span class="tail">\s*<span class="age[^>]*>[^<]*<\/span>\s*\{#if !ghost[^]*?<span class="actions">/);
  });

  it('lets the branch chip share the first row with the agent instead of taking a row alone', () => {
    const ref = rule('.ref');
    expect(ref).toContain('flex: 1 1 6em');
    expect(ref).toContain('min-width: 6em');
  });

  it('keeps the effort chip from shrinking', () => {
    expect(rule('.effort')).toContain('flex: none');
  });

  it('only fades the actions in on hover and focus', () => {
    expect(rule('.actions')).toContain('opacity: 0');
    expect(style).toMatch(/\.card:hover \.actions/);
  });
});
