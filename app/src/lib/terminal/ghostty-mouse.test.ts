import { beforeAll, describe, expect, it, vi } from 'vitest';
import source from 'ghostty-web?raw';
import { patchGhostty } from '../../../vite.config';
import { eventTarget } from '../../test/fake-events';

type Mod = typeof import('ghostty-web');
type Wasm = ReturnType<InstanceType<Mod['Ghostty']>['createTerminal']>;
type Terminal = InstanceType<Mod['Terminal']>;

// The PATCHES in vite.config.ts change these members at runtime; the shipped .d.ts still
// declares the old signatures, so the test uses the patched shape through this type.
type PatchedSelectionManager = Omit<InstanceType<Mod['SelectionManager']>, 'clearSelection'> & {
  clearSelection: (data?: string) => void;
  copyToClipboard: (text: string) => void;
};

let mod: Mod;
let wasm: WebAssembly.Module;

beforeAll(async () => {
  const fsModule = 'node:fs/promises';
  const { readFile } = (await import(/* @vite-ignore */ fsModule)) as { readFile(path: URL): Promise<Uint8Array<ArrayBuffer>> };
  wasm = await WebAssembly.compile(await readFile(new URL('../../../node_modules/ghostty-web/ghostty-vt.wasm', import.meta.url)));
  const url = `data:text/javascript,${encodeURIComponent(patchGhostty(source))}`;
  mod = (await import(/* @vite-ignore */ url)) as Mod;
});

const COLS = 40;
const ROWS = 5;
const CELL_W = 10;
const CELL_H = 20;

// A minimal DOM: elements are event targets that bubble to their parent, then to document.
interface FakeEl {
  tagName: string;
  style: Record<string, string>;
  attrs: Record<string, string>;
  children: FakeEl[];
  parent: FakeEl | null;
  target: ReturnType<typeof eventTarget>;
  clientWidth: number;
  clientHeight: number;
  value: string;
  appendChild(c: FakeEl): void;
  remove(): void;
  setAttribute(k: string, v: string): void;
  getAttribute(k: string): string | null;
  hasAttribute(k: string): boolean;
  removeAttribute(k: string): void;
  addEventListener(t: string, fn: (e: unknown) => void): void;
  removeEventListener(t: string, fn: (e: unknown) => void): void;
  contains(e: FakeEl | null): boolean;
  focus(): void;
  getBoundingClientRect(): { left: number; top: number; right: number; bottom: number; width: number; height: number };
  dispatch(type: string, e: Record<string, unknown>): void;
  select(): void;
  setSelectionRange(a: number, b: number): void;
}

function makeEl(tagName: string, parent: FakeEl | null, doc: FakeEl): FakeEl {
  const target = eventTarget();
  const el: FakeEl = {
    tagName,
    style: {},
    attrs: {},
    children: [],
    parent,
    target,
    clientWidth: 0,
    clientHeight: 0,
    value: '',
    appendChild(c) {
      el.children.push(c);
      c.parent = el;
    },
    remove() {
      if (el.parent) el.parent.children = el.parent.children.filter((c) => c !== el);
    },
    setAttribute(k, v) {
      el.attrs[k] = v;
    },
    getAttribute(k) {
      return el.attrs[k] ?? null;
    },
    hasAttribute(k) {
      return k in el.attrs;
    },
    removeAttribute(k) {
      delete el.attrs[k];
    },
    addEventListener(t, fn) {
      target.addEventListener(t, fn as (e: unknown) => void);
    },
    removeEventListener(t, fn) {
      target.removeEventListener(t, fn as (e: unknown) => void);
    },
    contains(e) {
      for (let n: FakeEl | null = e; n; n = n.parent) if (n === el) return true;
      return false;
    },
    focus() {},
    getBoundingClientRect() {
      return { left: 0, top: 0, right: el.clientWidth, bottom: el.clientHeight, width: el.clientWidth, height: el.clientHeight };
    },
    dispatch(type, e) {
      const ev = { type, target: el, currentTarget: el, preventDefault() {}, stopPropagation() {}, stopImmediatePropagation() {}, ...e };
      for (let n: FakeEl | null = el; n; n = n.parent) n.target.fire(type, ev);
      doc.target.fire(type, ev);
    },
    select() {},
    setSelectionRange() {},
  };
  return el;
}

interface Rig {
  wasmTerm: Wasm;
  canvas: FakeEl;
  container: FakeEl;
  sm: PatchedSelectionManager;
  sent: string[];
  copied: string[];
  cleanup(): void;
}

