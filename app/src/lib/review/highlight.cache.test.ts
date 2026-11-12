import { describe, expect, it } from 'vitest';
import { cachedHighlight, highlightCode } from './highlight';

describe('highlight cache', () => {
  it('returns the last highlighted HTML synchronously', async () => {
    const code = 'fn main() { let x = 1; }';
    expect(cachedHighlight(code, 'rust')).toBeNull();
    const html = await highlightCode(code, 'rust');
    expect(html).toContain('hljs-keyword');
    expect(cachedHighlight(code, 'rust')).toBe(html);
  });

  it('ignores unknown languages', async () => {
    expect(await highlightCode('x', 'nope')).toBeNull();
    expect(cachedHighlight('x', 'nope')).toBeNull();
  });
});
