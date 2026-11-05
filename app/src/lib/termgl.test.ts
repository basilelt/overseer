import { afterEach, describe, expect, it, vi } from 'vitest';
import * as ghostty from 'ghostty-web';
import {
  CELL_BYTES,
  GlTerm,
  GlyphAtlas,
  NO_GLYPH,
  cursorRect,
  inLink,
  inSelection,
  installWebglRenderer,
  packRow,
  parseColor,
  scrollbarRects,
  softwareGl,
  webglMode,
  type Attach,
  type RowStyle,
} from './termgl';
import { cell, fakeGl, grid, host, scrollback } from './termgl.fake';

afterEach(() => {
  vi.restoreAllMocks();
  vi.unstubAllGlobals();
});

function style(over: Partial<RowStyle> = {}): RowStyle {
  let next = 0;
  const slots = new Map<string, number>();
  return {
    selFg: [255, 255, 255],
    selBg: [1, 2, 3],
    cellW: 8,
    cellH: 16,
    baseline: 12,
    selection: null,
    hoveredLink: 0,
    linkRange: null,
    glyph: (text, bold, italic) => {
      const k = `${bold}${italic}${text}`;
      if (!slots.has(k)) slots.set(k, next++);
      return { slot: slots.get(k)!, colored: text === '😀' };
    },
    text: (c) => String.fromCodePoint(c.codepoint),
    ...over,
  };
}

function packed(cells: ReturnType<typeof cell>[], s = style()) {
  const cols = cells.length;
  const out = new Uint8Array(cols * CELL_BYTES);
  const decos: number[] = [];
  packRow(cells, 0, cols, s, out, decos);
  const u16 = new Uint16Array(out.buffer);
  const at = (c: number) => ({
    bg: [...out.subarray(c * CELL_BYTES, c * CELL_BYTES + 4)],
    fg: [...out.subarray(c * CELL_BYTES + 4, c * CELL_BYTES + 8)],
    slot: u16[c * 6 + 4],
    colored: u16[c * 6 + 5],
  });
  return { at, decos };
}

describe('parseColor', () => {
  it('reads six and three digit hex', () => {
    expect(parseColor('#2f3b52')).toEqual([0x2f, 0x3b, 0x52]);
    expect(parseColor('#fa0')).toEqual([255, 170, 0]);
  });

  it('falls back on anything else', () => {
    expect(parseColor('rgb(1,2,3)', [9, 9, 9])).toEqual([9, 9, 9]);
    expect(parseColor(undefined)).toEqual([255, 255, 255]);
  });
});

describe('inSelection and inLink', () => {
  it('matches a single-row selection by column', () => {
    const s = { startCol: 2, startRow: 1, endCol: 4, endRow: 1 };
    expect([1, 2, 4, 5].map((c) => inSelection(s, c, 1))).toEqual([false, true, true, false]);
    expect(inSelection(s, 3, 0)).toBe(false);
    expect(inSelection(null, 0, 0)).toBe(false);
  });

  it('matches a multi-row selection like the canvas renderer', () => {
    const s = { startCol: 5, startRow: 0, endCol: 2, endRow: 2 };
    expect(inSelection(s, 4, 0)).toBe(false);
    expect(inSelection(s, 5, 0)).toBe(true);
    expect(inSelection(s, 0, 1)).toBe(true);
    expect(inSelection(s, 2, 2)).toBe(true);
    expect(inSelection(s, 3, 2)).toBe(false);
  });

  it('matches hovered link ranges across rows', () => {
    const r = { startX: 3, startY: 0, endX: 1, endY: 2 };
    expect(inLink(r, 2, 0)).toBe(false);
    expect(inLink(r, 3, 0)).toBe(true);
    expect(inLink(r, 9, 1)).toBe(true);
    expect(inLink(r, 1, 2)).toBe(true);
    expect(inLink(r, 2, 2)).toBe(false);
    expect(inLink(null, 0, 0)).toBe(false);
  });
});

describe('GlyphAtlas', () => {
  it('rasterizes each key once', () => {
    const a = new GlyphAtlas(4);
    const make = vi.fn();
    expect(a.slot('a', make)).toBe(0);
    expect(a.slot('b', make)).toBe(1);
    expect(a.slot('a', make)).toBe(0);
    expect(make).toHaveBeenCalledTimes(2);
    expect(a.rasterized).toBe(2);
    expect(a.size).toBe(2);
  });

  it('reports full and starts over after clear', () => {
    const a = new GlyphAtlas(1);
    a.slot('a', () => {});
    expect(a.slot('b', () => {})).toBe(-1);
    a.clear();
    expect(a.slot('b', () => {})).toBe(0);
  });
});

