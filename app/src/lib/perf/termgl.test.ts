import { describe, expect, it } from 'vitest';
import { CELL_BYTES, GlTerm } from '../termgl';
import { fakeGl, grid, host } from '../termgl.fake';

const raster = () => ({}) as TexImageSource;

function frame(rows: number, cols: number) {
  const { gl, log } = fakeGl();
  const t = new GlTerm(gl, host(), raster);
  const buf = grid(rows, cols);
  t.render(buf);
  return { t, log, buf };
}

describe('webgl terminal renderer work', () => {
  it('issues the same draw calls for a 24×80 and a 240×80 grid', () => {
    const small = frame(24, 80).log.draws;
    const big = frame(240, 80).log.draws;
    expect(small).toBe(3);
    expect(big).toBe(small);
  });

  it('draws cursor, decorations and scrollbar in one call', () => {
    const { t, log, buf } = frame(24, 80);
    buf.setRow(3, 'underlined');
    for (const c of buf.lines[3]) c.flags = 4;
    buf.dirty.add(3);
    log.draws = 0;
    t.render(buf, false, 0, { getScrollbackLine: () => null, getScrollbackLength: () => 100 }, 1);
    expect(log.draws).toBe(3);
  });

  it('uploads one row and rasterizes nothing when one row of known glyphs changes', () => {
    const { t, log, buf } = frame(50, 120);
    log.bytes = 0;
    log.textures = 0;
    t.rowsPacked = 0;
    buf.setRow(7, 'abcdefg');
    t.render(buf);
    expect(t.rowsPacked).toBe(1);
    expect(log.bytes).toBe(120 * CELL_BYTES);
    expect(log.textures).toBe(0);
  });

  it('uploads nothing and packs nothing on a frame with no changes', () => {
    const { t, log, buf } = frame(50, 120);
    log.bytes = 0;
    t.rowsPacked = 0;
    t.render(buf);
    expect(t.rowsPacked).toBe(0);
    expect(log.bytes).toBe(0);
  });

  it('rasterizes once per unique glyph, not per cell', () => {
    const small = frame(24, 80);
    const big = frame(240, 80);
    expect(small.t.glyphs).toBe(26);
    expect(big.t.glyphs).toBe(26);
    expect(big.log.textures).toBe(small.log.textures);
  });

  it('fetches the viewport once per frame whatever the row count', () => {
    expect(frame(24, 80).buf.viewports).toBe(1);
    expect(frame(240, 80).buf.viewports).toBe(1);
  });
});
