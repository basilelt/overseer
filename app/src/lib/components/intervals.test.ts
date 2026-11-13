import { beforeAll, describe, expect, it } from 'vitest';

interface Fs {
  readFileSync(path: URL, enc: 'utf8'): string;
  readdirSync(path: URL, opts: { recursive: true }): string[];
}
let fs: Fs;
beforeAll(async () => {
  const name: string = 'node:fs';
  fs = await import(/* @vite-ignore */ name);
});

const components = new URL('./', import.meta.url);
const allowed = ['usage/UsageView.svelte'];

describe('component timers', () => {
  it('components poll through visibleClock, not a raw setInterval', () => {
    const found = fs
      .readdirSync(components, { recursive: true })
      .map((f) => f.replace(/\\/g, '/'))
      .filter((f) => f.endsWith('.svelte'))
      .filter((f) => /\bsetInterval\s*\(/.test(fs.readFileSync(new URL(f, components), 'utf8')))
      .sort();
    expect(found).toEqual(allowed);
  });
});