describe('packRow', () => {
  it('leaves default backgrounds transparent and spaces without a glyph', () => {
    const { at } = packed([cell(' '), cell('x')]);
    expect(at(0).bg[3]).toBe(0);
    expect(at(0).slot).toBe(NO_GLYPH);
    expect(at(1).slot).toBe(0);
    expect(at(1).fg).toEqual([200, 200, 200, 255]);
  });

  it('swaps colors for inverse cells and paints selection colors', () => {
    const inv = cell('a', { flags: 16, fg_r: 10, fg_g: 20, fg_b: 30, bg_r: 40, bg_g: 50, bg_b: 60 });
    expect(packed([inv]).at(0).bg).toEqual([10, 20, 30, 255]);
    expect(packed([inv]).at(0).fg).toEqual([40, 50, 60, 255]);
    const sel = packed([cell('a')], style({ selection: { startCol: 0, startRow: 0, endCol: 0, endRow: 0 } }));
    expect(sel.at(0).bg).toEqual([1, 2, 3, 255]);
    expect(sel.at(0).fg).toEqual([255, 255, 255, 255]);
  });

  it('halves alpha for faint text and skips invisible text', () => {
    expect(packed([cell('a', { flags: 128 })]).at(0).fg[3]).toBe(128);
    const hidden = packed([cell('a', { flags: 32, bg_r: 9 })]).at(0);
    expect(hidden.slot).toBe(NO_GLYPH);
    expect(hidden.bg[0]).toBe(9);
  });

  it('extends a wide cell background over its spacer', () => {
    const { at } = packed([cell('漢', { width: 2, bg_r: 7 }), cell('', { codepoint: 0, width: 0 })]);
    expect(at(1).bg).toEqual(at(0).bg);
    expect(at(1).slot).toBe(NO_GLYPH);
  });

  it('marks color glyphs and keys bold and italic separately', () => {
    const s = style();
    const { at } = packed([cell('😀'), cell('a', { flags: 1 }), cell('a', { flags: 2 }), cell('a')], s);
    expect(at(0).colored).toBe(1);
    expect(new Set([at(1).slot, at(2).slot, at(3).slot]).size).toBe(3);
  });

  it('emits underline, strikethrough and hovered link rectangles', () => {
    const { decos } = packed([cell('a', { flags: 4 }), cell('b', { flags: 8 }), cell('c', { hyperlink_id: 3 })], style({ hoveredLink: 3 }));
    expect(decos.length / 8).toBe(3);
    expect(decos.slice(0, 4)).toEqual([0, 14, 8, 1]);
    expect(decos.slice(8, 12)).toEqual([8, 8, 8, 1]);
    expect(decos.slice(16, 20)).toEqual([16, 14, 8, 1]);
  });

  it('clears the row it rewrites', () => {
    const out = new Uint8Array(2 * CELL_BYTES).fill(255);
    packRow([cell(' ')], 0, 2, style(), out, []);
    expect(out[CELL_BYTES + 3]).toBe(0);
    expect(new Uint16Array(out.buffer)[6 + 4]).toBe(NO_GLYPH);
  });
});

describe('cursorRect and scrollbarRects', () => {
  it('shapes the cursor like the canvas renderer', () => {
    expect(cursorRect(2, 1, 8, 16, 'block')).toEqual([16, 16, 8, 16]);
    expect(cursorRect(0, 0, 8, 16, 'underline')).toEqual([0, 14, 8, 2]);
    expect(cursorRect(0, 0, 20, 16, 'bar')).toEqual([0, 0, 3, 16]);
  });

  it('draws nothing without scrollback or opacity', () => {
    expect(scrollbarRects(0, 0, 10, 100, 100, 1)).toEqual([]);
    expect(scrollbarRects(0, 10, 10, 100, 100, 0)).toEqual([]);
  });

  it('draws a track and a thumb that moves up when scrolled back', () => {
    const bottom = scrollbarRects(0, 100, 10, 200, 100, 1);
    const top = scrollbarRects(100, 100, 10, 200, 100, 1);
    expect(bottom.length).toBe(16);
    expect(top[9]).toBeLessThan(bottom[9]);
    expect(top[15]).toBeGreaterThan(bottom[15]);
  });
});

