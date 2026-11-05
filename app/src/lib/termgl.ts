import type { GhosttyCell } from 'ghostty-web';

const BOLD = 1;
const ITALIC = 2;
const UNDERLINE = 4;
const STRIKE = 8;
const INVERSE = 16;
const INVISIBLE = 32;
const FAINT = 128;

export const NO_GLYPH = 0xffff;
export const CELL_BYTES = 12;
const RECT_FLOATS = 8;
const LINK = [0x4a / 255, 0x90 / 255, 0xe2 / 255, 1];
const ATLAS_START = 1024;
const ATLAS_MAX = 4096;
const BLEED = 1;

export type RGB = readonly [number, number, number];

export function parseColor(css: string | undefined, fallback: RGB = [255, 255, 255]): RGB {
  const m = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(css?.trim() ?? '');
  if (!m) return fallback;
  const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join('') : m[1];
  const n = parseInt(h, 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

export interface SelectionCoords {
  startCol: number;
  startRow: number;
  endCol: number;
  endRow: number;
}

export interface LinkRange {
  startX: number;
  startY: number;
  endX: number;
  endY: number;
}

export function inSelection(s: SelectionCoords | null, col: number, row: number): boolean {
  if (!s) return false;
  if (s.startRow === s.endRow) return row === s.startRow && col >= s.startCol && col <= s.endCol;
  if (row === s.startRow) return col >= s.startCol;
  if (row === s.endRow) return col <= s.endCol;
  return row > s.startRow && row < s.endRow;
}

export function inLink(r: LinkRange | null, col: number, row: number): boolean {
  if (!r) return false;
  return (
    (row === r.startY && col >= r.startX && (row < r.endY || col <= r.endX)) ||
    (row > r.startY && row < r.endY) ||
    (row === r.endY && col <= r.endX && (row > r.startY || col >= r.startX))
  );
}

export class GlyphAtlas {
  private slots = new Map<string, number>();
  rasterized = 0;

  constructor(readonly capacity: number) {}

  get size(): number {
    return this.slots.size;
  }

  slot(key: string, make: (slot: number) => void): number {
    const hit = this.slots.get(key);
    if (hit !== undefined) return hit;
    if (this.slots.size >= this.capacity) return -1;
    const s = this.slots.size;
    this.slots.set(key, s);
    this.rasterized++;
    make(s);
    return s;
  }

  clear(): void {
    this.slots.clear();
  }
}

export interface RowStyle {
  selFg: RGB;
  selBg: RGB;
  cellW: number;
  cellH: number;
  baseline: number;
  selection: SelectionCoords | null;
  hoveredLink: number;
  linkRange: LinkRange | null;
  glyph(text: string, bold: boolean, italic: boolean, dark: boolean): { slot: number; colored: boolean };
  text(cell: GhosttyCell, col: number): string;
}

export function packRow(cells: readonly GhosttyCell[], row: number, cols: number, style: RowStyle, out: Uint8Array, decos: number[]): void {
  const base = row * cols * CELL_BYTES;
  out.fill(0, base, base + cols * CELL_BYTES);
  const u16 = new Uint16Array(out.buffer, out.byteOffset + base, cols * (CELL_BYTES / 2));
  for (let c = 0; c < cols; c++) u16[c * 6 + 4] = NO_GLYPH;
  decos.length = 0;
  const y = row * style.cellH;
  const line = (x: number, w: number, at: number, rgba: readonly number[]) => decos.push(x, at, w, 1, rgba[0], rgba[1], rgba[2], rgba[3]);
  let spill = -1;
  const n = Math.min(cells.length, cols);
  for (let c = 0; c < n; c++) {
    const cell = cells[c];
    const o = base + c * CELL_BYTES;
    if (cell.width === 0) {
      if (spill >= 0) out.copyWithin(o, spill, spill + 4);
      spill = -1;
      continue;
    }
    spill = cell.width > 1 ? o : -1;
    const sel = inSelection(style.selection, c, row);
    const inv = (cell.flags & INVERSE) !== 0;
    let br = inv ? cell.fg_r : cell.bg_r;
    let bg = inv ? cell.fg_g : cell.bg_g;
    let bb = inv ? cell.fg_b : cell.bg_b;
    if (sel) [br, bg, bb] = style.selBg;
    if (sel || br || bg || bb) {
      out[o] = br;
      out[o + 1] = bg;
      out[o + 2] = bb;
      out[o + 3] = 255;
    }
    if (cell.flags & INVISIBLE) continue;
    let fr = inv ? cell.bg_r : cell.fg_r;
    let fg = inv ? cell.bg_g : cell.fg_g;
    let fb = inv ? cell.bg_b : cell.fg_b;
    if (sel) [fr, fg, fb] = style.selFg;
    const alpha = cell.flags & FAINT ? 128 : 255;
    out[o + 4] = fr;
    out[o + 5] = fg;
    out[o + 6] = fb;
    out[o + 7] = alpha;
    const cp = cell.codepoint;
    if (cp > 32 || cell.grapheme_len > 0) {
      const g = style.glyph(style.text(cell, c), (cell.flags & BOLD) !== 0, (cell.flags & ITALIC) !== 0, fr * 299 + fg * 587 + fb * 114 < 128000);
      if (g.slot >= 0 && g.slot < NO_GLYPH) {
        u16[c * 6 + 4] = g.slot;
        u16[c * 6 + 5] = g.colored ? 1 : 0;
      }
    }
    const x = c * style.cellW;
    const w = style.cellW * cell.width;
    const fgRgba = [fr / 255, fg / 255, fb / 255, alpha / 255];
    if (cell.flags & UNDERLINE) line(x, w, y + style.baseline + 2, fgRgba);
    if (cell.flags & STRIKE) line(x, w, y + Math.floor(style.cellH / 2), fgRgba);
    if ((cell.hyperlink_id > 0 && cell.hyperlink_id === style.hoveredLink) || inLink(style.linkRange, c, row)) line(x, w, y + style.baseline + 2, LINK);
  }
}

export function cursorRect(x: number, y: number, cellW: number, cellH: number, shape: string): [number, number, number, number] {
  const px = x * cellW;
  const py = y * cellH;
  if (shape === 'underline') {
    const h = Math.max(2, Math.floor(cellH * 0.15));
    return [px, py + cellH - h, cellW, h];
  }
  if (shape === 'bar') return [px, py, Math.max(2, Math.floor(cellW * 0.15)), cellH];
  return [px, py, cellW, cellH];
}

export function scrollbarRects(viewportY: number, scrollback: number, rows: number, width: number, height: number, opacity: number): number[] {
  if (opacity <= 0 || scrollback === 0) return [];
  const w = 8;
  const x = width - w - 4;
  const pad = 4;
  const track = height - pad * 2;
  const thumb = Math.max(20, (rows / (scrollback + rows)) * track);
  const top = pad + (track - thumb) * (1 - viewportY / scrollback);
  const g = 128 / 255;
  return [x, pad, w, track, g, g, g, 0.1 * opacity, x, top, w, thumb, g, g, g, (viewportY > 0 ? 0.5 : 0.3) * opacity];
}

export interface Renderable {
  getLine(y: number): GhosttyCell[] | null;
  getCursor(): { x: number; y: number; visible: boolean };
  getDimensions(): { cols: number; rows: number };
  isRowDirty(y: number): boolean;
  needsFullRedraw?(): boolean;
  clearDirty(): void;
  getGraphemeString?(row: number, col: number): string;
  getViewport?(): GhosttyCell[];
  update?(): unknown;
}

export interface Scrollback {
  getScrollbackLine(offset: number): GhosttyCell[] | null;
  getScrollbackLength(): number;
}

export interface Host {
  theme: Record<string, string | undefined>;
  metrics: { width: number; height: number; baseline: number };
  fontSize: number;
  fontFamily: string;
  cursorStyle: string;
  cursorVisible: boolean;
  devicePixelRatio: number;
  hoveredHyperlinkId?: number;
  hoveredLinkRange?: LinkRange | null;
  selectionManager?: {
    hasSelection(): boolean;
    getSelectionCoords(): SelectionCoords | null;
    getDirtySelectionRows(): Set<number>;
    clearDirtySelectionRows(): void;
  };
}

export type Raster = (text: string, font: string, colored: boolean, dark: boolean, w: number, h: number, x: number, baseline: number) => TexImageSource;

const COLORED = /\p{Extended_Pictographic}/u;

const CELL_VS = `#version 300 es
uniform vec2 uView;
uniform vec2 uCell;
uniform int uCols;
uniform vec2 uSlot;
uniform int uAtlasCols;
uniform int uPass;
layout(location = 0) in vec4 aBg;
layout(location = 1) in vec4 aFg;
layout(location = 2) in uvec2 aGlyph;
out vec4 vColor;
out vec2 vUv;
flat out uint vColored;
void main() {
  vec2 corner = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
  vec2 cell = vec2(float(gl_InstanceID % uCols), float(gl_InstanceID / uCols));
  vColored = aGlyph.y;
  bool hide;
  vec2 size;
  if (uPass == 0) {
    hide = aBg.a == 0.0;
    size = uCell;
    vColor = aBg;
  } else {
    hide = aGlyph.x == 65535u;
    size = vec2(uCell.x * 3.0, uCell.y);
    cell.x -= 1.0;
    vColor = aFg;
    int slot = int(aGlyph.x);
    vUv = (vec2(float(slot % uAtlasCols), float(slot / uAtlasCols)) + corner) * uSlot;
  }
  vec2 p = cell * uCell + corner * size;
  gl_Position = hide ? vec4(2.0, 2.0, 2.0, 1.0) : vec4(p.x / uView.x * 2.0 - 1.0, 1.0 - p.y / uView.y * 2.0, 0.0, 1.0);
}`;

const CELL_FS = `#version 300 es
precision mediump float;
precision highp int;
uniform sampler2D uAtlas;
uniform int uPass;
in vec4 vColor;
in vec2 vUv;
flat in uint vColored;
out vec4 o;
void main() {
  if (uPass == 0) { o = vec4(vColor.rgb, 1.0); return; }
  vec4 t = texture(uAtlas, vUv);
  o = vColored == 1u ? t : vec4(vColor.rgb, 1.0) * t.a * vColor.a;
}`;

const RECT_VS = `#version 300 es
uniform vec2 uView;
layout(location = 0) in vec4 aRect;
layout(location = 1) in vec4 aColor;
out vec4 vColor;
void main() {
  vec2 corner = vec2(float(gl_VertexID & 1), float((gl_VertexID >> 1) & 1));
  vec2 p = aRect.xy + corner * aRect.zw;
  vColor = vec4(aColor.rgb * aColor.a, aColor.a);
  gl_Position = vec4(p.x / uView.x * 2.0 - 1.0, 1.0 - p.y / uView.y * 2.0, 0.0, 1.0);
}`;

const RECT_FS = `#version 300 es
precision mediump float;
in vec4 vColor;
out vec4 o;
void main() { o = vColor; }`;

interface Res {
  cell: WebGLProgram;
  rect: WebGLProgram;
  cellVao: WebGLVertexArrayObject;
  rectVao: WebGLVertexArrayObject;
  cells: WebGLBuffer;
  rects: WebGLBuffer;
  atlas: WebGLTexture;
  u: Record<string, WebGLUniformLocation | null>;
}

interface Frame {
  W: number;
  H: number;
  cols: number;
  rows: number;
  cellW: number;
  cellH: number;
  n: number;
  dpr: number;
  bg: RGB;
}

export class GlTerm {
  draws = 0;
  redraws = 0;
  settle = 0;
  private inked = false;
  onSettle: (() => void) | null = null;
  private frame: Frame | null = null;
  uploaded = 0;
  rowsPacked = 0;
  lost = false;
  private res: Res | null = null;
  private atlas = new GlyphAtlas(0);
  private atlasSize = ATLAS_START;
  private atlasMax = ATLAS_START;
  private atlasCols = 1;
  private slotW = 0;
  private slotH = 0;
  private cols = 0;
  private rows = 0;
  private cells = new Uint8Array(0);
  private decos: number[][] = [];
  private full = true;
  private lastVy = 0;
  private prevLink = 0;
  private prevRange: LinkRange | null = null;
  private metricsKey = '';
  private rectData = new Float32Array(64);
  private overflowed = false;

  constructor(
    private gl: WebGL2RenderingContext,
    private host: Host,
    private raster: Raster,
  ) {
    this.init();
  }

  init(): void {
    const gl = this.gl;
    const prog = (vs: string, fs: string) => {
      const p = gl.createProgram()!;
      for (const [type, src] of [
        [gl.VERTEX_SHADER, vs],
        [gl.FRAGMENT_SHADER, fs],
      ] as const) {
        const s = gl.createShader(type)!;
        gl.shaderSource(s, src);
        gl.compileShader(s);
        if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(`terminal shader: ${gl.getShaderInfoLog(s)}`);
        gl.attachShader(p, s);
      }
      gl.linkProgram(p);
      if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(`terminal program: ${gl.getProgramInfoLog(p)}`);
      return p;
    };
    const cell = prog(CELL_VS, CELL_FS);
    const rect = prog(RECT_VS, RECT_FS);
    const cells = gl.createBuffer()!;
    const rects = gl.createBuffer()!;
    const cellVao = gl.createVertexArray()!;
    gl.bindVertexArray(cellVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, cells);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 4, gl.UNSIGNED_BYTE, true, CELL_BYTES, 0);
    gl.vertexAttribDivisor(0, 1);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 4, gl.UNSIGNED_BYTE, true, CELL_BYTES, 4);
    gl.vertexAttribDivisor(1, 1);
    gl.enableVertexAttribArray(2);
    gl.vertexAttribIPointer(2, 2, gl.UNSIGNED_SHORT, CELL_BYTES, 8);
    gl.vertexAttribDivisor(2, 1);
    const rectVao = gl.createVertexArray()!;
    gl.bindVertexArray(rectVao);
    gl.bindBuffer(gl.ARRAY_BUFFER, rects);
    gl.enableVertexAttribArray(0);
    gl.vertexAttribPointer(0, 4, gl.FLOAT, false, RECT_FLOATS * 4, 0);
    gl.vertexAttribDivisor(0, 1);
    gl.enableVertexAttribArray(1);
    gl.vertexAttribPointer(1, 4, gl.FLOAT, false, RECT_FLOATS * 4, 16);
    gl.vertexAttribDivisor(1, 1);
    gl.bindVertexArray(null);
    const atlas = gl.createTexture()!;
    gl.bindTexture(gl.TEXTURE_2D, atlas);
    this.atlasMax = Math.max(ATLAS_START, Math.min(ATLAS_MAX, Number(gl.getParameter(gl.MAX_TEXTURE_SIZE)) || ATLAS_START));
    this.atlasSize = ATLAS_START;
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, this.atlasSize, this.atlasSize, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MIN_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_MAG_FILTER, gl.NEAREST);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_S, gl.CLAMP_TO_EDGE);
    gl.texParameteri(gl.TEXTURE_2D, gl.TEXTURE_WRAP_T, gl.CLAMP_TO_EDGE);
    gl.pixelStorei(gl.UNPACK_PREMULTIPLY_ALPHA_WEBGL, true);
    const u: Res['u'] = {};
    for (const n of ['uView', 'uCell', 'uCols', 'uSlot', 'uAtlasCols', 'uPass', 'uAtlas']) u[n] = gl.getUniformLocation(cell, n);
    u.rView = gl.getUniformLocation(rect, 'uView');
    this.res = { cell, rect, cellVao, rectVao, cells, rects, atlas, u };
    this.lost = false;
    this.metricsKey = '';
    this.alloc(this.cols, this.rows);
  }

  dispose(): void {
    const r = this.res;
    this.res = null;
    if (!r || this.lost) return;
    const gl = this.gl;
    gl.deleteProgram(r.cell);
    gl.deleteProgram(r.rect);
    gl.deleteBuffer(r.cells);
    gl.deleteBuffer(r.rects);
    gl.deleteVertexArray(r.cellVao);
    gl.deleteVertexArray(r.rectVao);
    gl.deleteTexture(r.atlas);
  }

  get glyphs(): number {
    return this.atlas.rasterized;
  }

  invalidate(): void {
    this.full = true;
    this.metricsKey = '';
  }

  private alloc(cols: number, rows: number): void {
    this.cols = cols;
    this.rows = rows;
    this.frame = null;
    this.cells = new Uint8Array(cols * rows * CELL_BYTES);
    this.decos = Array.from({ length: rows }, () => []);
    this.full = true;
    const r = this.res;
    if (!r || this.lost) return;
    this.gl.bindBuffer(this.gl.ARRAY_BUFFER, r.cells);
    this.gl.bufferData(this.gl.ARRAY_BUFFER, this.cells.byteLength, this.gl.DYNAMIC_DRAW);
  }

  resize(cols: number, rows: number): void {
    if (cols !== this.cols || rows !== this.rows) this.alloc(cols, rows);
  }

  private syncMetrics(): void {
    const { width, height, baseline } = this.host.metrics;
    const dpr = this.host.devicePixelRatio || 1;
    const key = `${width}x${height}x${baseline}@${dpr}:${this.host.fontSize}:${this.host.fontFamily}`;
    if (key === this.metricsKey) return;
    this.metricsKey = key;
    this.slotW = Math.ceil(width * (2 + BLEED) * dpr);
    this.slotH = Math.ceil(height * dpr);
    this.layout();
  }

  private layout(): void {
    this.atlasCols = Math.max(1, Math.floor(this.atlasSize / this.slotW));
    this.atlas = new GlyphAtlas(this.atlasCols * Math.max(1, Math.floor(this.atlasSize / this.slotH)));
    this.full = true;
  }

  private grow(): boolean {
    if (this.atlasSize >= this.atlasMax) return false;
    this.atlasSize *= 2;
    const gl = this.gl;
    gl.bindTexture(gl.TEXTURE_2D, this.res!.atlas);
    gl.texImage2D(gl.TEXTURE_2D, 0, gl.RGBA, this.atlasSize, this.atlasSize, 0, gl.RGBA, gl.UNSIGNED_BYTE, null);
    this.layout();
    return true;
  }

  get atlasPixels(): number {
    return this.atlasSize;
  }

  private glyph = (text: string, bold: boolean, italic: boolean, dark: boolean): { slot: number; colored: boolean } => {
    const colored = COLORED.test(text);
    dark &&= !colored;
    const key = `${bold ? 'b' : ''}${italic ? 'i' : ''}${dark ? 'd' : ''}:${text}`;
    const make = (slot: number) => {
      const h = this.host;
      const font = `${italic ? 'italic ' : ''}${bold ? 'bold ' : ''}${h.fontSize}px ${h.fontFamily}`;
      const dpr = h.devicePixelRatio || 1;
      const src = this.raster(text, font, colored, dark, this.slotW, this.slotH, BLEED * h.metrics.width * dpr, h.metrics.baseline * dpr);
      const gl = this.gl;
      gl.bindTexture(gl.TEXTURE_2D, this.res!.atlas);
      gl.texSubImage2D(gl.TEXTURE_2D, 0, (slot % this.atlasCols) * this.slotW, Math.floor(slot / this.atlasCols) * this.slotH, gl.RGBA, gl.UNSIGNED_BYTE, src);
    };
    const slot = this.atlas.slot(key, make);
    if (slot < 0) this.overflowed = true;
    return { slot, colored };
  };

  render(buf: Renderable, force = false, viewportY = 0, sb?: Scrollback, opacity = 1): boolean {
    const h = this.host;
    const cursor = buf.getCursor();
    const { cols, rows } = buf.getDimensions();
    const scrollback = sb ? sb.getScrollbackLength() : 0;
    if (buf.needsFullRedraw?.()) force = true;
    this.resize(cols, rows);
    this.syncMetrics();
    if (viewportY !== this.lastVy) {
      force = true;
      this.lastVy = viewportY;
    }
    const dirty = new Set<number>();
    const sm = h.selectionManager;
    const selection = sm?.hasSelection() ? sm.getSelectionCoords() : null;
    if (selection) for (let y = selection.startRow; y <= selection.endRow; y++) dirty.add(y);
    if (sm) {
      const d = sm.getDirtySelectionRows();
      if (d.size) {
        for (const y of d) dirty.add(y);
        sm.clearDirtySelectionRows();
      }
    }
    const hovered = h.hoveredHyperlinkId ?? 0;
    if (hovered !== this.prevLink) {
      force = true;
      this.prevLink = hovered;
    }
    const range = h.hoveredLinkRange ?? null;
    if (JSON.stringify(range) !== JSON.stringify(this.prevRange)) {
      for (const r of [this.prevRange, range]) if (r) for (let y = r.startY; y <= r.endY; y++) dirty.add(y);
      this.prevRange = range;
    }
    if (this.lost || !this.res) return true;
    const { width: cellW, height: cellH } = h.metrics;
    let packing = 0;
    let viewport: GhosttyCell[] | null = null;
    const screenLine = (y: number): readonly GhosttyCell[] | null => {
      if (!buf.getViewport) return buf.getLine(y);
      if (!viewport) {
        buf.update?.();
        viewport = buf.getViewport();
      }
      return y >= 0 && y < rows ? viewport.slice(y * cols, y * cols + cols) : null;
    };
    const lineAt = (y: number) => {
      if (viewportY <= 0) return screenLine(y);
      const top = Math.floor(viewportY);
      return y < top && sb ? sb.getScrollbackLine(scrollback - top + y) : screenLine(y - top);
    };
    const style: RowStyle = {
      selFg: parseColor(h.theme.selectionForeground),
      selBg: parseColor(h.theme.selectionBackground),
      cellW,
      cellH,
      baseline: h.metrics.baseline,
      selection,
      hoveredLink: hovered,
      linkRange: range,
      glyph: this.glyph,
      text: (cell, col) => (cell.grapheme_len > 0 && buf.getGraphemeString ? buf.getGraphemeString(packing, col) : String.fromCodePoint(cell.codepoint || 32)),
    };
    let lo = rows;
    let hi = -1;
    const pack = (all: boolean) => {
      for (let y = 0; y < rows; y++) {
        if (!(all || this.full || viewportY > 0 || buf.isRowDirty(y) || dirty.has(y))) continue;
        const cells = lineAt(y);
        if (!cells) continue;
        packing = y;
        packRow(cells, y, cols, style, this.cells, this.decos[y]);
        if (this.settle > 0 && !this.inked) for (const c of cells) if (c.codepoint > 32) this.inked = true;
        this.rowsPacked++;
        lo = Math.min(lo, y);
        hi = Math.max(hi, y);
      }
    };
    this.overflowed = false;
    pack(force);
    for (let tries = 0; this.overflowed; tries++) {
      if (tries === 0) this.atlas.clear();
      else if (!this.grow()) {
        this.full = true;
        return false;
      }
      this.overflowed = false;
      pack(true);
    }
    this.full = false;
    const gl = this.gl;
    const r = this.res;
    if (hi >= lo) {
      const from = lo * cols * CELL_BYTES;
      const to = (hi + 1) * cols * CELL_BYTES;
      gl.bindBuffer(gl.ARRAY_BUFFER, r.cells);
      gl.bufferSubData(gl.ARRAY_BUFFER, from, this.cells, from, to - from);
      this.uploaded += to - from;
    }
    const W = cols * cellW;
    const H = rows * cellH;
    const extra: number[] = [];
    if (viewportY === 0 && cursor.visible && h.cursorVisible) {
      const c = parseColor(h.theme.cursor);
      extra.push(...cursorRect(cursor.x, cursor.y, cellW, cellH, h.cursorStyle), c[0] / 255, c[1] / 255, c[2] / 255, 1);
    }
    if (sb) extra.push(...scrollbarRects(viewportY, scrollback, rows, W, H, opacity));
    let n = extra.length;
    for (const d of this.decos) n += d.length;
    if (this.rectData.length < n) this.rectData = new Float32Array(n * 2);
    let at = 0;
    for (const d of this.decos) {
      this.rectData.set(d, at);
      at += d.length;
    }
    this.rectData.set(extra, at);
    const bg = parseColor(h.theme.background, [0, 0, 0]);
    const frame: Frame = { W, H, cols, rows, cellW, cellH, n, dpr: h.devicePixelRatio || 1, bg };
    this.frame = frame;
    this.paint(frame);
    if (this.settle > 0 && hi >= lo) {
      if (this.settle > 1 || this.inked) this.settle--;
      this.onSettle?.();
    }
    buf.clearDirty();
    return true;
  }

  startSettle(renders: number): void {
    this.settle = renders;
    this.inked = false;
  }

  redraw(): boolean {
    const f = this.frame;
    if (!f || this.lost || !this.res) return false;
    const gl = this.gl;
    gl.bindBuffer(gl.ARRAY_BUFFER, this.res.cells);
    gl.bufferSubData(gl.ARRAY_BUFFER, 0, this.cells, 0, this.cells.length);
    this.paint(f);
    this.redraws++;
    return true;
  }

  private paint(f: Frame): void {
    const { W, H, cols, rows, cellW, cellH, n, dpr, bg } = f;
    const gl = this.gl;
    const r = this.res!;
    gl.viewport(0, 0, Math.round(W * dpr), Math.round(H * dpr));
    gl.clearColor(bg[0] / 255, bg[1] / 255, bg[2] / 255, 1);
    gl.clear(gl.COLOR_BUFFER_BIT);
    gl.enable(gl.BLEND);
    gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
    gl.useProgram(r.cell);
    gl.uniform2f(r.u.uView, W, H);
    gl.uniform2f(r.u.uCell, cellW, cellH);
    gl.uniform1i(r.u.uCols, cols);
    gl.uniform2f(r.u.uSlot, this.slotW / this.atlasSize, this.slotH / this.atlasSize);
    gl.uniform1i(r.u.uAtlasCols, this.atlasCols);
    gl.uniform1i(r.u.uAtlas, 0);
    gl.activeTexture(gl.TEXTURE0);
    gl.bindTexture(gl.TEXTURE_2D, r.atlas);
    gl.bindVertexArray(r.cellVao);
    for (const pass of [0, 1]) {
      gl.uniform1i(r.u.uPass, pass);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, cols * rows);
      this.draws++;
    }
    if (n) {
      gl.useProgram(r.rect);
      gl.uniform2f(r.u.rView, W, H);
      gl.bindVertexArray(r.rectVao);
      gl.bindBuffer(gl.ARRAY_BUFFER, r.rects);
      gl.bufferData(gl.ARRAY_BUFFER, this.rectData, gl.STREAM_DRAW, 0, n);
      gl.drawArraysInstanced(gl.TRIANGLE_STRIP, 0, 4, n / RECT_FLOATS);
      this.draws++;
    }
    gl.bindVertexArray(null);
  }
}

