import { describe, expect, it } from 'vitest';
import { cachedHighlight, highlightCode } from './highlight';

describe('highlight cache', () => {
  // Given a code block highlighted once
  // When the same block is mounted again (tab switch)
  // Then its HTML is available synchronously, without a new hljs pass
  it('returns the last highlighted HTML synchronously', async () => {
    const code = 'fn main() { let x = 1; }';
    expect(cachedHighlight(code, 'rust')).toBeNull();
    const html = await highlightCode(code, 'rust');
    expect(html).toContain('hljs-keyword');
    expect(cachedHighlight(code, 'rust')).toBe(html);
  });

  // Given an unknown language
  // When it is looked up
  // Then nothing is cached
  it('ignores unknown languages', async () => {
    expect(await highlightCode('x', 'nope')).toBeNull();
    expect(cachedHighlight('x', 'nope')).toBeNull();
  });
});