describe('webglMode and softwareGl', () => {
  it('reads the query first, then localStorage', () => {
    expect(webglMode('?renderer=webgl')).toBe('gpu');
    expect(webglMode('?renderer=webgl-force')).toBe('force');
    expect(webglMode('?renderer=canvas')).toBe('off');
    vi.stubGlobal('localStorage', { getItem: () => 'webgl' });
    expect(webglMode('')).toBe('gpu');
  });

  it('stays off when storage throws', () => {
    vi.stubGlobal('localStorage', {
      getItem: () => {
        throw new Error('denied');
      },
    });
    expect(webglMode('')).toBe('off');
  });

  it('detects software renderers', () => {
    expect(softwareGl(fakeGl({ renderer: 'ANGLE (SwiftShader Device)' }).gl)).toBe(true);
    expect(softwareGl(fakeGl({ renderer: 'llvmpipe (LLVM 17)' }).gl)).toBe(true);
    expect(softwareGl(fakeGl({ renderer: 'ANGLE (AMD Radeon)' }).gl)).toBe(false);
  });
});

describe('GlTerm', () => {
  const raster = () => ({}) as TexImageSource;

  it('throws when a shader does not compile', () => {
    expect(() => new GlTerm(fakeGl({ failCompile: true }).gl, host(), raster)).toThrow(/terminal shader: bad shader/);
  });

  it('does the canvas renderer bookkeeping', () => {
    const { gl } = fakeGl();
    const sel = { hasSelection: () => false, getSelectionCoords: () => null, getDirtySelectionRows: () => new Set([1]), clearDirtySelectionRows: vi.fn() };
    const t = new GlTerm(gl, host({ selectionManager: sel }), raster);
    const buf = grid(3, 4);
    const clear = vi.spyOn(buf, 'clearDirty');
    t.render(buf);
    expect(clear).toHaveBeenCalledOnce();
    expect(sel.clearDirtySelectionRows).toHaveBeenCalledOnce();
  });

  it('repacks the selected rows and the rows of a changed link range', () => {
    const { gl } = fakeGl();
    const h = host({ selectionManager: { hasSelection: () => true, getSelectionCoords: () => ({ startCol: 0, startRow: 1, endCol: 1, endRow: 2 }), getDirtySelectionRows: () => new Set(), clearDirtySelectionRows: () => {} } });
    const t = new GlTerm(gl, h, raster);
    const buf = grid(5, 4);
    t.render(buf);
    t.rowsPacked = 0;
    t.render(buf);
    expect(t.rowsPacked).toBe(2);
    h.hoveredLinkRange = { startX: 0, startY: 4, endX: 1, endY: 4 };
    t.rowsPacked = 0;
    t.render(buf);
    expect(t.rowsPacked).toBe(3);
  });

  it('repacks everything when the hovered hyperlink or viewport changes', () => {
    const { gl } = fakeGl();
    const h = host();
    const t = new GlTerm(gl, h, raster);
    const buf = grid(4, 4);
    t.render(buf);
    h.hoveredHyperlinkId = 2;
    t.rowsPacked = 0;
    t.render(buf);
    expect(t.rowsPacked).toBe(4);
    t.rowsPacked = 0;
    t.render(buf, false, 2, scrollback([[cell('x')], [cell('y')], [cell('z')]]));
    expect(t.rowsPacked).toBe(4);
  });

  it('reads scrollback rows above the live screen when scrolled back', () => {
    const { gl } = fakeGl();
    const t = new GlTerm(gl, host(), raster);
    const buf = grid(2, 1);
    const sb = scrollback([[cell('p')], [cell('q')]]);
    const read = vi.spyOn(sb, 'getScrollbackLine');
    t.render(buf, false, 1, sb, 1);
    expect(read).toHaveBeenCalledWith(1);
  });

  it('fetches the viewport once per frame instead of once per row', () => {
    const { gl } = fakeGl();
    const t = new GlTerm(gl, host(), raster);
    const buf = grid(30, 10);
    const line = vi.spyOn(buf, 'getLine');
    t.render(buf);
    expect(buf.viewports).toBe(1);
    expect(line).not.toHaveBeenCalled();
  });

  it('falls back to getLine on a buffer without a viewport', () => {
    const { gl } = fakeGl();
    const t = new GlTerm(gl, host(), raster);
    const buf = grid(3, 2);
    delete (buf as { getViewport?: unknown }).getViewport;
    const line = vi.spyOn(buf, 'getLine');
    t.render(buf);
    expect(line).toHaveBeenCalledTimes(3);
  });

  it('skips drawing while the context is lost and redraws all rows after init', () => {
    const { gl, log } = fakeGl();
    const t = new GlTerm(gl, host(), raster);
    const buf = grid(3, 3);
    t.render(buf);
    t.lost = true;
    log.draws = 0;
    buf.setRow(0, 'abc');
    t.render(buf);
    expect(log.draws).toBe(0);
    t.init();
    t.invalidate();
    t.rowsPacked = 0;
    t.render(buf);
    expect(t.rowsPacked).toBe(3);
    expect(log.draws).toBeGreaterThan(0);
  });

  const big = { metrics: { width: 128, height: 512, baseline: 400 } };

  it('starts the atlas over when old glyphs fill it', () => {
    const { gl, log } = fakeGl();
    const t = new GlTerm(gl, host(big), raster);
    const buf = grid(1, 3, (_, x) => 'abc'[x]);
    expect(t.render(buf)).toBe(true);
    buf.setRow(0, 'def');
    expect(t.render(buf)).toBe(true);
    expect(t.atlasPixels).toBe(1024);
    expect(log.atlas).toEqual([1024]);
  });

  it('grows the atlas when one screen needs more glyphs than it holds', () => {
    const { gl, log } = fakeGl();
    const t = new GlTerm(gl, host(big), raster);
    expect(t.render(grid(1, 6, (_, x) => 'abcdef'[x]))).toBe(true);
    expect(t.atlasPixels).toBe(2048);
    expect(log.atlas).toEqual([1024, 2048]);
  });

  it('reports failure when the atlas cannot grow enough, leaving rows dirty', () => {
    const { gl, log } = fakeGl({ maxTexture: 1024 });
    const t = new GlTerm(gl, host(big), raster);
    const buf = grid(1, 6, (_, x) => 'abcdef'[x]);
    const clear = vi.spyOn(buf, 'clearDirty');
    expect(t.render(buf)).toBe(false);
    expect(clear).not.toHaveBeenCalled();
    expect(log.draws).toBe(0);
  });

  it('frees its GL objects once on dispose', () => {
    const { gl, log } = fakeGl();
    const t = new GlTerm(gl, host(), raster);
    t.dispose();
    t.dispose();
    expect(log.calls.filter((c) => c === 'deleteTexture')).toHaveLength(1);
  });
});

