import { describe, expect, it } from 'vitest';
import { cellAt, clickMove, ModeTracker, WheelAccumulator, wheelAction, wheelReport, type TermModes } from './mouse';

const base: TermModes = { tracking: false, sgr: false, urxvt: false, utf8: false, alt: false, alternateScroll: true, appCursor: false };
const at = { deltaY: 0, deltaMode: 0 as const, col: 3, row: 7 };

describe('ModeTracker', () => {
  it('tracks DECSET/DECRST mouse, encoding and alternate screen modes', () => {
    const t = new ModeTracker();
    expect(t.modes()).toEqual(base);
    t.feed('\x1b[?1049h\x1b[?1000;1006h');
    expect(t.modes()).toMatchObject({ tracking: true, sgr: true, alt: true });
    t.feed(new TextEncoder().encode('text\x1b[?1006l\x1b[?1015h'));
    expect(t.modes()).toMatchObject({ tracking: true, sgr: false, urxvt: true });
    t.feed('\x1b[?1000l\x1b[?1049l');
    expect(t.modes()).toMatchObject({ tracking: false, alt: false });
  });

  it('handles 1002/1003, 47/1047, 1007 and DECCKM', () => {
    const t = new ModeTracker();
    t.feed('\x1b[?1003h\x1b[?47h\x1b[?1h\x1b[?1007l');
    expect(t.modes()).toMatchObject({ tracking: true, alt: true, appCursor: true, alternateScroll: false });
    t.feed('\x1b[?1007h\x1b[?47l\x1b[?1047h');
    expect(t.modes()).toMatchObject({ alt: true, alternateScroll: true });
  });

  it('joins sequences split across chunks', () => {
    const t = new ModeTracker();
    t.feed('abc\x1b[?10');
    expect(t.modes().tracking).toBe(false);
    t.feed('02h');
    expect(t.modes().tracking).toBe(true);
    t.feed('\x1b');
    t.feed('[?1002l');
    expect(t.modes().tracking).toBe(false);
  });

  it('clears on RIS and reset()', () => {
    const t = new ModeTracker();
    t.feed('\x1b[?1000h\x1bc');
    expect(t.modes().tracking).toBe(false);
    t.feed('\x1b[?1049h');
    t.reset();
    expect(t.modes()).toEqual(base);
  });
});

describe('wheel', () => {
  it('encodes SGR, urxvt, UTF-8 and X10 reports', () => {
    expect(wheelReport(true, 3, 7, { ...base, tracking: true, sgr: true })).toBe('\x1b[<64;3;7M');
    expect(wheelReport(false, 3, 7, { ...base, tracking: true, sgr: true })).toBe('\x1b[<65;3;7M');
    expect(wheelReport(true, 3, 7, { ...base, tracking: true, sgr: true }, { ctrl: true })).toBe('\x1b[<80;3;7M');
    expect(wheelReport(true, 3, 7, { ...base, tracking: true, urxvt: true })).toBe('\x1b[96;3;7M');
    expect(wheelReport(false, 300, 7, { ...base, tracking: true, utf8: true })).toBe('\x1b[M' + String.fromCodePoint(97, 332, 39));
    expect(wheelReport(true, 3, 7, { ...base, tracking: true })).toBe('\x1b[M' + String.fromCharCode(96, 35, 39));
    expect(wheelReport(true, 500, 500, { ...base, tracking: true })).toBe('\x1b[M' + String.fromCharCode(96, 255, 255));
  });

  it('reports to a tracking app, sends arrows on the alternate screen, scrolls locally otherwise', () => {
    expect(wheelAction(-2, at, { ...base, tracking: true, sgr: true })).toEqual({ kind: 'send', data: '\x1b[<64;3;7M'.repeat(2) });
    expect(wheelAction(1, at, { ...base, tracking: true, sgr: true, alt: true })).toEqual({ kind: 'send', data: '\x1b[<65;3;7M' });
    expect(wheelAction(-1, at, { ...base, alt: true })).toEqual({ kind: 'send', data: '\x1b[A' });
    expect(wheelAction(2, at, { ...base, alt: true, appCursor: true })).toEqual({ kind: 'send', data: '\x1bOB\x1bOB' });
    expect(wheelAction(-3, at, { ...base, alt: true, alternateScroll: false })).toEqual({ kind: 'local', lines: -3 });
    expect(wheelAction(3, at, base)).toEqual({ kind: 'local', lines: 3 });
    expect(wheelAction(0, at, { ...base, tracking: true })).toEqual({ kind: 'none' });
  });

  it('Shift+wheel always scrolls locally', () => {
    expect(wheelAction(-1, { ...at, shift: true }, { ...base, tracking: true, sgr: true })).toEqual({ kind: 'local', lines: -1 });
  });

  it('accumulates trackpad pixels and caps a fling', () => {
    const a = new WheelAccumulator();
    const g = { cellHeight: 20, rows: 30 };
    expect(a.steps(8, 0, g)).toBe(0);
    expect(a.steps(8, 0, g)).toBe(0);
    expect(a.steps(8, 0, g)).toBe(1);
    expect(a.steps(-4, 0, g)).toBe(0);
    expect(a.steps(-1000, 0, g)).toBe(-5);
    expect(a.steps(3, 1, g)).toBe(3);
    expect(a.steps(1, 2, g)).toBe(5);
  });

  it('maps pointer positions to 1-based cells', () => {
    const rect = { left: 10, top: 20, width: 800, height: 400 };
    expect(cellAt(10, 20, rect, 80, 20)).toEqual({ col: 1, row: 1 });
    expect(cellAt(25, 45, rect, 80, 20)).toEqual({ col: 2, row: 2 });
    expect(cellAt(5000, 5000, rect, 80, 20)).toEqual({ col: 80, row: 20 });
  });
});

describe('clickMove', () => {
  const narrow = () => 1;
  const cur = { x: 10, y: 4 };

  it('sends left arrows for a click before the cursor on its row', () => {
    expect(clickMove(7, 4, cur, narrow, base)).toBe('\x1b[D'.repeat(3));
  });

  it('sends right arrows for a click after the cursor', () => {
    expect(clickMove(12, 4, cur, narrow, base)).toBe('\x1b[C\x1b[C');
  });

  it('uses application cursor keys when DECCKM is set', () => {
    expect(clickMove(9, 4, cur, narrow, { ...base, appCursor: true })).toBe('\x1bOD');
  });

  it('counts a wide character once', () => {
    const wide = (x: number) => (x === 8 ? 0 : x === 7 ? 2 : 1);
    expect(clickMove(6, 4, cur, wide, base)).toBe('\x1b[D'.repeat(3));
  });

  it('works in the alternate screen, where Codex draws its composer', () => {
    expect(clickMove(9, 4, cur, narrow, { ...base, alt: true })).toBe('\x1b[D');
  });

  it('does nothing on another row, on the cursor, or when the app reads the mouse', () => {
    expect(clickMove(3, 2, cur, narrow, base)).toBeNull();
    expect(clickMove(10, 4, cur, narrow, base)).toBeNull();
    expect(clickMove(3, 4, cur, narrow, { ...base, tracking: true })).toBeNull();
  });
});