export type GlMode = 'off' | 'gpu' | 'force';

export function webglMode(search = globalThis.location?.search ?? ''): GlMode {
  let v = new URLSearchParams(search).get('renderer');
  if (!v) {
    try {
      v = globalThis.localStorage?.getItem('overseer.renderer') ?? null;
    } catch {
      v = null;
    }
  }
  return v === 'webgl' ? 'gpu' : v === 'webgl-force' ? 'force' : 'off';
}

const SOFTWARE = /swiftshader|llvmpipe|softpipe|software|basic render/i;

export function softwareGl(gl: WebGL2RenderingContext): boolean {
  const ext = gl.getExtension('WEBGL_debug_renderer_info');
  const name = String(ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER));
  return SOFTWARE.test(name);
}

type Proto = Record<string, unknown>;

interface Patched extends Host {
  canvas: HTMLCanvasElement;
}

export interface Handle {
  term: GlTerm;
  resize(cols: number, rows: number): void;
  dispose(): void;
}

export type Attach = (r: Patched) => Handle | null;

const GL = Symbol.for('overseer.webgl');

interface State {
  g: Handle | null;
  failed: boolean;
  cols: number;
  rows: number;
}

export function installWebglRenderer(mod: { CanvasRenderer?: { prototype: object } }, attach: Attach = (r) => attachOverlay(r, false), enabled: () => boolean = () => true): boolean {
  const p = mod.CanvasRenderer?.prototype as (Proto & { [GL]?: boolean }) | undefined;
  const names = ['render', 'resize', 'clear', 'dispose', 'remeasureFont', 'setFontSize', 'setFontFamily', 'setTheme'];
  if (!p || !names.every((n) => typeof p[n] === 'function')) {
    console.warn('ghostty-web internals changed: keeping the canvas renderer');
    return false;
  }
  if (p[GL]) return true;
  p[GL] = true;
  const orig = Object.fromEntries(names.map((n) => [n, p[n] as (...a: unknown[]) => unknown]));
  const states = new WeakMap<object, State>();
  const stateOf = (r: Patched) => {
    let s = states.get(r);
    if (!s) states.set(r, (s = { g: null, failed: false, cols: 0, rows: 0 }));
    return s;
  };
  const drop = (r: Patched, s: State) => {
    s.g?.dispose();
    s.g = null;
    if (s.cols && s.rows) orig.resize.call(r, s.cols, s.rows);
  };
  const sync = (r: Patched): { g: Handle | null; switched: boolean } => {
    const s = stateOf(r);
    const want = !s.failed && enabled();
    if (want && !s.g) {
      try {
        s.g = attach(r);
      } catch (err) {
        console.warn('webgl terminal unavailable, using canvas', err);
      }
      if (!s.g) {
        s.failed = true;
        return { g: null, switched: false };
      }
      if (s.cols && s.rows) s.g.resize(s.cols, s.rows);
      return { g: s.g, switched: true };
    }
    if (!want && s.g) {
      drop(r, s);
      return { g: null, switched: true };
    }
    return { g: s.g, switched: false };
  };
  p.render = function (this: Patched, buf: Renderable, force?: boolean, ...rest: unknown[]) {
    const s = stateOf(this);
    const d = buf.getDimensions();
    s.cols = d.cols;
    s.rows = d.rows;
    const { g, switched } = sync(this);
    if (g) {
      if (g.term.render(buf, force || switched, ...(rest as [number, Scrollback, number]))) return;
      console.warn('webgl terminal: too many distinct glyphs on screen, using canvas');
      s.failed = true;
      drop(this, s);
      return orig.render.call(this, buf, true, ...rest);
    }
    return orig.render.call(this, buf, force || switched, ...rest);
  };
  p.resize = function (this: Patched, cols: number, rows: number) {
    const s = stateOf(this);
    s.cols = cols;
    s.rows = rows;
    const { g } = sync(this);
    if (!g) return orig.resize.call(this, cols, rows);
    const { width, height } = this.metrics;
    this.canvas.style.width = `${cols * width}px`;
    this.canvas.style.height = `${rows * height}px`;
    g.resize(cols, rows);
  };
  p.clear = function (this: Patched) {
    const g = states.get(this)?.g;
    if (!g) return orig.clear.call(this);
    g.term.invalidate();
  };
  p.dispose = function (this: Patched, ...a: unknown[]) {
    states.get(this)?.g?.dispose();
    states.delete(this);
    return orig.dispose.apply(this, a);
  };
  for (const n of ['remeasureFont', 'setFontSize', 'setFontFamily', 'setTheme']) {
    p[n] = function (this: Patched, ...a: unknown[]) {
      const out = orig[n].apply(this, a);
      states.get(this)?.g?.term.invalidate();
      return out;
    };
  }
  return true;
}