describe('installWebglRenderer', () => {
  class Renderer {
    calls: string[] = [];
    canvas = { style: {} as Record<string, string> };
    metrics = { width: 8, height: 16, baseline: 12 };
    render(...a: unknown[]) {
      this.calls.push(`render:${a[1] ? 'force' : 'lazy'}`);
    }
    resize(...a: number[]) {
      this.calls.push(`resize:${a.join('x')}`);
    }
    clear() {
      this.calls.push('clear');
    }
    dispose() {
      this.calls.push('dispose');
    }
    remeasureFont() {
      this.calls.push('remeasureFont');
    }
    setFontSize() {}
    setFontFamily() {}
    setTheme() {}
  }

  const buf = grid(4, 10);

  function fresh() {
    return { CanvasRenderer: class extends Renderer {} };
  }

  function glAttach(ok = true) {
    const term = { render: vi.fn((..._: unknown[]) => ok), invalidate: vi.fn() };
    const handle = { term: term as unknown as GlTerm, resize: vi.fn(), dispose: vi.fn() };
    const attach = vi.fn<Attach>(() => handle);
    return { term, handle, attach };
  }

  it('keeps the canvas renderer when ghostty-web internals changed', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    expect(installWebglRenderer({ CanvasRenderer: { prototype: {} } })).toBe(false);
    expect(warn).toHaveBeenCalled();
  });

  it('routes render, resize and clear to the GL renderer when enabled', () => {
    const mod = fresh();
    const { term, handle, attach } = glAttach();
    expect(installWebglRenderer(mod, attach)).toBe(true);
    const r = new mod.CanvasRenderer();
    r.resize(10, 4);
    r.render(buf);
    r.clear();
    r.remeasureFont();
    expect(r.calls).toEqual(['remeasureFont']);
    expect(handle.resize).toHaveBeenCalledWith(10, 4);
    expect(r.canvas.style).toEqual({ width: '80px', height: '64px' });
    expect(term.render).toHaveBeenCalledOnce();
    expect(term.invalidate).toHaveBeenCalledTimes(2);
    expect(attach).toHaveBeenCalledOnce();
    r.dispose();
    expect(handle.dispose).toHaveBeenCalledOnce();
    expect(r.calls).toContain('dispose');
  });

  it('stays on the canvas renderer while the option is off', () => {
    const mod = fresh();
    const { attach } = glAttach();
    installWebglRenderer(mod, attach, () => false);
    const r = new mod.CanvasRenderer();
    r.resize(10, 4);
    r.render(buf);
    r.clear();
    expect(attach).not.toHaveBeenCalled();
    expect(r.calls).toEqual(['resize:10x4', 'render:lazy', 'clear']);
  });

  it('switches renderers when the option changes, with a full redraw each time', () => {
    const mod = fresh();
    const { term, handle, attach } = glAttach();
    let on = false;
    installWebglRenderer(mod, attach, () => on);
    const r = new mod.CanvasRenderer();
    r.resize(10, 4);
    r.render(buf);
    on = true;
    r.render(buf);
    expect(handle.resize).toHaveBeenCalledWith(10, 4);
    expect(term.render.mock.calls[0][1]).toBe(true);
    r.render(buf);
    expect(term.render.mock.calls[1][1]).toBe(false);
    on = false;
    r.calls = [];
    r.render(buf);
    expect(handle.dispose).toHaveBeenCalledOnce();
    expect(r.calls).toEqual(['resize:10x4', 'render:force']);
  });

  it('falls back to the canvas for good when the GL renderer gives up', () => {
    const warn = vi.spyOn(console, 'warn').mockImplementation(() => {});
    const mod = fresh();
    const { handle, attach } = glAttach(false);
    installWebglRenderer(mod, attach);
    const r = new mod.CanvasRenderer();
    r.render(buf);
    expect(warn).toHaveBeenCalledWith(expect.stringMatching(/too many distinct glyphs/));
    expect(handle.dispose).toHaveBeenCalledOnce();
    expect(r.calls).toEqual(['resize:10x4', 'render:force']);
    r.render(buf);
    expect(attach).toHaveBeenCalledOnce();
  });

  it('falls back to the canvas renderer when attaching fails, without retrying', () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    const mod = fresh();
    const attach = vi.fn<Attach>(() => {
      throw new Error('no gl');
    });
    installWebglRenderer(mod, attach);
    const r = new mod.CanvasRenderer();
    r.resize(10, 4);
    r.render(buf);
    r.clear();
    expect(r.calls).toEqual(['resize:10x4', 'render:lazy', 'clear']);
    expect(attach).toHaveBeenCalledOnce();
  });

  it('patches a prototype only once', () => {
    const mod = fresh();
    const { attach } = glAttach();
    installWebglRenderer(mod, attach);
    const render = mod.CanvasRenderer.prototype.render;
    expect(installWebglRenderer(mod, attach)).toBe(true);
    expect(mod.CanvasRenderer.prototype.render).toBe(render);
  });

  it('finds every method it patches on the real ghostty-web renderer', () => {
    const p = ghostty.CanvasRenderer.prototype as unknown as Record<string, unknown>;
    for (const n of ['render', 'resize', 'clear', 'dispose', 'remeasureFont', 'setFontSize', 'setFontFamily', 'setTheme']) expect(typeof p[n]).toBe('function');
  });
});

