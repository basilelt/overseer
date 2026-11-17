import { describe, expect, it } from 'vitest';
import * as mod from 'ghostty-web';
import { drawsNothing, installBlankCellSkip, type PaintedCell } from '../terminal/termrender';

interface Cell extends PaintedCell {
  width: number;
  fg_r: number;
  fg_g: number;
  fg_b: number;
  bg_r: number;
  bg_g: number;
  bg_b: number;
}

const cell = (ch: string, extra: Partial<Cell> = {}): Cell => ({
  codepoint: ch ? ch.codePointAt(0)! : 0,
  width: 1,
  flags: 0,
  grapheme_len: 0,
  hyperlink_id: 0,
  fg_r: 200,
  fg_g: 200,
  fg_b: 200,
  bg_r: 0,
  bg_g: 0,
  bg_b: 0,
  ...extra,
});

const row = (text: string, cols: number): Cell[] => Array.from({ length: cols }, (_, x) => cell(text[x] ?? ''));

function painter() {
  const calls = { fillText: 0, font: 0, stroke: 0, fillRect: 0 };
  const ctx = {
    clearRect() {},
    fillRect: () => void calls.fillRect++,
    fillText: () => void calls.fillText++,
    beginPath() {},
    moveTo() {},
    lineTo() {},
    stroke: () => void calls.stroke++,
    fillStyle: '',
    strokeStyle: '',
    lineWidth: 1,
    globalAlpha: 1,
    set font(_: string) {
      calls.font++;
    },
  };
  const r = Object.create(mod.CanvasRenderer.prototype) as { renderLine(cells: Cell[], y: number, cols: number): void } & Record<string, unknown>;
  Object.assign(r, {
    ctx,
    metrics: { width: 8, height: 16, baseline: 12 },
    theme: { background: '#000', selectionBackground: '#222', selectionForeground: '#fff' },
    fontSize: 13,
    fontFamily: 'monospace',
    hoveredHyperlinkId: 0,
    hoveredLinkRange: null,
    currentBuffer: null,
    selectionManager: null,
    currentSelectionCoords: null,
  });
  return { r, calls };
}

function paint(rows: Cell[][]) {
  const { r, calls } = painter();
  rows.forEach((cells, y) => r.renderLine(cells, y, cells.length));
  return calls;
}

describe('canvas terminal: work per painted screen', () => {
  it('patches the renderer prototype once and refuses an unknown one', () => {
    expect(installBlankCellSkip(mod)).toBe(true);
    const text = (mod.CanvasRenderer.prototype as unknown as Record<string, unknown>).renderCellText;
    expect(installBlankCellSkip(mod)).toBe(true);
    expect((mod.CanvasRenderer.prototype as unknown as Record<string, unknown>).renderCellText).toBe(text);
    expect(installBlankCellSkip({ CanvasRenderer: class {} })).toBe(false);
    expect(installBlankCellSkip({})).toBe(false);
  });

  it('draws no text and sets no font for an empty screen, whatever its size', () => {
    installBlankCellSkip(mod);
    const small = paint(Array.from({ length: 24 }, () => row('', 80)));
    const big = paint(Array.from({ length: 50 }, () => row('', 130)));
    expect(small.fillText).toBe(0);
    expect(big.fillText).toBe(0);
    expect(big.font).toBe(0);
    expect(big.fillRect).toBe(50);
  });

  it('draws one glyph per visible character, not per cell', () => {
    installBlankCellSkip(mod);
    const calls = paint([row('ls -la', 130), row('', 130), row('total 12', 130)]);
    expect(calls.fillText).toBe('ls-la'.length + 'total12'.length);
  });

  it('keeps the same work when the screen is 10 times wider with the same text', () => {
    installBlankCellSkip(mod);
    const narrow = paint([row('hello world', 40)]);
    const wide = paint([row('hello world', 400)]);
    expect(wide.fillText).toBe(narrow.fillText);
    expect(wide.font).toBe(narrow.font);
  });

  it('still draws the decoration of an underlined or struck-through space', () => {
    installBlankCellSkip(mod);
    const calls = paint([[cell(' ', { flags: 4 }), cell('', { flags: 8 }), cell(' ')]]);
    expect(calls.stroke).toBe(2);
    expect(calls.fillText).toBe(2);
  });

  it('still paints the background of a coloured space', () => {
    installBlankCellSkip(mod);
    const calls = paint([[cell(' ', { bg_r: 255 }), cell(' ')]]);
    expect(calls.fillRect).toBe(2);
    expect(calls.fillText).toBe(0);
  });
});

describe('drawsNothing', () => {
  const plain = { hoveredHyperlinkId: 0, hoveredLinkRange: null };

  it('is true for an empty cell and a plain space', () => {
    expect(drawsNothing(cell(''), plain)).toBe(true);
    expect(drawsNothing(cell(' '), plain)).toBe(true);
    expect(drawsNothing(cell(' ', { flags: 1 | 2 | 16 | 128 }), plain)).toBe(true);
  });

  it('is false for a character or a grapheme cluster', () => {
    expect(drawsNothing(cell('a'), plain)).toBe(false);
    expect(drawsNothing(cell(' ', { grapheme_len: 1 }), plain)).toBe(false);
  });

  it('is false for a space under a hovered link or link range', () => {
    expect(drawsNothing(cell(' ', { hyperlink_id: 3 }), { hoveredHyperlinkId: 3, hoveredLinkRange: null })).toBe(false);
    expect(drawsNothing(cell(' ', { hyperlink_id: 3 }), { hoveredHyperlinkId: 4, hoveredLinkRange: null })).toBe(true);
    expect(drawsNothing(cell(' '), { hoveredHyperlinkId: 0, hoveredLinkRange: { startX: 0, startY: 0, endX: 4, endY: 0 } })).toBe(false);
  });
});
