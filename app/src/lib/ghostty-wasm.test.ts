import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import * as mod from 'ghostty-web';
import { Ghostty, Terminal } from 'ghostty-web';
import { installBlankResize, installWholeLineViewport } from './termrender';

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
  it('keeps every hyperlinked line once and in order across resizes that replay a growing stream', async () => {
    expect(installBlankResize(mod)).toBe(true);
    const term = headless(await load(), 120, 40);
    const enc = new TextEncoder();
    const row = (i: number) => enc.encode(`\x1b]8;;https://example.com\x07/rc ${i}\x1b]8;;\x07\r\n`);
    const parts: Uint8Array[] = [];
    const feed = (bytes: Uint8Array, step: number) => {
      for (let k = 0; k < bytes.length; k += step) term.write(bytes.slice(k, k + step));
    };
    const joined = () => Uint8Array.from(parts.flatMap((p) => Array.from(p)));
    const sizes: [number, number][] = [[120, 40], [90, 40], [150, 36], [100, 44]];
    let n = 0;
    for (let s = 0; s < 30; s++) {
      for (let k = 0; k < 17 && n < 500; k++) {
        parts.push(row(++n));
        feed(parts[parts.length - 1], 7);
      }
      const [c, r] = sizes[s % sizes.length];
      term.resize(c, r);
      feed(joined(), 331);
    }
    while (n < 500) {
      parts.push(row(++n));
      feed(parts[parts.length - 1], 7);
    }
    const t = term.wasmTerm!;
    t.update();
    const rows: string[] = [];
    const cells = (cs: { codepoint: number }[] | null) => (cs ?? []).map((c) => (c.codepoint ? String.fromCodePoint(c.codepoint) : ' ')).join('').trimEnd();
    for (let y = 0; y < t.getScrollbackLength(); y++) rows.push(cells(t.getScrollbackLine(y)));
    for (let y = 0; y < t.getDimensions().rows; y++) rows.push(text(t, y));
    expect(rows.filter((l) => l.startsWith('/rc'))).toEqual(Array.from({ length: 500 }, (_, i) => `/rc ${i + 1}`));
  });

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

  it('starts blank after a reset that reuses the memory of a freed terminal', async () => {
    installBlankResize(mod);
    const term = headless(await load(), 80, 24);
    for (let i = 0; i < 3000; i++) term.write(`\x1b[1;31mold line ${i} ${'x'.repeat(i % 60)}\x1b[0m\r\n`);
    term.reset();
    term.write('new\r\n');
    expect(screen(term.wasmTerm!)).toEqual(['new', ...Array(23).fill('')]);
  });

  it('points the selection at the new terminal after a reset', async () => {
    installBlankResize(mod);
    const term = headless(await load(), 80, 24);
    const selection = { wasmTerm: term.wasmTerm };
    Object.assign(term, { selectionManager: selection });
    term.reset();
    term.write('copied text\r\n');
    expect(selection.wasmTerm).toBe(term.wasmTerm);
    expect(text(selection.wasmTerm!, 0)).toBe('copied text');
  });

  it('patches the prototype once', () => {
    expect(installBlankResize(mod)).toBe(true);
    expect(installBlankResize(mod)).toBe(true);
    expect(installBlankResize({ Terminal: class {} })).toBe(false);
  });
});

describe('fractional viewport', () => {
  async function painted(viewportY: number): Promise<string[]> {
    const t = (await load()).createTerminal(40, 8, { scrollbackLimit: 1_000_000 });
    for (let i = 1; i <= 30; i++) t.write(`line${i}\r\n`);
    t.update();
    const rows: string[] = Array(8).fill('stale');
    const r = Object.create(mod.CanvasRenderer.prototype) as { render(...a: unknown[]): void };
    Object.assign(r, {
      canvas: { width: 320, height: 128 },
      metrics: { width: 8, height: 16, baseline: 12 },
      devicePixelRatio: 1,
      lastViewportY: 0,
      lastCursorPosition: { x: 0, y: 0 },
      cursorBlink: false,
      cursorVisible: true,
      renderLine: (cells: { codepoint: number }[], y: number) => {
        rows[y] = cells.map((c) => (c.codepoint ? String.fromCodePoint(c.codepoint) : ' ')).join('').trimEnd();
      },
      renderCursor: () => {},
      renderScrollbar: () => {},
      resize: () => {},
    });
    const scrollback = { getScrollbackLength: () => t.getScrollbackLength(), getScrollbackLine: (n: number) => t.getScrollbackLine(n) };
    r.render(t, false, viewportY, scrollback, 1);
    return rows;
  }

  it('paints every row of a viewport scrolled by a fractional number of lines', async () => {
    expect(installWholeLineViewport(mod)).toBe(true);
    expect(await painted(3.6)).toEqual(await painted(3));
  });

  it('patches the renderer prototype once', () => {
    expect(installWholeLineViewport(mod)).toBe(true);
    const render = mod.CanvasRenderer.prototype.render;
    expect(installWholeLineViewport(mod)).toBe(true);
    expect(mod.CanvasRenderer.prototype.render).toBe(render);
    expect(installWholeLineViewport({ CanvasRenderer: class {} })).toBe(false);
  });
});
