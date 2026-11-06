import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import type { TermIO } from './client';
import { termLink, type LinkDeps } from './termlink';

interface Conn {
  io: TermIO;
  data(s: string): void;
  close(): void;
  closed: boolean;
}

function harness(retry: number | null = 100) {
  const conns: Conn[] = [];
  const opens: { resolve(): void; reject(e: unknown): void; conn: Conn }[] = [];
  const seen: string[] = [];
  const ups: TermIO[] = [];
  let downs = 0;
  let resets = 0;
  let ended = 0;
  const failures: unknown[] = [];
  const deps: LinkDeps = {
    open: (onData, onClose) =>
      new Promise((resolve, reject) => {
        const conn: Conn = {
          closed: false,
          io: { write() {}, resize() {}, close: () => void (conn.closed = true) },
          data: (s) => onData(new TextEncoder().encode(s)),
          close: onClose,
        };
        conns.push(conn);
        opens.push({ resolve: () => resolve(conn.io), reject, conn });
      }),
    reset: () => void resets++,
    data: (b) => void seen.push(new TextDecoder().decode(b)),
    up: (io) => void ups.push(io),
    down: () => void downs++,
    failed: (e) => void failures.push(e),
    retryMs: () => retry,
    ended: () => void ended++,
    now: () => 0,
  };
  return {
    link: termLink(deps),
    conns,
    opens,
    seen,
    ups,
    failures,
    get downs() {
      return downs;
    },
    get resets() {
      return resets;
    },
    get ended() {
      return ended;
    },
  };
}

const flush = () => Promise.resolve().then(() => Promise.resolve());

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe('termLink', () => {
  it('resets the terminal, opens one connection and writes its data', async () => {
    const h = harness();
    const done = h.link.attach();
    h.opens[0].resolve();
    await done;
    h.conns[0].data('hello');
    expect(h.resets).toBe(1);
    expect(h.ups).toEqual([h.conns[0].io]);
    expect(h.seen).toEqual(['hello']);
  });

  it('closes the previous connection and ignores its data when reattaching', async () => {
    const h = harness();
    const first = h.link.attach();
    h.opens[0].resolve();
    await first;
    h.link.reattach();
    expect(h.conns[0].closed).toBe(true);
    h.opens[1].resolve();
    await flush();
    h.conns[0].data('stale');
    h.conns[0].close();
    h.conns[1].data('fresh');
    expect(h.seen).toEqual(['fresh']);
    expect(h.resets).toBe(2);
    vi.runAllTimers();
    expect(h.conns).toHaveLength(2);
  });

  it('closes a connection that opens after a newer attach started', async () => {
    const h = harness();
    void h.link.attach();
    h.link.reattach();
    h.opens[0].resolve();
    h.opens[1].resolve();
    await flush();
    expect(h.conns[0].closed).toBe(true);
    expect(h.conns[1].closed).toBe(false);
    expect(h.ups).toEqual([h.conns[1].io]);
  });

  it('schedules a single retry when a failed attach also reports a close', async () => {
    const h = harness();
    const done = h.link.attach();
    h.conns[0].close();
    h.opens[0].reject('terminal attach failed');
    await done;
    expect(h.failures).toEqual(['terminal attach failed']);
    vi.advanceTimersByTime(100);
    expect(h.conns).toHaveLength(2);
    vi.advanceTimersByTime(1000);
    expect(h.conns).toHaveLength(2);
  });

  it('retries once after the daemon drops a live connection', async () => {
    const h = harness();
    const done = h.link.attach();
    h.opens[0].resolve();
    await done;
    h.conns[0].close();
    expect(h.downs).toBe(1);
    vi.advanceTimersByTime(100);
    expect(h.conns).toHaveLength(2);
  });

  it('re-attaches a link that gave up once the connection returns', async () => {
    const h = harness(null);
    const done = h.link.attach();
    h.opens[0].reject('terminal attach failed');
    await done;
    vi.runAllTimers();
    expect(h.conns).toHaveLength(1);
    h.link.resume();
    expect(h.conns).toHaveLength(2);
    h.opens[1].resolve();
    await flush();
    expect(h.ups).toEqual([h.conns[1].io]);
  });

  it('does not resume a live link, a pending attach or a finished terminal', async () => {
    const h = harness(null);
    const done = h.link.attach();
    h.link.resume();
    h.opens[0].resolve();
    await done;
    h.link.resume();
    expect(h.conns).toHaveLength(1);
    h.conns[0].close();
    expect(h.ended).toBe(1);
    h.link.resume();
    expect(h.conns).toHaveLength(1);
  });

  it('does not resume after dispose', async () => {
    const h = harness(null);
    const done = h.link.attach();
    h.opens[0].reject('x');
    await done;
    h.link.dispose();
    h.link.resume();
    expect(h.conns).toHaveLength(1);
  });

  it('cancels a pending retry when a resize reattaches first', async () => {
    const h = harness();
    const done = h.link.attach();
    h.opens[0].resolve();
    await done;
    h.conns[0].close();
    h.link.reattach();
    vi.advanceTimersByTime(1000);
    expect(h.conns).toHaveLength(2);
  });

  it('does not reattach on resize before anything is connected', () => {
    const h = harness();
    h.link.reattach();
    expect(h.conns).toHaveLength(0);
  });

  it('marks the terminal closed when the daemon says not to retry', async () => {
    const h = harness(null);
    const done = h.link.attach();
    h.opens[0].resolve();
    await done;
    h.conns[0].close();
    expect(h.ended).toBe(1);
  });

  it('retries when the connection closes before the attach resolves', async () => {
    const h = harness();
    const done = h.link.attach();
    h.conns[0].close();
    h.opens[0].resolve();
    await done;
    expect(h.conns[0].closed).toBe(true);
    expect(h.ups).toEqual([]);
    vi.advanceTimersByTime(100);
    expect(h.conns).toHaveLength(2);
  });

  it('closes everything and stops retrying once disposed', async () => {
    const h = harness();
    const done = h.link.attach();
    h.opens[0].resolve();
    await done;
    h.conns[0].close();
    h.link.dispose();
    vi.advanceTimersByTime(1000);
    expect(h.conns).toHaveLength(1);
    const late = harness();
    void late.link.attach();
    late.link.dispose();
    late.opens[0].resolve();
    await flush();
    expect(late.conns[0].closed).toBe(true);
    expect(late.ups).toEqual([]);
  });
});
