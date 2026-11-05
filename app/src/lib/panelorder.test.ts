import { describe, expect, it } from 'vitest';
import { moveSection, normalizeOrder } from './panelorder';

describe('side panel order', () => {
  it('fills missing sections and drops unknown or duplicate ones', () => {
    expect(normalizeOrder(undefined)).toEqual(['last', 'log', 'todos', 'files', 'diff']);
    expect(normalizeOrder(['diff', 'nope', 'diff', 'log'])).toEqual(['diff', 'log', 'last', 'todos', 'files']);
  });

  it('moves a section before or after another', () => {
    expect(moveSection(undefined, 'diff', 'last', false)).toEqual(['diff', 'last', 'log', 'todos', 'files']);
    expect(moveSection(undefined, 'last', 'files', true)).toEqual(['log', 'todos', 'files', 'last', 'diff']);
    expect(moveSection(undefined, 'log', 'log', true)).toEqual(['last', 'log', 'todos', 'files', 'diff']);
  });
});
