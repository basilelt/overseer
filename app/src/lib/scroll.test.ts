import { afterAll, beforeAll, describe, expect, it, vi } from 'vitest';
import { Ghostty, Terminal } from 'ghostty-web';
import { ModeTracker, wheelAction } from './mouse';
import { anchoredViewport, clampScrollback, DEFAULT_SCROLLBACK_LINES, resetViewport, scrollbackBytes, writeAnchored } from './scroll';

const COLS = 80;
const ROWS = 24;

const CLAUDE_START = '\x1b[?2026$p\x1b[?1016$p\x1b[?2004h\x1b[?1004h\x1b[?2031h\x1b[?25l';
const CLAUDE_REGION = `\x1b[1;${ROWS - 4}r\x1b[H`;
const CLAUDE_SPINNER = (n: number) => `\x1b[?2026h\x1b[${ROWS - 2};1H\x1b[2K✻ Thinking… (${n}s · esc to interrupt)\x1b[?2026l`;
const CLAUDE_COMMIT = (n: number) => `\x1b[?2026h\x1b[${ROWS - 4};1H\r\n⏺ committed line ${n}\x1b[?2026l`;
const CLAUDE_FULLSCREEN_ON = '\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1003h\x1b[?1006h';
const CLAUDE_FULLSCREEN_OFF = '\x1b[?1006l\x1b[?1003l\x1b[?1002l\x1b[?1000l\x1b[?1049l';

let ghostty: Ghostty;

