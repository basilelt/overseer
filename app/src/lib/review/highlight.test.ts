import { afterEach, describe, expect, it, vi } from 'vitest';
import { escapeHtml, highlightCode, highlightHunks, highlightLines, langFor, langOf, splitHtml } from './highlight';

const { render } = vi.hoisted(() => ({ render: vi.fn((code: string, lang: string) => code.split('\n').map((l) => `<span class="${lang}">${l}</span>`).join('\n')) }));
vi.mock('./hljs', () => ({ render }));

const tag = (lang: string, s: string) => `<span class="${lang}">${s}</span>`;

afterEach(() => {
  render.mockClear();
});

describe('langFor', () => {
  it('maps extensions case-insensitively and recognises extensionless build files', () => {
    expect(langFor('a/b/C.TSX')).toBe('typescript');
    expect(langFor('x.yml')).toBe('yaml');
    expect(langFor('conf/app.toml')).toBe('ini');
    expect(langFor('docker/Dockerfile.dev')).toBe('dockerfile');
    expect(langFor('GNUmakefile')).toBe('makefile');
    expect(langFor('Makefile')).toBe('makefile');
    expect(langFor('rules.mk')).toBe('makefile');
  });

  it('returns null for unknown or missing extensions and dotfiles without a known suffix', () => {
    expect(langFor('README')).toBeNull();
    expect(langFor('a.unknown')).toBeNull();
    expect(langFor('dir.ts/')).toBeNull();
    expect(langFor('')).toBeNull();
  });
});

describe('langOf', () => {
  it('accepts language names, aliases and extensions from fenced code tags', () => {
    expect(langOf('Rust')).toBe('rust');
    expect(langOf('shell')).toBe('bash');
    expect(langOf('console')).toBe('bash');
    expect(langOf('golang')).toBe('go');
    expect(langOf('py3')).toBe('python');
    expect(langOf('HTML')).toBe('xml');
    expect(langOf('toml')).toBe('ini');
    expect(langOf('tsx')).toBe('typescript');
    expect(langOf('makefile')).toBe('makefile');
  });

  it('returns null for unknown tags', () => {
    expect(langOf('brainfuck')).toBeNull();
    expect(langOf('')).toBeNull();
  });
});

describe('escapeHtml and splitHtml', () => {
  it('leaves plain text alone', () => {
    expect(escapeHtml('a b')).toBe('a b');
    expect(splitHtml('a\n\nb')).toEqual(['a', '', 'b']);
    expect(splitHtml('')).toEqual(['']);
  });

  it('reopens nested spans on each line and closes them at line end', () => {
    const html = '<span class="a"><span class="b c">x\ny</span>z\nw</span>';
    expect(splitHtml(html)).toEqual(['<span class="a"><span class="b c">x</span></span>', '<span class="a"><span class="b c">y</span>z</span>', '<span class="a">w</span>']);
  });
});

describe('highlightLines', () => {
  it('renders all lines in one pass and splits them back per line', async () => {
    expect(await highlightLines(['a', 'b'], 'x.rs')).toEqual([tag('rust', 'a'), tag('rust', 'b')]);
    expect(render).toHaveBeenCalledTimes(1);
    expect(render).toHaveBeenCalledWith('a\nb', 'rust');
  });

  it('skips unknown languages and empty input without loading the highlighter', async () => {
    expect(await highlightLines(['a'], 'LICENSE')).toBeNull();
    expect(await highlightLines([], 'x.rs')).toBeNull();
    expect(render).not.toHaveBeenCalled();
  });

  it('gives up when the rendered line count does not match', async () => {
    render.mockReturnValueOnce('one line only');
    expect(await highlightLines(['a', 'b'], 'x.rs')).toBeNull();
  });

  it('returns null instead of throwing when the highlighter fails', async () => {
    render.mockImplementationOnce(() => {
      throw new Error('boom');
    });
    expect(await highlightLines(['a'], 'x.rs')).toBeNull();
  });
});

describe('highlightHunks', () => {
  const hunks = [
    {
      lines: [
        { kind: 'ctx', old: 1, new: 1, text: 'c1' },
        { kind: 'del', old: 2, text: 'gone' },
        { kind: 'add', new: 2, text: 'added' },
        { kind: 'ctx', old: 3, new: 3, text: 'c3' },
      ],
    },
    { lines: [{ kind: 'add', new: 10, text: 'late' }] },
  ];

  it('keys each side by its own line numbers, without the other side lines', async () => {
    const h = await highlightHunks('a.ts', hunks);
    expect([...h!.new]).toEqual([
      [1, tag('typescript', 'c1')],
      [2, tag('typescript', 'added')],
      [3, tag('typescript', 'c3')],
      [10, tag('typescript', 'late')],
    ]);
    expect([...h!.old]).toEqual([
      [1, tag('typescript', 'c1')],
      [2, tag('typescript', 'gone')],
      [3, tag('typescript', 'c3')],
    ]);
  });

  it('fills extra context lines on the new side without overwriting hunk lines', async () => {
    const h = await highlightHunks('a.ts', hunks, { start: 2, lines: ['X', 'Y', 'Z'] });
    expect(h!.new.get(2)).toBe(tag('typescript', 'added'));
    expect(h!.new.get(3)).toBe(tag('typescript', 'c3'));
    expect(h!.new.get(4)).toBe(tag('typescript', 'Z'));
    expect(h!.old.has(4)).toBe(false);
  });

  it('returns null for files without a known language', async () => {
    expect(await highlightHunks('notes.txt', hunks)).toBeNull();
    expect(render).not.toHaveBeenCalled();
  });

  it('returns empty maps when a side fails to render', async () => {
    render.mockReturnValueOnce('mismatch').mockReturnValueOnce('mismatch');
    const h = await highlightHunks('a.ts', hunks);
    expect(h!.new.size).toBe(0);
    expect(h!.old.size).toBe(0);
  });
});

describe('highlightCode', () => {
  it('renders a code block with the language from its tag', async () => {
    expect(await highlightCode('let a', 'rs')).toBe(tag('rust', 'let a'));
    expect(render).toHaveBeenCalledWith('let a', 'rust');
  });

  it('returns null for empty code, unknown tags or a failing highlighter', async () => {
    expect(await highlightCode('', 'rust')).toBeNull();
    expect(await highlightCode('x', 'nope')).toBeNull();
    render.mockImplementationOnce(() => {
      throw new Error('bad');
    });
    expect(await highlightCode('x', 'rust')).toBeNull();
  });
});
