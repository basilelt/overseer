import { describe, expect, it, vi } from 'vitest';
import { DaemonError, type DaemonClient } from './daemon/client';
import { isMissingFile, filePath, fileKind, findPaths, mimeOf, saveFile } from './files';

describe('findPaths', () => {
  it('finds absolute, relative and home paths with known extensions', () => {
    const line = 'Saved /tmp/out/shot.png and ./docs/spec.pdf, see ~/v/clip.MP4 or notes.md:12:3.';
    expect(findPaths(line).map((m) => m.path)).toEqual(['/tmp/out/shot.png', './docs/spec.pdf', '~/v/clip.MP4', 'notes.md']);
  });

  it('gives the range of the whole match including :line:col', () => {
    const [m] = findPaths('x src/a.ts b.json:4:2 y');
    expect(m).toEqual({ path: 'b.json', start: 11, end: 21 });
  });

  it('skips URLs, unknown extensions and words glued to other text', () => {
    expect(findPaths('https://example.com/a.png')).toEqual([]);
    expect(findPaths('main.rs lib.ts archive.zip')).toEqual([]);
    expect(findPaths('foo.pngx')).toEqual([]);
  });
});

describe('filePath', () => {
  it('accepts a lone path and rejects text around it', () => {
    expect(filePath('  /a/b/d.png ')).toBe('/a/b/d.png');
    expect(filePath('d.png and more')).toBeNull();
  });

  it('accepts spaces inside a path with a known file kind', () => {
    expect(filePath(' /a/b c/d.png ')).toBe('/a/b c/d.png');
    expect(filePath('docs/J3 modifications.pdf')).toBe('docs/J3 modifications.pdf');
    expect(filePath('docs/J3 modifications.pdf:12')).toBe('docs/J3 modifications.pdf');
    expect(filePath('~/My Docs/report (1).pdf')).toBe('~/My Docs/report (1).pdf');
    expect(filePath('J3 modifications.pdf')).toBe('J3 modifications.pdf');
    expect(filePath('My Documents/report.pdf')).toBe('My Documents/report.pdf');
    expect(filePath('~/Notes/Q3 plan.md')).toBe('~/Notes/Q3 plan.md');
    expect(filePath('./build out/app.log')).toBe('./build out/app.log');
  });

  it('rejects a span that is a verb followed by a path', () => {
    for (const t of ['see docs/a.pdf', 'run tests/x.py', 'Open docs/a.pdf', 'check a.pdf', 'edit src/a.ts:3', 'in docs/a.pdf', 'voir docs/a.pdf', 'ouvrir docs/a.pdf']) {
      expect(filePath(t)).toBeNull();
    }
  });

  it('rejects spaced text that is not a path with a known kind', () => {
    expect(filePath('docs/J3 modifications')).toBeNull();
    expect(filePath('docs/J3 modifications.zip')).toBeNull();
    expect(filePath('run the tests, then a.pdf')).toBeNull();
    expect(filePath('a.pdf  b.pdf')).toBeNull();
    expect(filePath('docs/ a.pdf')).toBeNull();
    expect(filePath('see https://x.com/a b.pdf')).toBeNull();
    expect(filePath(`${'a '.repeat(300)}.pdf`)).toBeNull();
  });
});

describe('fileKind and mimeOf', () => {
  it('maps extensions', () => {
    expect(fileKind('/x/a.JPG')).toBe('image');
    expect(fileKind('a.webm')).toBe('video');
    expect(fileKind('a.pdf')).toBe('pdf');
    expect(fileKind('README.md')).toBe('markdown');
    expect(fileKind('a.log')).toBe('text');
    expect(fileKind('.png')).toBeNull();
    expect(fileKind('a.rs')).toBeNull();
    expect(mimeOf('a.svg')).toBe('image/svg+xml');
    expect(mimeOf('a.yaml')).toBe('text/plain');
  });
});