beforeAll(async () => {
  const fsModule = 'node:fs/promises';
  const { readFile } = (await import(/* @vite-ignore */ fsModule)) as { readFile(path: URL): Promise<Uint8Array<ArrayBuffer>> };
  const wasm = await readFile(new URL('../../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url));
  const { instance } = await WebAssembly.instantiate(wasm, { env: { log: () => {} } });
  ghostty = new Ghostty(instance);
  vi.stubGlobal('window', { setTimeout: () => 0, clearTimeout: () => {} });
  vi.stubGlobal('requestAnimationFrame', () => 0);
  vi.stubGlobal('WheelEvent', { DOM_DELTA_PIXEL: 0, DOM_DELTA_LINE: 1, DOM_DELTA_PAGE: 2 });
});

afterAll(() => vi.unstubAllGlobals());

function headless(scrollback: number): Terminal {
  const term = new Terminal({ ghostty, cols: COLS, rows: ROWS, scrollback, smoothScrollDuration: 0 });
  Object.assign(term, { isOpen: true, wasmTerm: ghostty.createTerminal(COLS, ROWS, { scrollbackLimit: scrollback }) });
  return term;
}

function history(n: number): string {
  let s = '';
  for (let i = 0; i < n; i++) s += `history line ${i}\r\n`;
  return s;
}

function wheel(term: Terminal, deltaY: number) {
  const handle = (term as unknown as { handleWheel(e: unknown): void }).handleWheel;
  handle({ deltaY, deltaMode: 0, preventDefault() {}, stopPropagation() {} });
}

describe('Claude Code session in ghostty-web', () => {
  it('stays on the normal screen without mouse tracking, so the wheel scrolls local history', () => {
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    const modes = new ModeTracker();
    for (const chunk of [CLAUDE_START, history(200), CLAUDE_REGION, CLAUDE_SPINNER(1)]) {
      modes.feed(chunk);
      term.write(chunk);
    }
    expect(term.wasmTerm!.isAlternateScreen()).toBe(false);
    expect(term.wasmTerm!.hasMouseTracking()).toBe(false);
    const m = modes.modes();
    expect(m).toMatchObject({ tracking: false, alt: false });
    expect(wheelAction(-3, { deltaY: -60, deltaMode: 0, col: 1, row: 1 }, m)).toEqual({ kind: 'local', lines: -3 });
  });

  it('reports the wheel to Claude Code only while its fullscreen mode has mouse tracking on', () => {
    const modes = new ModeTracker();
    const at = { deltaY: -20, deltaMode: 0 as const, col: 4, row: 2 };
    modes.feed(CLAUDE_START + CLAUDE_FULLSCREEN_ON);
    expect(wheelAction(-1, at, modes.modes())).toEqual({ kind: 'send', data: '\x1b[<64;4;2M' });
    modes.feed(CLAUDE_FULLSCREEN_OFF);
    expect(wheelAction(-1, at, modes.modes())).toEqual({ kind: 'local', lines: -1 });
  });

  it('ghostty-web itself yanks a scrolled-back viewport to the bottom on every redraw', () => {
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    term.write(CLAUDE_START + history(200) + CLAUDE_REGION);
    wheel(term, -200);
    expect(term.getViewportY()).toBe(10);
    term.write(CLAUDE_SPINNER(2));
    expect(term.getViewportY()).toBe(0);
  });

  it('keeps the scrolled-back viewport through spinner redraws', () => {
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    term.write(CLAUDE_START + history(200) + CLAUDE_REGION);
    wheel(term, -200);
    for (let i = 0; i < 20; i++) expect(writeAnchored(term, CLAUDE_SPINNER(i))).toBe(true);
    expect(term.getViewportY()).toBe(10);
  });

  it('follows the same content when Claude Code commits lines into history', () => {
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    term.write(CLAUDE_START + history(200) + CLAUDE_REGION);
    wheel(term, -200);
    const before = term.getScrollbackLength();
    for (let i = 0; i < 5; i++) writeAnchored(term, CLAUDE_COMMIT(i));
    const grown = term.getScrollbackLength() - before;
    expect(grown).toBe(5);
    expect(term.getViewportY()).toBe(10 + grown);
  });

  it('keeps following output while the viewport is at the bottom', () => {
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    term.write(history(200));
    expect(writeAnchored(term, CLAUDE_COMMIT(1))).toBe(false);
    expect(term.getViewportY()).toBe(0);
  });

  it('keeps 10k lines of history; the old `scrollback: 5000` was a 5000-byte budget', () => {
    const lines = 3000;
    const old = headless(5000);
    old.write(history(lines));
    expect(old.getScrollbackLength()).toBeLessThan(lines / 2);
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    term.write(history(lines));
    expect(term.getScrollbackLength()).toBe(lines - ROWS + 1);
  });

  it('keeps lines that scroll out of a top-anchored region, as Claude Code renders', () => {
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    term.write(CLAUDE_REGION + `\x1b[${ROWS - 4};1H`);
    for (let i = 0; i < 100; i++) term.write(`\r\n⏺ line ${i}`);
    expect(term.getScrollbackLength()).toBe(100);
  });
});

describe('anchoredViewport', () => {
  it('stays at the bottom, shifts by new history, and clamps after a scrollback clear', () => {
    expect(anchoredViewport(0, 5, 100)).toBe(0);
    expect(anchoredViewport(10, 0, 100)).toBe(10);
    expect(anchoredViewport(10, 3, 100)).toBe(13);
    expect(anchoredViewport(10, -100, 0)).toBe(0);
    expect(anchoredViewport(90, 30, 100)).toBe(100);
  });

  it('clamps the scrollback setting', () => {
    expect(clampScrollback(Number.NaN)).toBe(DEFAULT_SCROLLBACK_LINES);
    expect(clampScrollback(10)).toBe(1000);
    expect(clampScrollback(1e9)).toBe(50_000);
    expect(scrollbackBytes(10_000)).toBe(16_000_000);
  });

  it('a terminal reset while scrolled back replays to the bottom, not to the top of the history', () => {
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    Object.assign(term, { renderer: { clear() {}, render() {} } });
    term.write(history(300));
    term.scrollToLine(10);
    expect(term.getViewportY()).toBeGreaterThan(0);
    resetViewport(term);
    expect(term.getViewportY()).toBe(0);
    expect(writeAnchored(term, history(300))).toBe(false);
    expect(term.getViewportY()).toBe(0);
  });

  it('a terminal reset while at the bottom replays to the bottom', () => {
    const term = headless(scrollbackBytes(DEFAULT_SCROLLBACK_LINES));
    Object.assign(term, { renderer: { clear() {}, render() {} } });
    term.write(history(300));
    resetViewport(term);
    expect(writeAnchored(term, history(300))).toBe(false);
    expect(term.getViewportY()).toBe(0);
  });
});
