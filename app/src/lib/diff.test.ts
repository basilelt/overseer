import { describe, expect, it } from 'vitest';
import { parseDiff, tokenize } from './diff';

const SAMPLE = `diff --git a/src/a.rs b/src/a.rs
index 1..2 100644
--- a/src/a.rs
+++ b/src/a.rs
@@ -10,4 +10,5 @@ fn main() {
 let a = 1;
-let b = 2;
+let b = 3;
+let c = "x";
 done();
\\ No newline at end of file
diff --git a/new.txt b/new.txt
new file mode 100644
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hello
diff --git a/old.txt b/old.txt
deleted file mode 100644
--- a/old.txt
+++ /dev/null
@@ -1 +0,0 @@
-bye
diff --git a/img.png b/img.png
Binary files a/img.png and b/img.png differ
`;

describe('parseDiff', () => {
  const files = parseDiff(SAMPLE);

  it('splits files with paths and counts', () => {
    expect(files.map((f) => [f.path, f.added, f.removed, f.binary])).toEqual([
      ['src/a.rs', 2, 1, false],
      ['new.txt', 1, 0, false],
      ['old.txt', 0, 1, false],
      ['img.png', 0, 0, true],
    ]);
  });

  it('numbers old and new lines per hunk', () => {
    const lines = files[0].hunks[0].lines;
    expect(lines.map((l) => [l.kind, l.oldNo, l.newNo])).toEqual([
      ['ctx', 10, 10],
      ['del', 11, undefined],
      ['add', undefined, 11],
      ['add', undefined, 12],
      ['ctx', 12, 13],
      ['meta', undefined, undefined],
    ]);
    expect(files[0].hunks[0].header).toMatch(/^@@ -10,4/);
  });

  it('returns nothing for empty input', () => {
    expect(parseDiff('')).toEqual([]);
  });
});

describe('tokenize', () => {
  it('colours keywords, strings, numbers and comments', () => {
    const toks = tokenize('let s = "a // b"; // note 42');
    expect(toks.filter((t) => t.kind !== 'txt').map((t) => [t.kind, t.text])).toEqual([
      ['kw', 'let'],
      ['str', '"a // b"'],
      ['com', '// note 42'],
    ]);
    expect(toks.map((t) => t.text).join('')).toBe('let s = "a // b"; // note 42');
  });

  it('keeps plain identifiers as text', () => {
    expect(tokenize('foo(10)')).toEqual([
      { kind: 'txt', text: 'foo(' },
      { kind: 'num', text: '10' },
      { kind: 'txt', text: ')' },
    ]);
  });
});