describe('findPaths on long tokens', () => {
  const scanWork = (text: string) => {
    const spy = vi.spyOn(String.prototype, 'matchAll');
    const out = findPaths(text);
    const scans = spy.mock.calls.length;
    spy.mockRestore();
    return { out, scans };
  };

  it('runs no path scan on SHAs and UUIDs', () => {
    const { out, scans } = scanWork('commit 3f9a1c0d2b7e4f6a8c1d3e5f7a9b0c2d4e6f8a1b in session 0b7f2c4e-91d3-4a6b-8e2f-5c7d9a1b3e4f_resume_token');
    expect(out).toEqual([]);
    expect(scans).toBe(1);
  });

  it('does the same work for a huge token as for a large one', () => {
    const blob = (n: number) => `${'aB3+/x'.repeat(n)}.png see ./docs/spec.pdf`;
    const small = scanWork(blob(2_000));
    const huge = scanWork(blob(20_000));
    expect(huge.out.map((m) => m.path)).toEqual(['./docs/spec.pdf']);
    expect(huge.scans).toBe(small.scans);
  });

  it('keeps matching paths around punctuation', () => {
    expect(findPaths('(see ../a/b.md) "x/y.json", a//b.png, ~foo.png').map((m) => m.path)).toEqual(['../a/b.md', 'x/y.json', '~foo.png']);
  });
});

describe('saveFile', () => {
  function client(fetchBytes: DaemonClient['fetchBytes']) {
    return { fetchBytes: vi.fn(fetchBytes), saveDownload: vi.fn(async (name: string) => `/Users/me/Downloads/${name}`) } as unknown as DaemonClient & {
      fetchBytes: ReturnType<typeof vi.fn>;
      saveDownload: ReturnType<typeof vi.fn>;
    };
  }

  it('fetches the bytes from the daemon and saves them under the file name', async () => {
    const c = client(async () => new Uint8Array([7, 8, 9]).buffer);
    const path = await saveFile(c, { path: 'shots/a b.png', term: 't1' }, 'a b.png');
    expect(path).toBe('/Users/me/Downloads/a b.png');
    expect(c.fetchBytes).toHaveBeenCalledWith('/v1/file?path=shots%2Fa+b.png&term=t1');
    expect(c.saveDownload).toHaveBeenCalledWith('a b.png', new Uint8Array([7, 8, 9]));
  });

  it('does not save when the daemon cannot read the file', async () => {
    const c = client(async () => {
      throw new DaemonError(404, 'not found');
    });
    await expect(saveFile(c, { path: '/gone.png' }, 'gone.png')).rejects.toThrow('not found');
    expect(c.saveDownload).not.toHaveBeenCalled();
  });
});

describe('findPaths on paths with spaces in prose', () => {
  it('does not link the tail of a spaced path', () => {
    expect(findPaths('Also docs/J3 modifications.pdf in prose.')).toEqual([]);
  });

  it('does not link the tail after a home directory fragment', () => {
    expect(findPaths('see ~/My Docs/J3 modifications.pdf')).toEqual([]);
  });

  it('still links consecutive complete paths', () => {
    expect(findPaths('src/a.json b.json').map((m) => m.path)).toEqual(['src/a.json', 'b.json']);
  });

  it('still links a spaceless path after plain words', () => {
    expect(findPaths('open docs/spec.pdf now').map((m) => m.path)).toEqual(['docs/spec.pdf']);
  });
});

describe('isMissingFile', () => {
  it('recognises the daemon error for a path that does not exist', () => {
    expect(isMissingFile('/tmp/x.md: No such file or directory (os error 2)')).toBe(true);
    expect(isMissingFile('x: os error 2')).toBe(true);
  });

  it('leaves other errors alone', () => {
    expect(isMissingFile('Permission denied (os error 13)')).toBe(false);
    expect(isMissingFile('os error 22')).toBe(false);
  });
});
