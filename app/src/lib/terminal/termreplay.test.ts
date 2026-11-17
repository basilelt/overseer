import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { REPLAY_FRAME, replayGuard, replayPaint } from './termreplay';

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

const CPR = '\x1b[24;1R';

describe('replayGuard', () => {
  it('drops the terminal reports xterm answers to replayed queries', () => {
    const g = replayGuard(100);
    g.begin();
    expect(g.allow(CPR)).toBe(false);
    expect(g.allow('\x1b[0n')).toBe(false);
    expect(g.allow('\x1b[?62;4c')).toBe(false);
  });

  it('lets typed input through during the replay', () => {
    const g = replayGuard(100);
    g.begin();
    expect(g.allow('a')).toBe(true);
    expect(g.allow('\x1b[A')).toBe(true);
    expect(g.allow('\r')).toBe(true);
  });

  it('lets live reports through once the replay has gone quiet', () => {
    const g = replayGuard(100);
    g.begin();
    vi.advanceTimersByTime(100);
    expect(g.allow(CPR)).toBe(true);
  });

  it('stays closed while replay chunks keep arriving', () => {
    const g = replayGuard(100);
    g.begin();
    vi.advanceTimersByTime(80);
    g.chunk();
    vi.advanceTimersByTime(80);
    expect(g.allow(CPR)).toBe(false);
    vi.advanceTimersByTime(30);
    expect(g.allow(CPR)).toBe(true);
  });

  it('allows everything before the first attach and after dispose', () => {
    const g = replayGuard(100);
    expect(g.allow(CPR)).toBe(true);
    g.begin();
    g.dispose();
    expect(g.allow(CPR)).toBe(true);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('restarts the window on every attach with a single timer', () => {
    const g = replayGuard(100);
    g.begin();
    g.begin();
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(100);
    g.begin();
    expect(g.allow(CPR)).toBe(false);
  });
});

describe('replayPaint', () => {
  function setup() {
    const log: string[] = [];
    const p = replayPaint(
      () => log.push('hold'),
      () => log.push('release'),
      60,
      1000,
    );
    return { log, p };
  }

  it('holds the paints from the attach to the short frame that ends the replay', () => {
    const { log, p } = setup();
    p.begin();
    p.up();
    p.chunk(REPLAY_FRAME);
    vi.advanceTimersByTime(40);
    p.chunk(REPLAY_FRAME);
    vi.advanceTimersByTime(40);
    expect(log).toEqual(['hold']);
    p.chunk(120);
    expect(log).toEqual(['hold', 'release']);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('releases after a pause when the replay ends on a full frame or is empty', () => {
    const { log, p } = setup();
    p.begin();
    p.up();
    p.chunk(REPLAY_FRAME);
    vi.advanceTimersByTime(59);
    expect(log).toEqual(['hold']);
    vi.advanceTimersByTime(1);
    expect(log).toEqual(['hold', 'release']);
    p.begin();
    p.up();
    vi.advanceTimersByTime(60);
    expect(log).toEqual(['hold', 'release', 'hold', 'release']);
  });

  it('never holds longer than the cap while the attach is pending', () => {
    const { log, p } = setup();
    p.begin();
    vi.advanceTimersByTime(999);
    expect(log).toEqual(['hold']);
    vi.advanceTimersByTime(1);
    expect(log).toEqual(['hold', 'release']);
    expect(vi.getTimerCount()).toBe(0);
  });

  it('releases once when the connection drops and ignores live output afterwards', () => {
    const { log, p } = setup();
    p.up();
    p.chunk(REPLAY_FRAME);
    p.end();
    expect(log).toEqual([]);
    p.begin();
    p.end();
    p.end();
    p.chunk(REPLAY_FRAME);
    vi.advanceTimersByTime(2000);
    expect(log).toEqual(['hold', 'release']);
  });

  it('starts over on a new attach with one cap timer', () => {
    const { log, p } = setup();
    p.begin();
    p.up();
    p.begin();
    expect(vi.getTimerCount()).toBe(1);
    vi.advanceTimersByTime(1000);
    expect(log).toEqual(['hold', 'hold', 'release']);
  });
});
