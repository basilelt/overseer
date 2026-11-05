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

const escape = (v: string) => v.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

function lucideMarkup(name: string): string {
  const src = fs.readFileSync(lucide(name), 'utf8');
  const data = JSON.parse(/__iconData = (\{.*?\});\n/.exec(src)![1]) as { node: [string, Record<string, string>][] };
  return data.node.map(([tag, attrs]) => `<${tag} ${Object.entries(attrs).map(([k, v]) => `${k}="${escape(String(v))}"`).join(' ')}/>`).join('');
}

describe('icon sprite', () => {
  it('matches the installed lucide icon data', () => {
    for (const [name, body] of Object.entries(ICONS)) expect(body, name).toBe(lucideMarkup(name));
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
