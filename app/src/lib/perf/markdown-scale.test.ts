import { describe, expect, it } from 'vitest';
import { parseMarkdown } from '../chat/markdown';
import { miniDiff } from '../chat/tools';
import { findPaths } from '../files';

const LINEAR = 30;

function reps(fn: () => void): number {
  let n = 1;
  for (;;) {
    const t = performance.now();
    for (let i = 0; i < n; i++) fn();
    if (performance.now() - t >= 2 || n >= 4096) return n;
    n *= 2;
  }
}

function per(fn: () => void, n: number): number {
  const t = performance.now();
  for (let i = 0; i < n; i++) fn();
  return (performance.now() - t) / n;
}

function scaling(small: () => void, big: () => void, runs = 7): number {
  let best = Infinity;
  for (let attempt = 0; attempt < 3 && best >= LINEAR; attempt++) {
    const ns = reps(small);
    const nb = reps(big);
    let a = Infinity;
    let b = Infinity;
    for (let i = 0; i < runs; i++) {
      a = Math.min(a, per(small, ns));
      b = Math.min(b, per(big, nb));
    }
    best = Math.min(best, b / a);
  }
  return best;
}

function section(i: number): string {
  return [
    `## Step ${i}`,
    '',
    `I looked at \`src/lib/mod${i}.ts\` and **the reducer** in src/app/file${i}.json, then ran the _tests_ again, also \`docs/J3 modifications ${i}.pdf\` and \`not a path ${i}\`.`,
    `See [the docs](https://example.com/d/${i}) and https://example.com/x/${i} for ~~old~~ new notes.`,
    '',
    `- first item with \`code\` and **bold**`,
    `- second item mentions notes/run${i}.log:12`,
    `1. ordered one`,
    `2. ordered two`,
    '',
    '> a quoted remark',
    '',
    '| file | lines |',
    '| --- | --- |',
    `| a${i}.md | ${i} |`,
    `| b${i}.txt | ${i * 2} |`,
    '',
    '```ts',
    `const x${i} = await load(${i});`,
    'return x;',
    '```',
    '',
    '---',
    '',
  ].join('\n');
}

const transcript = (n: number) => Array.from({ length: n }, (_, i) => section(i)).join('\n');

describe('chat markdown rendering cost', () => {
  it('parses a long transcript in time linear to its length', () => {
    const small = transcript(20);
    const big = transcript(200);
    expect(parseMarkdown(big).length).toBe(parseMarkdown(small).length * 10);
    expect(scaling(() => parseMarkdown(small), () => parseMarkdown(big))).toBeLessThan(LINEAR);
  });

  it('a long single paragraph stays linear', () => {
    const para = (n: number) => Array.from({ length: n }, (_, i) => `word${i % 9} **b${i % 7}** in \`x${i % 5}\` see a/b${i % 3}.md`).join(' ');
    const small = para(200);
    const big = para(2000);
    expect(scaling(() => parseMarkdown(small), () => parseMarkdown(big))).toBeLessThan(LINEAR);
  });

  it('unclosed emphasis markers do not rescan the rest of the line', () => {
    const line = (n: number) => Array.from({ length: n }, (_, i) => `**a${i % 9} __b ~~c *d _e`).join(' ');
    const small = line(100);
    const big = line(1000);
    expect(scaling(() => parseMarkdown(small), () => parseMarkdown(big))).toBeLessThan(LINEAR);
  });

  it('long identifiers do not blow up path detection', () => {
    const word = (n: number) => `fixed in ${'4b825dc642cb6eb9a060e54bf8d69288fbee4904'.slice(0, n)} today`;
    expect(scaling(() => findPaths(word(12)), () => findPaths(word(24)))).toBeLessThan(LINEAR);
  });

  it('spaced path tails scale linearly with the number of paths', () => {
    const line = (n: number) => Array.from({ length: n }, (_, i) => `docs/d${i % 9} file${i % 7}.pdf`).join(' ');
    const small = line(200);
    const big = line(2000);
    expect(findPaths(small)).toEqual([]);
    expect(scaling(() => findPaths(small), () => findPaths(big))).toBeLessThan(LINEAR);
  });

  it('caps the tool diff LCS so huge edits stay linear', () => {
    const text = (n: number) => Array.from({ length: n }, (_, i) => `line ${i} ${i % 3 ? 'same' : 'diff'}`).join('\n');
    const edit = (n: number) => ({ old: text(n), new: text(n).replace(/diff/g, 'changed') });
    const small = edit(1000);
    const big = edit(10000);
    expect(miniDiff(big)!.lines.length).toBe(40);
    expect(scaling(() => miniDiff(small), () => miniDiff(big))).toBeLessThan(LINEAR);
  });
});