// Wires a real Terminal's selection manager and input handler the way Terminal.open() does
// (against a fake renderer and fake DOM), so the patched ghostty-web code runs for real.
async function rig(): Promise<Rig> {
  const ghostty = new mod.Ghostty(await WebAssembly.instantiate(wasm, { env: { log: () => {} } }) as never);
  const wasmTerm = ghostty.createTerminal(COLS, ROWS, { scrollbackLimit: 1000 });
  const term = new mod.Terminal({ ghostty, cols: COLS, rows: ROWS, scrollback: 1000 });
  const doc = makeEl('#document', null, null as never);
  const body = makeEl('body', doc, doc);
  const container = makeEl('div', body, doc);
  const canvas = makeEl('canvas', container, doc);
  const textarea = makeEl('textarea', container, doc);
  canvas.clientWidth = COLS * CELL_W;
  canvas.clientHeight = ROWS * CELL_H;
  const renderer = {
    getCanvas: () => canvas,
    getMetrics: () => ({ width: CELL_W, height: CELL_H, baseline: CELL_H - 4 }),
    setSelectionManager: () => {},
  };
  Object.assign(term, { isOpen: true, element: container, canvas, textarea, renderer, wasmTerm });
  vi.stubGlobal('document', doc);
  const sent: string[] = [];
  const copied: string[] = [];
  const sm = new mod.SelectionManager(term as unknown as Terminal, renderer as never, wasmTerm, textarea as never) as unknown as PatchedSelectionManager;
  sm.copyToClipboard = (text: string) => void copied.push(text); // what routeCopies() in the app does
  (container as unknown as Record<string, unknown>).__overseerSel = sm; // what the open() patch adds
  const onData = (data: string) => {
    sm.clearSelection(data); // what the patched input callback does
    sent.push(data);
  };
  const ih = new mod.InputHandler(
    ghostty,
    container as never,
    onData,
    () => {},
    undefined,
    undefined,
    (mode: number) => wasmTerm.getMode(mode, false),
    undefined,
    textarea as never,
    {
      hasMouseTracking: () => wasmTerm.hasMouseTracking(),
      hasSgrMouseMode: () => wasmTerm.getMode(1006, false),
      getCellDimensions: () => ({ width: CELL_W, height: CELL_H }),
      getCanvasOffset: () => ({ left: 0, top: 0 }),
    },
  );
  return {
    wasmTerm,
    canvas,
    container,
    sm,
    sent,
    copied,
    cleanup() {
      ih.dispose();
      sm.dispose();
      vi.stubGlobal('document', undefined);
    },
  };
}

function mouse(el: FakeEl, type: string, x: number, y: number, extra: Record<string, unknown> = {}) {
  el.dispatch(type, { button: 0, clientX: x, clientY: y, offsetX: x, offsetY: y, detail: 1, shiftKey: false, ctrlKey: false, altKey: false, metaKey: false, ...extra });
}

// Drag from (x0,y) to (x1,y) on the canvas: press, move, release. Row 2 keeps clear of the
// auto-scroll edge zones (30 px) of the fake canvas.
function drag(canvas: FakeEl, x0: number, x1: number) {
  const y = 2 * CELL_H + 5;
  mouse(canvas, 'mousedown', x0, y);
  mouse(canvas, 'mousemove', x1, y);
  mouse(canvas, 'mouseup', x1, y);
}

describe('mouse tracking versus local selection', () => {
  it('keeps a mouse report from clearing the running selection, but still clears on typed input', async () => {
    const r = await rig();
    try {
      r.wasmTerm.write('hello world\r\n');
      r.wasmTerm.update();
      r.sm.select(0, 0, 5); // "hello"
      expect(r.sm.hasSelection()).toBe(true);
      r.sm.clearSelection('\x1b[<0;1;1M'); // SGR press report
      expect(r.sm.hasSelection()).toBe(true);
      r.sm.clearSelection('\x1b[M !!'); // X10 press report
      expect(r.sm.hasSelection()).toBe(true);
      r.sm.clearSelection('x'); // typed character
      expect(r.sm.hasSelection()).toBe(false);
    } finally {
      r.cleanup();
    }
  });

  it('selects a drag locally in a program with mouse tracking on, sending only press and release', async () => {
    const r = await rig();
    try {
      r.wasmTerm.write('\x1b[?1049h\x1b[?1000h\x1b[?1002h\x1b[?1006h'); // alt screen + the CLI asks for mouse reports
      r.wasmTerm.write('first\r\nsecond\r\nlog in at https://example.com/x now\r\n');
      r.wasmTerm.update();
      expect(r.wasmTerm.hasMouseTracking()).toBe(true);
      drag(r.canvas, 0, 350); // across row 2, the whole URL line
      expect(r.sm.hasSelection()).toBe(true);
      expect(r.sm.getSelection()).toBe('log in at https://example.com/x now');
      const reports = r.sent.join('');
      expect(reports).toContain('\x1b[<0;1;3M'); // press at the drag start
      expect(reports).toContain('\x1b[<0;36;3m'); // release at the drag end
      expect(reports).not.toMatch(/\x1b\[<32;/); // no drag motion was sent to the program
      expect(r.copied).toEqual(['log in at https://example.com/x now']); // released drag copies
    } finally {
      r.cleanup();
    }
  });

  it('still sends plain clicks to a program with mouse tracking on', async () => {
    const r = await rig();
    try {
      r.wasmTerm.write('\x1b[?1000h\x1b[?1006h');
      r.wasmTerm.write('pick one\r\n');
      r.wasmTerm.update();
      const y = 2 * CELL_H + 5;
      mouse(r.canvas, 'mousedown', 30, y);
      mouse(r.canvas, 'mouseup', 30, y);
      expect(r.sm.hasSelection()).toBe(false);
      const reports = r.sent.join('');
      expect(reports).toContain('\x1b[<0;4;3M');
      expect(reports).toContain('\x1b[<0;4;3m');
    } finally {
      r.cleanup();
    }
  });

  it('selects a drag locally and sends nothing when the program does not read the mouse', async () => {
    const r = await rig();
    try {
      r.wasmTerm.write('a\r\nb\r\nthird line here\r\n');
      r.wasmTerm.update();
      expect(r.wasmTerm.hasMouseTracking()).toBe(false);
      drag(r.canvas, 0, 150);
      expect(r.sm.hasSelection()).toBe(true);
      expect(r.sm.getSelection()).toBe('third line here');
      expect(r.sent).toEqual([]);
      expect(r.copied).toEqual(['third line here']);
    } finally {
      r.cleanup();
    }
  });
});
