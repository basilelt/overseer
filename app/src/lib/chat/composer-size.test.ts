import { describe, expect, it } from 'vitest';
import { lineCount, shrank, STALE_SIZE } from './composer';

const mark = (text: string) => ({ length: text.length, lines: lineCount(text) });

describe('composer height shrink check', () => {
  it('shrinks when the text got shorter or has fewer lines', () => {
    expect(shrank(mark('hello world'), mark('hello'))).toBe(true);
    expect(shrank(mark('a\nb\nc'), mark('abc'))).toBe(true);
  });

  it('does not shrink when the text grew or stayed the same size', () => {
    expect(shrank(mark('hello'), mark('hello!'))).toBe(false);
    expect(shrank(mark('hello'), mark('hello'))).toBe(false);
    expect(shrank(mark(''), mark('a\nb'))).toBe(false);
  });

  it('always shrinks after the width changed, so wrapped lines are measured again', () => {
    expect(shrank(STALE_SIZE, mark('same text'))).toBe(true);
    expect(shrank(STALE_SIZE, mark(''))).toBe(true);
  });
});