describe('attachOverlay', () => {
  type Listener = (e: { preventDefault(): void }) => void;

  function fakeCanvas(ctx: unknown) {
    const on = new Map<string, Listener>();
    return {
      on,
      style: { cssText: '' } as Record<string, string>,
      width: 0,
      height: 0,
      removed: false,
      getContext: vi.fn(() => ctx),
      addEventListener: (n: string, f: Listener) => on.set(n, f),
      removeEventListener: (n: string) => on.delete(n),
      remove() {
        this.removed = true;
      },
    };
  }

  function setup(gl: WebGL2RenderingContext | null) {
    const ctx2d = { canvas: { width: 0, height: 0 }, clearRect: vi.fn(), fillText: vi.fn(), font: '', textBaseline: '', fillStyle: '' };
    const overlay = fakeCanvas(gl);
    const made: unknown[] = [];
    vi.stubGlobal('document', {
      createElement: () => {
        if (!made.length) {
          made.push(overlay);
          return overlay;
        }
        const c = fakeCanvas(ctx2d);
        ctx2d.canvas = c as never;
        made.push(c);
        return c;
      },
    });
    const src = { parentNode: {}, offsetLeft: 5, offsetTop: 7, style: {}, after: vi.fn() };
    const r = { ...host({ devicePixelRatio: 2 }), canvas: src as unknown as HTMLCanvasElement };
    return { overlay, src, r, ctx2d };
  }

  it('returns null when the canvas is detached or WebGL2 is missing', async () => {
    const { attachOverlay } = await import('./termgl');
    const { r, src, overlay } = setup(null);
    expect(attachOverlay({ ...r, canvas: { parentNode: null } as unknown as HTMLCanvasElement }, false)).toBeNull();
    expect(attachOverlay(r, false)).toBeNull();
    expect(src.after).not.toHaveBeenCalled();
    expect(overlay.removed).toBe(false);
  });

  it('refuses software GL unless forced', async () => {
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    const { attachOverlay } = await import('./termgl');
    const { gl, log } = fakeGl({ renderer: 'SwiftShader' });
    const { r, src, overlay } = setup(gl);
    const overlayOf = (_: unknown) => overlay;
    expect(attachOverlay(r, false)).toBeNull();
    expect(log.calls).toContain('loseContext');
    expect(overlayOf(r).getContext).toHaveBeenCalledWith('webgl2', expect.objectContaining({ failIfMajorPerformanceCaveat: true }));
    expect(src.after).not.toHaveBeenCalled();
    expect(attachOverlay(setup(fakeGl({ renderer: 'SwiftShader' }).gl).r, true)).not.toBeNull();
  });

  it('sizes the overlay over the canvas and rasterizes glyphs at device scale', async () => {
    const { attachOverlay } = await import('./termgl');
    const { gl, log } = fakeGl();
    const { r, src, overlay, ctx2d } = setup(gl);
    const a = attachOverlay(r, false)!;
    expect(src.after).toHaveBeenCalledWith(overlay);
    a.resize(10, 2);
    expect(overlay.style).toMatchObject({ marginTop: '-32px', width: '80px', height: '32px' });
    expect([overlay.width, overlay.height]).toEqual([160, 64]);
    a.term.render(grid(1, 2, () => 'a'));
    expect(log.textures).toBe(1);
    expect(ctx2d.font).toBe('26px monospace');
    expect(ctx2d.fillText).toHaveBeenCalledWith('a', 16, 24);
  });

  it('stops drawing on context loss and redraws after restore', async () => {
    const { attachOverlay } = await import('./termgl');
    const { gl, log } = fakeGl();
    const { r, overlay } = setup(gl);
    const requestRender = vi.fn();
    const a = attachOverlay({ ...r, selectionManager: { hasSelection: () => false, getSelectionCoords: () => null, getDirtySelectionRows: () => new Set(), clearDirtySelectionRows: () => {}, requestRender } as never }, false)!;
    const preventDefault = vi.fn();
    overlay.on.get('webglcontextlost')!({ preventDefault });
    expect(preventDefault).toHaveBeenCalled();
    expect(a.term.lost).toBe(true);
    log.draws = 0;
    a.term.render(grid(2, 2));
    expect(log.draws).toBe(0);
    overlay.on.get('webglcontextrestored')!({ preventDefault });
    expect(a.term.lost).toBe(false);
    expect(requestRender).toHaveBeenCalledOnce();
    a.dispose();
    expect(overlay.removed).toBe(true);
    expect(overlay.on.size).toBe(0);
  });

  it('logs instead of throwing when restore fails', async () => {
    const err = vi.spyOn(console, 'error').mockImplementation(() => {});
    const { attachOverlay } = await import('./termgl');
    const { gl } = fakeGl();
    const { r, overlay } = setup(gl);
    const a = attachOverlay(r, false)!;
    vi.spyOn(a.term, 'init').mockImplementation(() => {
      throw new Error('still lost');
    });
    overlay.on.get('webglcontextrestored')!({ preventDefault() {} });
    expect(err).toHaveBeenCalledWith('webgl terminal restore failed', expect.any(Error));
  });
});
