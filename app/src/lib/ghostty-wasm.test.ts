import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import * as mod from 'ghostty-web';
import { Ghostty, Terminal } from 'ghostty-web';
import { installBlankResize } from './termrender';

let wasm: WebAssembly.Module;

beforeAll(async () => {
  const fsModule = 'node:fs/promises';
  const { readFile } = (await import(/* @vite-ignore */ fsModule)) as { readFile(path: URL): Promise<Uint8Array<ArrayBuffer>> };
  wasm = await WebAssembly.compile(await readFile(new URL('../../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url)));
  vi.stubGlobal('window', { setTimeout: () => 0, clearTimeout: () => {} });
  vi.stubGlobal('requestAnimationFrame', () => 0);
  vi.stubGlobal('cancelAnimationFrame', () => {});
});

afterAll(() => vi.unstubAllGlobals());

async function load(): Promise<Ghostty> {
  return new Ghostty(await WebAssembly.instantiate(wasm, { env: { log: () => {} } }));
}

const linked = (i: number) =>
  `\x1b[38;2;153;153;153m${'word '.repeat(i % 20)}\x1b[39m\x1b]8;id=l${i};https://example.com/s/${i}\x07\x1b[38;2;78;186;101m/rc\x1b[39m\x1b]8;;\x07\r\n`;
const line = (i: number) => `${'word '.repeat(i % 20)}/rc`;

type Wasm = ReturnType<Ghostty['createTerminal']>;

function text(t: Wasm, y: number): string {
  return (t.getLine(y) ?? []).map((c) => (c.codepoint ? String.fromCodePoint(c.codepoint) : ' ')).join('').trimEnd();
}

function screen(t: Wasm): string[] {
  t.update();
  return Array.from({ length: t.getDimensions().rows }, (_, y) => text(t, y));
}

function headless(ghostty: Ghostty, cols: number, rows: number): Terminal {
  const term = new Terminal({ ghostty, cols, rows, scrollback: 16_000_000 });
  const renderer = { resize() {}, render() {}, clear() {}, getMetrics: () => ({ width: 8, height: 16, baseline: 12 }) };
  const canvas = { width: 0, height: 0, style: {} };
  Object.assign(term, { isOpen: true, renderer, canvas, wasmTerm: ghostty.createTerminal(cols, rows, { scrollbackLimit: 16_000_000 }) });
  return term;
}

describe('ghostty-web wasm', () => {
  it('writes text after hyperlinks when the terminal was not resized', async () => {
    const t = (await load()).createTerminal(108, 54, { scrollbackLimit: 16_000_000 });
    for (let i = 0; i < 400; i++) t.write(linked(i));
    expect(screen(t)[52]).toBe(line(399));
  });

  it('writes to a new terminal at another width after freeing one with scrollback', async () => {
    const g = await load();
    for (const cols of [195, 288, 195, 288]) {
      const t = g.createTerminal(cols, 40, { scrollbackLimit: 25_952_256 });
      let s = '';
      for (let n = 0; n < 1000; n++) s += `line ${n} ${'x'.repeat(cols - 20)}\r\n`;
      expect(() => {
        for (let k = 0; k < s.length; k += 4096) t.write(s.slice(k, k + 4096));
      }).not.toThrow();
      t.free();
    }
  });
});

describe('blank resize', () => {
  it('replays hyperlinked output cleanly after a resize', async () => {
    expect(installBlankResize(mod)).toBe(true);
    const term = headless(await load(), 80, 24);
    for (let i = 0; i < 100; i++) term.write(linked(i));
    term.resize(108, 54);
    expect(screen(term.wasmTerm!).every((l) => l === '')).toBe(true);
    expect(() => {
      for (let i = 0; i < 400; i++) term.write(linked(i));
    }).not.toThrow();
    expect(screen(term.wasmTerm!)[52]).toBe(line(399));
  });

  it('keeps the screen when the size does not change', async () => {
    installBlankResize(mod);
    const term = headless(await load(), 80, 24);
    term.write(linked(3));
    term.resize(80, 24);
    expect(screen(term.wasmTerm!)[0]).toBe(line(3));
  });

  it('patches the prototype once', () => {
    expect(installBlankResize(mod)).toBe(true);
    expect(installBlankResize(mod)).toBe(true);
    expect(installBlankResize({ Terminal: class {} })).toBe(false);
  });
});
