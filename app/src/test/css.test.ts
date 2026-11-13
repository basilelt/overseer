import { describe, expect, it } from 'vitest';
import { declOf, parseStyle } from './css';

const src = `<div class="a"></div>
<style>
  /* c { x: y } */
  .a, .b > .c { color: red; margin: 0 4px; }
  .a { color: blue }
  @media (hover: none) { .a { opacity: 1; } }
  :global(:root.phone) .a { min-height: var(--touch); }
</style>`;

describe('css helper', () => {
  it('returns the last declaration of an exact selector, per at-rule', () => {
    expect(declOf(src, '.a', 'color')).toBe('blue');
    expect(declOf(src, '.a', 'margin')).toBe('0 4px');
    expect(declOf(src, '.b > .c', 'color')).toBe('red');
    expect(declOf(src, '.a', 'opacity')).toBeUndefined();
    expect(declOf(src, '.a', 'opacity', '@media (hover: none)')).toBe('1');
  });

  it('matches selectors exactly, ignores comments and keeps :global selectors whole', () => {
    expect(declOf(src, 'c', 'x')).toBeUndefined();
    expect(declOf(src, ':global(:root.phone) .a', 'min-height')).toBe('var(--touch)');
    expect(parseStyle(src).map((r) => r.selector)).toEqual(['.a', '.b > .c', '.a', '.a', ':global(:root.phone) .a']);
  });
});
