export interface TermModes {
  tracking: boolean;
  sgr: boolean;
  urxvt: boolean;
  utf8: boolean;
  alt: boolean;
  alternateScroll: boolean;
  appCursor: boolean;
  lineEditor: boolean;
}

const TRACKING = [1000, 1002, 1003] as const;

export class ModeTracker {
  private set = new Set<number>();
  private alternateScrollOff = false;
  private tail = '';
  private decoder = new TextDecoder();

  reset() {
    this.set.clear();
    this.alternateScrollOff = false;
    this.tail = '';
    this.decoder = new TextDecoder();
  }

  feed(data: Uint8Array | string) {
    const text = this.tail + (typeof data === 'string' ? data : this.decoder.decode(data, { stream: true }));
    const re = /\x1b(?:\[\?([0-9;]*)([hl])|c)/g;
    let m: RegExpExecArray | null;
    while ((m = re.exec(text))) {
      if (m[0] === '\x1bc') {
        this.set.clear();
        this.alternateScrollOff = false;
        continue;
      }
      const on = m[2] === 'h';
      for (const p of m[1].split(';')) {
        const n = Number(p);
        if (!p || !Number.isInteger(n)) continue;
        if (n === 1007) this.alternateScrollOff = !on;
        else if (on) this.set.add(n);
        else this.set.delete(n);
      }
    }
    const partial = /\x1b(?:\[(?:\?[0-9;]{0,64})?)?$/.exec(text);
    this.tail = partial ? partial[0] : '';
  }

  modes(): TermModes {
    const s = this.set;
    return {
      tracking: TRACKING.some((m) => s.has(m)),
      sgr: s.has(1006),
      urxvt: s.has(1015),
      utf8: s.has(1005),
      alt: s.has(1049) || s.has(1047) || s.has(47),
      alternateScroll: !this.alternateScrollOff,
      appCursor: s.has(1),
      lineEditor: s.has(2004),
    };
  }
}

export interface WheelInput {
  deltaY: number;
  deltaMode: 0 | 1 | 2;
  col: number;
  row: number;
  shift?: boolean;
  alt?: boolean;
  ctrl?: boolean;
}

export interface Geometry {
  cellHeight: number;
  rows: number;
}

export type WheelAction = { kind: 'send'; data: string } | { kind: 'local'; lines: number } | { kind: 'none' };

export const MAX_STEPS = 5;

export class WheelAccumulator {
  private acc = 0;

  steps(deltaY: number, deltaMode: 0 | 1 | 2, g: Geometry): number {
    const lines = deltaMode === 1 ? deltaY : deltaMode === 2 ? deltaY * g.rows : deltaY / Math.max(1, g.cellHeight);
    if (Math.sign(lines) !== Math.sign(this.acc)) this.acc = 0;
    this.acc += lines;
    const n = Math.trunc(this.acc) || 0;
    this.acc -= n;
    return Math.max(-MAX_STEPS, Math.min(MAX_STEPS, n)) || 0;
  }

  reset() {
    this.acc = 0;
  }
}

export function wheelReport(up: boolean, col: number, row: number, m: TermModes, mods: { shift?: boolean; alt?: boolean; ctrl?: boolean } = {}): string {
  const b = (up ? 64 : 65) + (mods.shift ? 4 : 0) + (mods.alt ? 8 : 0) + (mods.ctrl ? 16 : 0);
  const x = Math.max(1, col);
  const y = Math.max(1, row);
  if (m.sgr) return `\x1b[<${b};${x};${y}M`;
  if (m.urxvt) return `\x1b[${32 + b};${x};${y}M`;
  if (m.utf8) return `\x1b[M${String.fromCodePoint(32 + b, 32 + Math.min(x, 2015), 32 + Math.min(y, 2015))}`;
  return `\x1b[M${String.fromCharCode(32 + b, 32 + Math.min(x, 223), 32 + Math.min(y, 223))}`;
}

export function wheelAction(steps: number, w: WheelInput, m: TermModes, opts: { preferLocal?: boolean } = {}): WheelAction {
  if (steps === 0) return { kind: 'none' };
  const up = steps < 0;
  const n = Math.abs(steps);
  if (w.shift || (opts.preferLocal && !m.alt)) return { kind: 'local', lines: steps };
  if (m.tracking) return { kind: 'send', data: wheelReport(up, w.col, w.row, m, w).repeat(n) };
  if (m.alt && m.alternateScroll) {
    const key = `\x1b${m.appCursor ? 'O' : '['}${up ? 'A' : 'B'}`;
    return { kind: 'send', data: key.repeat(n) };
  }
  return { kind: 'local', lines: steps };
}

export function cellAt(clientX: number, clientY: number, rect: { left: number; top: number; width: number; height: number }, cols: number, rows: number): { col: number; row: number } {
  const cw = rect.width / Math.max(1, cols);
  const ch = rect.height / Math.max(1, rows);
  const col = Math.floor((clientX - rect.left) / cw) + 1;
  const row = Math.floor((clientY - rect.top) / ch) + 1;
  return { col: Math.max(1, Math.min(cols, col)), row: Math.max(1, Math.min(rows, row)) };
}

export function clickMove(col: number, row: number, cursor: { x: number; y: number }, widthAt: (x: number) => number, m: TermModes): string | null {
  if (m.tracking || !m.lineEditor || row !== cursor.y || col === cursor.x) return null;
  const from = Math.min(col, cursor.x);
  const to = Math.max(col, cursor.x);
  let n = 0;
  for (let x = from; x < to; x++) if (widthAt(x) !== 0) n++;
  if (!n) return null;
  return `\x1b${m.appCursor ? 'O' : '['}${col < cursor.x ? 'D' : 'C'}`.repeat(n);
}
