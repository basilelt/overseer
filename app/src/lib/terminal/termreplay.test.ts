import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { replayGuard } from './termreplay';

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
