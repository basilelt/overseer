import type { GhosttyCell } from 'ghostty-web';
import type { Host, Renderable, Scrollback } from '../lib/termgl';

export interface GlLog {
  draws: number;
  bytes: number;
  textures: number;
  calls: string[];
  atlas: number[];
}

export function fakeGl(opts: { renderer?: string; failCompile?: boolean; maxTexture?: number } = {}): { gl: WebGL2RenderingContext; log: GlLog } {
  const log: GlLog = { draws: 0, bytes: 0, textures: 0, calls: [], atlas: [] };
  const consts = new Map<string, number>();
  const fns: Record<string, (...a: unknown[]) => unknown> = {
    drawArraysInstanced: () => void log.draws++,
    bufferSubData: (...a) => void (log.bytes += a[4] as number),
    texSubImage2D: () => void log.textures++,
    getShaderParameter: () => !opts.failCompile,
    getProgramParameter: () => true,
    getExtension: (name) => (name === 'WEBGL_debug_renderer_info' ? { UNMASKED_RENDERER_WEBGL: 0x9246 } : name === 'WEBGL_lose_context' ? { loseContext: () => log.calls.push('loseContext') } : null),
    getParameter: (p) => (p === consts.get('MAX_TEXTURE_SIZE') ? (opts.maxTexture ?? 4096) : (opts.renderer ?? 'ANGLE (NVIDIA)')),
    texImage2D: (...a) => void log.atlas.push(a[3] as number),
    getShaderInfoLog: () => 'bad shader',
    getProgramInfoLog: () => 'bad program',
  };
  const gl = new Proxy(
    {},
    {
      get(_, prop: string) {
        if (/^[A-Z0-9_]+$/.test(prop)) {
          if (!consts.has(prop)) consts.set(prop, consts.size + 1);
          return consts.get(prop);
        }
        const f = fns[prop];
        return (...a: unknown[]) => {
          log.calls.push(prop);
          return f ? f(...a) : {};
        };
      },
    },
  ) as WebGL2RenderingContext;
  return { gl, log };
}

export function cell(ch: string, over: Partial<GhosttyCell> = {}): GhosttyCell {
  return {
    codepoint: ch.codePointAt(0) ?? 32,
    fg_r: 200,
    fg_g: 200,
    fg_b: 200,
    bg_r: 0,
    bg_g: 0,
    bg_b: 0,
    flags: 0,
    width: 1,
    hyperlink_id: 0,
    grapheme_len: 0,
    ...over,
  };
}

export function grid(rows: number, cols: number, text = (y: number, x: number) => String.fromCharCode(97 + ((x + y) % 26))) {
  const lines = Array.from({ length: rows }, (_, y) => Array.from({ length: cols }, (_, x) => cell(text(y, x))));
  const dirty = new Set<number>(lines.map((_, y) => y));
  let full = true;
  const buf: Renderable & { dirty: Set<number>; lines: GhosttyCell[][]; viewports: number; cursor: { x: number; y: number; visible: boolean }; setRow(y: number, s: string): void } = {
    dirty,
    lines,
    viewports: 0,
    cursor: { x: 0, y: 0, visible: true },
    getLine: (y) => lines[y] ?? null,
    getViewport() {
      buf.viewports++;
      return lines.flat();
    },
    getCursor: () => buf.cursor,
    getDimensions: () => ({ cols, rows }),
    isRowDirty: (y) => full || dirty.has(y),
    needsFullRedraw: () => full,
    clearDirty() {
      dirty.clear();
      full = false;
    },
    setRow(y, s) {
      lines[y] = Array.from({ length: cols }, (_, x) => cell(s[x] ?? ' '));
      dirty.add(y);
    },
  };
  return buf;
}

export function scrollback(lines: GhosttyCell[][]): Scrollback {
  return { getScrollbackLine: (o) => lines[o] ?? null, getScrollbackLength: () => lines.length };
}

export function host(over: Partial<Host> = {}): Host {
  return {
    theme: { background: '#0b0b0c', foreground: '#ededef', cursor: '#ededef', selectionBackground: '#2f3b52', selectionForeground: '#ffffff' },
    metrics: { width: 8, height: 16, baseline: 12 },
    fontSize: 13,
    fontFamily: 'monospace',
    cursorStyle: 'block',
    cursorVisible: true,
    devicePixelRatio: 1,
    hoveredHyperlinkId: 0,
    hoveredLinkRange: null,
    ...over,
  };
}