function rasterizer(dpr: number): Raster {
  let ctx: CanvasRenderingContext2D | null = null;
  return (text, font, colored, dark, w, h, x, baseline) => {
    if (!ctx || ctx.canvas.width !== w || ctx.canvas.height !== h) {
      const c = document.createElement('canvas');
      c.width = w;
      c.height = h;
      ctx = c.getContext('2d')!;
    }
    ctx.clearRect(0, 0, w, h);
    ctx.font = font.replace(/(\d+(?:\.\d+)?)px/, (_, px) => `${Number(px) * dpr}px`);
    ctx.textBaseline = 'alphabetic';
    // Text rasterizers (Skia, CoreGraphics) draw light ink heavier than dark ink; match the real text luminance or dark text renders bold.
    ctx.fillStyle = colored || dark ? '#000' : '#fff';
    ctx.fillText(text, x, baseline);
    return ctx.canvas;
  };
}

const SETTLE_RENDERS = 3;

export function attachOverlay(r: Patched, allowSoftware: boolean): ReturnType<Attach> {
  const src = r.canvas;
  if (!src?.parentNode || typeof document === 'undefined') return null;
  const c = document.createElement('canvas');
  c.style.cssText = 'display:block;position:relative;pointer-events:none;';
  const gl = c.getContext('webgl2', { alpha: false, antialias: false, depth: false, stencil: false, premultipliedAlpha: true, preserveDrawingBuffer: true, failIfMajorPerformanceCaveat: !allowSoftware });
  if (!gl) return null;
  if (!allowSoftware && softwareGl(gl)) {
    console.warn('webgl terminal: software GL, using canvas');
    gl.getExtension('WEBGL_lose_context')?.loseContext();
    return null;
  }
  const dpr = r.devicePixelRatio || 1;
  const term = new GlTerm(gl, r, rasterizer(dpr));
  src.after(c);
  const place = (cols: number, rows: number) => {
    const { width, height } = r.metrics;
    c.style.marginTop = `-${rows * height}px`;
    c.style.width = `${cols * width}px`;
    c.style.height = `${rows * height}px`;
    c.width = Math.round(cols * width * dpr);
    c.height = Math.round(rows * height * dpr);
  };
  const lost = (e: Event) => {
    e.preventDefault();
    term.lost = true;
  };
  let raf = 0;
  term.onSettle = () => {
    if (raf || typeof requestAnimationFrame !== 'function') return;
    raf = requestAnimationFrame(() => {
      raf = 0;
      term.redraw();
    });
  };
  const restored = () => {
    try {
      term.init();
      term.invalidate();
      (r.selectionManager as { requestRender?: () => void } | undefined)?.requestRender?.();
    } catch (err) {
      console.error('webgl terminal restore failed', err);
    }
  };
  c.addEventListener('webglcontextlost', lost);
  c.addEventListener('webglcontextrestored', restored);
  return {
    term,
    resize(cols, rows) {
      place(cols, rows);
      term.resize(cols, rows);
      term.startSettle(SETTLE_RENDERS);
    },
    dispose() {
      if (raf) cancelAnimationFrame(raf);
      raf = 0;
      term.dispose();
      c.removeEventListener('webglcontextlost', lost);
      c.removeEventListener('webglcontextrestored', restored);
      c.remove();
    },
  };
}
