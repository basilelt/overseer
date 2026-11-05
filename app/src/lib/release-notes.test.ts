import { describe, expect, it } from 'vitest';
import { parseInline, parseNotes, safeUrl, type Block, type Inline } from './release-notes';

const REPO = 'https://github.com/basilelt/overseer';

const NOTES = `## [0.6.2](${REPO}/compare/v0.6.1...v0.6.2) (2026-10-29)


### Features

* **app:** update Homebrew installs from the dialog ([1a2b3c4d5e6f](${REPO}/commit/1a2b3c4d5e6f7a8b9c0d))
* run \`brew trust\` when needed, closes [#42](${REPO}/issues/42)

### Bug Fixes

* render release notes ([abcdef0](${REPO}/commit/abcdef0123))
`;

function links(blocks: Block[]): string[] {
  const all = blocks.flatMap((b) => (b.t === 'ul' ? b.items.flat() : b.inl));
  return all.flatMap((p) => (p.t === 'link' || p.t === 'sha' ? [p.href] : []));
}

function plain(parts: Inline[]): string {
  return parts.map((p) => p.v).join('');
}

describe('release notes', () => {
  it('reads release-please notes', () => {
    const blocks = parseNotes(NOTES);
    expect(blocks.map((b) => b.t)).toEqual(['h', 'h', 'ul', 'h', 'ul']);
    expect(blocks[0]).toEqual({ t: 'h', level: 2, inl: [{ t: 'text', v: '0.6.2 (2026-10-29)' }] });
    expect(blocks[1]).toEqual({ t: 'h', level: 3, inl: [{ t: 'text', v: 'Features' }] });
    const features = blocks[2];
    if (features.t !== 'ul') throw new Error('expected a list');
    expect(features.items[0]).toEqual([
      { t: 'strong', v: 'app:' },
      { t: 'text', v: ' update Homebrew installs from the dialog' },
      { t: 'sha', v: '1a2b3c4', href: `${REPO}/commit/1a2b3c4d5e6f7a8b9c0d` },
    ]);
    expect(features.items[1]).toEqual([
      { t: 'text', v: 'run ' },
      { t: 'code', v: 'brew trust' },
      { t: 'text', v: ' when needed, closes ' },
      { t: 'link', v: '#42', href: `${REPO}/issues/42` },
    ]);
  });

  it('turns bare commit links into sha chips and drops compare links', () => {
    expect(parseInline(`fix [abcdef0](${REPO}/commit/abcdef0)`)).toEqual([
      { t: 'text', v: 'fix' },
      { t: 'sha', v: 'abcdef0', href: `${REPO}/commit/abcdef0` },
    ]);
    expect(parseInline(`[v1](${REPO}/compare/v0...v1)`)).toEqual([{ t: 'text', v: 'v1' }]);
  });

  it('keeps plain text, paragraphs and dash bullets', () => {
    expect(parseNotes('Hello\n\n- one\n- two\n\nBye')).toEqual([
      { t: 'p', inl: [{ t: 'text', v: 'Hello' }] },
      { t: 'ul', items: [[{ t: 'text', v: 'one' }], [{ t: 'text', v: 'two' }]] },
      { t: 'p', inl: [{ t: 'text', v: 'Bye' }] },
    ]);
    expect(parseNotes('')).toEqual([]);
    expect(parseNotes('\r\n* a\r\n')).toEqual([{ t: 'ul', items: [[{ t: 'text', v: 'a' }]] }]);
  });

  it('never produces markup, only text', () => {
    const html = '<img src=x onerror=alert(1)><script>alert(1)</script>';
    expect(parseNotes(html)).toEqual([{ t: 'p', inl: [{ t: 'text', v: html }] }]);
    expect(parseInline('`<script>alert(1)</script>`')).toEqual([{ t: 'code', v: '<script>alert(1)</script>' }]);
    expect(plain(parseInline('* <b>bold</b> & "quotes"'))).toBe('* <b>bold</b> & "quotes"');
  });

  it('only links http and https URLs', () => {
    for (const bad of ['javascript:alert(1)', 'data:text/html,<script>alert(1)</script>', 'vbscript:x', 'file:///etc/passwd', '/relative', 'notaurl']) {
      const parts = parseInline(`[click](${bad})`);
      expect(parts.some((p) => p.t === 'link' || p.t === 'sha')).toBe(false);
      expect(plain(parts)).toContain('click');
    }
    expect(parseInline('([abcdef0](javascript:alert(1)))').some((p) => p.t === 'sha' || p.t === 'link')).toBe(false);
    expect(safeUrl('HTTPS://example.com/a')).toBe('https://example.com/a');
    expect(safeUrl('javascript:alert(1)')).toBeNull();
  });

  it('cannot break out of the href attribute', () => {
    const blocks = parseNotes('* [x](https://a.test/"onmouseover=alert(1)) and [y](https://a.test/<script>)');
    for (const href of links(blocks)) {
      expect(href).not.toMatch(/["<>\s]/);
      expect(href.startsWith('https://a.test/')).toBe(true);
    }
    expect(links(blocks)).toHaveLength(2);
  });
});
