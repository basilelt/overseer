import { describe, expect, it } from 'vitest';
import { inline, parseMarkdown, parseMarkdownCached } from './markdown';

describe('chat markdown', () => {
  it('renders paragraphs, headings, lists and code fences', () => {
    const b = parseMarkdown('# Title\n\nSome **bold** and *it* and `code`.\nnext line\n\n- one\n- two\n\n1. a\n2. b\n\n```rust\nfn x() {}\n```\n\n> quoted\n\n---');
    expect(b[0]).toEqual({ type: 'h', level: 1, html: 'Title' });
    expect(b[1]).toEqual({ type: 'p', html: 'Some <strong>bold</strong> and <em>it</em> and <code>code</code>.<br>next line' });
    expect(b[2]).toEqual({ type: 'list', ordered: false, start: 1, items: ['one', 'two'] });
    expect(b[3]).toEqual({ type: 'list', ordered: true, start: 1, items: ['a', 'b'] });
    expect(b[4]).toEqual({ type: 'code', lang: 'rust', code: 'fn x() {}' });
    expect(b[5]).toEqual({ type: 'quote', html: 'quoted' });
    expect(b[6]).toEqual({ type: 'hr' });
  });

  it('keeps an unterminated fence as code to the end', () => {
    expect(parseMarkdown('```\nlet a = 1;\nlet b')).toEqual([{ type: 'code', lang: '', code: 'let a = 1;\nlet b' }]);
  });

  it('renders tables', () => {
    expect(parseMarkdown('| a | b |\n|---|---|\n| 1 | `2` |')).toEqual([{ type: 'table', head: ['a', 'b'], rows: [['1', '<code>2</code>']] }]);
  });

  it('never passes raw HTML through', () => {
    expect(inline('<img src=x onerror=alert(1)>')).toBe('&lt;img src=x onerror=alert(1)&gt;');
    expect(inline('`<script>`')).toBe('<code>&lt;script&gt;</code>');
    expect(parseMarkdown('<div onclick="x">hi</div>')[0]).toEqual({ type: 'p', html: '&lt;div onclick=&quot;x&quot;&gt;hi&lt;/div&gt;' });
  });

  it('only links http(s) and mailto urls', () => {
    expect(inline('[docs](https://a.b/c?x=1&y=2)')).toBe('<a href="https://a.b/c?x=1&amp;y=2" target="_blank" rel="noopener noreferrer">docs</a>');
    expect(inline('[x](javascript:alert(1))')).not.toContain('href');
    expect(inline('[x](https://a.b/"onmouseover="y)')).not.toContain('onmouseover="');
    expect(inline('see https://example.com/a.')).toBe('see <a href="https://example.com/a" target="_blank" rel="noopener noreferrer">https://example.com/a</a>.');
  });

  it('does not treat snake_case or code as emphasis', () => {
    expect(inline('a_b_c and 2*3*4')).toBe('a_b_c and 2*3*4');
    expect(inline('`**not bold**`')).toBe('<code>**not bold**</code>');
  });

  it('pairs strong, underline-strong and strike markers with the next closing one', () => {
    expect(inline('**a** b **c** __d__ ~~e~~')).toBe('<strong>a</strong> b <strong>c</strong> <strong>d</strong> <del>e</del>');
    expect(inline('**bold *it* x**')).toBe('<strong>bold <em>it</em> x</strong>');
    expect(inline('** a and b**')).toBe('** a and b**');
  });

  it('renders a long line of unclosed markers quickly', () => {
    const line = Array.from({ length: 12000 }, (_, i) => `**a${i} __b ~~c`).join(' ');
    const t = performance.now();
    expect(inline(line)).not.toContain('<strong>');
    expect(performance.now() - t).toBeLessThan(300);
  });
});

describe('parseMarkdownCached', () => {
  // Given a message already parsed once (before a tab switch)
  // When the same text is parsed again
  // Then the cached blocks are returned without a new parse
  it('reuses blocks for the same text', () => {
    const text = '# Same\n\nbody `x.png`';
    const a = parseMarkdownCached(text);
    expect(parseMarkdownCached(text)).toBe(a);
    expect(a).toEqual(parseMarkdown(text));
  });

  // Given two different texts
  // When both are parsed
  // Then each gets its own blocks
  it('keys the cache by text', () => {
    expect(parseMarkdownCached('one')).not.toBe(parseMarkdownCached('two'));
  });

  it('links a whole code span that is a path with spaces', () => {
    const html = inline('Done: see `docs/J3 modifications.pdf` and also `docs/missing.pdf`.');
    expect(html).toContain('<a class="file-link" data-file="docs/J3 modifications.pdf"><code>docs/J3 modifications.pdf</code></a>');
    expect(html).toContain('data-file="docs/missing.pdf"');
  });

  it('links spaced code spans whose first word is not a verb', () => {
    for (const t of ['My Documents/report.pdf', '~/Notes/Q3 plan.md', './build out/app.log', 'J3 modifications.pdf']) {
      expect(inline('`' + t + '`')).toContain('data-file="' + t + '"');
    }
  });

  it('keeps spaced prose and non-path code spans plain', () => {
    expect(inline('open docs/J3 modifications.pdf now')).not.toContain('data-file="docs/J3');
    expect(inline('open J3 modifications.pdf now')).not.toContain('data-file="J3');
    expect(inline('`docs/J3 modifications`')).toBe('<code>docs/J3 modifications</code>');
    for (const t of ['see docs/a.pdf', 'run tests/x.py', 'voir docs/a.pdf']) expect(inline('`' + t + '`')).toBe('<code>' + t + '</code>');
    expect(inline('`run the tests`')).toBe('<code>run the tests</code>');
    expect(inline('`docs/J3 modifications.zip`')).not.toContain('data-file');
  });

  it('does not link code that holds a URL or a code block line', () => {
    expect(inline('`https://x.com/a b.pdf`')).not.toContain('data-file');
    const [b] = parseMarkdown('```\ndocs/J3 modifications.pdf\n```');
    expect(b).toEqual({ type: 'code', lang: '', code: 'docs/J3 modifications.pdf' });
  });
});
