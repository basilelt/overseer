import { beforeAll, describe, expect, it } from 'vitest';
import { ICONS } from './paths';
import { ensureSprite, iconHref, SPRITE_ID, spriteMarkup } from './sprite';

interface Fs {
  readFileSync(path: URL, enc: 'utf8'): string;
  readdirSync(path: URL, opts: { recursive: true }): string[];
}
let fs: Fs;
beforeAll(async () => {
  const name: string = 'node:fs';
  fs = await import(/* @vite-ignore */ name);
});
const lucide = (name: string) => new URL(`../../../node_modules/@lucide/svelte/dist/icons/${name}.svelte`, import.meta.url);
const components = new URL('../components/', import.meta.url);

type Node = [string, Record<string, string>];

function lucideNodes(name: string): Node[] {
  const src = fs.readFileSync(lucide(name), 'utf8');
  return (JSON.parse(/__iconData = (\{.*?\});\n/.exec(src)![1]) as { node: Node[] }).node.map(([tag, attrs]) => [tag, Object.fromEntries(Object.entries(attrs).map(([k, v]) => [k, String(v)]))]);
}

function spriteNodes(body: string): Node[] {
  const unescape = (v: string) => v.replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
  return [...body.matchAll(/<([a-z]+)((?:\s+[\w-]+="[^"]*")*)\s*\/>/g)].map((m) => [m[1], Object.fromEntries([...m[2].matchAll(/([\w-]+)="([^"]*)"/g)].map((a) => [a[1], unescape(a[2])]))]);
}

describe('icon sprite', () => {
  it('matches the installed lucide icon data', () => {
    for (const [name, body] of Object.entries(ICONS)) expect(spriteNodes(body), name).toEqual(lucideNodes(name));
  });

  it('reads real shapes from a sprite body, so the lucide comparison is not vacuous', () => {
    expect(spriteNodes(ICONS.play)).toEqual([['path', { d: 'M5 5a2 2 0 0 1 3.008-1.728l11.997 6.998a2 2 0 0 1 .003 3.458l-12 7A2 2 0 0 1 5 19z' }]]);
  });

  it('declares one symbol per icon', () => {
    const markup = spriteMarkup();
    for (const name of Object.keys(ICONS)) expect(markup).toContain(`<symbol id="${iconHref(name as keyof typeof ICONS).slice(1)}"`);
  });

  it('only references icons the sprite defines, from components that insert it', () => {
    const files = fs.readdirSync(components, { recursive: true }).filter((f) => f.endsWith('.svelte'));
    let total = 0;
    for (const f of files) {
      const src = fs.readFileSync(new URL(f, components), 'utf8');
      const used = [...src.matchAll(/href="#sbi-([a-z-]+)"/g)].map((m) => m[1]);
      if (!used.length) continue;
      total += used.length;
      expect(src, f).toContain('ensureSprite()');
      for (const name of used) expect(Object.keys(ICONS), f).toContain(name);
    }
    expect(total).toBeGreaterThan(10);
  });

  it('inserts the sprite once', () => {
    const added: { id: string; innerHTML: string }[] = [];
    const doc = {
      body: { prepend: (el: { id: string; innerHTML: string }) => added.push(el) },
      getElementById: (id: string) => added.find((x) => x.id === id) ?? null,
      createElementNS: () => ({ id: '', innerHTML: '', setAttribute() {} }),
    } as unknown as Document;
    expect(ensureSprite(doc)).toBe(true);
    expect(ensureSprite(doc)).toBe(true);
    expect(added).toHaveLength(1);
    expect(added[0].id).toBe(SPRITE_ID);
    expect(ensureSprite(undefined)).toBe(false);
  });

  it('waits for the body when loaded from the head', () => {
    const added: { id: string }[] = [];
    let ready: (() => void) | null = null;
    const doc = {
      body: null as unknown,
      addEventListener: (_: string, fn: () => void) => (ready = fn),
      getElementById: (id: string) => added.find((x) => x.id === id) ?? null,
      createElementNS: () => ({ id: '', innerHTML: '', setAttribute() {} }),
    };
    expect(ensureSprite(doc as unknown as Document)).toBe(false);
    doc.body = { prepend: (el: { id: string }) => added.push(el) };
    ready!();
    expect(added.map((x) => x.id)).toEqual([SPRITE_ID]);
  });
});
