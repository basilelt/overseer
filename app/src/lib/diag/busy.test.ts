import { describe, expect, it } from 'vitest';
import { BusyMeter, LATE_MS, STALL_MS } from './busy';

function meter() {
  let t = 0;
  const m = new BusyMeter(() => t);
  return { m, tick: (ms: number) => (t += ms) };
}

describe('BusyMeter', () => {
  it('reports nothing while the page keeps up', () => {
    const { m, tick } = meter();
    m.add('term.write', STALL_MS - 1);
    expect(m.take(1000, 1000 + LATE_MS - 1)).toEqual({ late: 0, busy: '' });
    tick(2000);
    expect(m.take(undefined, 5000)).toEqual({ late: 0, busy: '' });
  });

  it('names the longest sections when one of them held the page', () => {
    const { m } = meter();
    m.add('term.write', 300, 1 << 20);
    m.add('term.write', 700, 1 << 20);
    m.add('ws.switch', 40);
    m.add('idle', 0.2);
    expect(m.take(1000, 1200)).toEqual({ late: 200, busy: 'term.write 1.0 s x2 (max 0.7 s, 2.0 MB), ws.switch 0.0 s x1 (max 0.0 s)' });
  });

  it('reports a late ping even when no measured section explains it', () => {
    const { m } = meter();
    expect(m.take(1000, 9000)).toEqual({ late: 8000, busy: '' });
    const r = meter();
    r.m.add('events', 3);
    expect(r.m.take(0, LATE_MS)).toEqual({ late: LATE_MS, busy: 'events 0.0 s x1 (max 0.0 s)' });
  });

  it('starts from zero after each report', () => {
    const { m, tick } = meter();
    m.add('term.open', 900);
    expect(m.take(0, 0).busy).toBe('term.open 0.9 s x1 (max 0.9 s)');
    tick(2000);
    expect(m.take(0, 0)).toEqual({ late: 0, busy: '' });
  });

  it('counts a stall once when the pings it held up arrive together', () => {
    const { m, tick } = meter();
    tick(10_000);
    expect(m.take(0, 9000).late).toBe(9000);
    tick(5);
    expect(m.take(2000, 9005).late).toBe(0);
    tick(2000);
    expect(m.take(11_000, 12_500).late).toBe(1500);
  });

  it('keeps the five heaviest sections', () => {
    const { m } = meter();
    for (let i = 1; i <= 7; i++) m.add(`s${i}`, i * 1000);
    expect(m.take(0, 0).busy.split(', ').map((x) => x.split(' ')[0])).toEqual(['s7', 's6', 's5', 's4', 's3']);
  });

  it('times a call and what follows a mark, also when the call throws', () => {
    const { m, tick } = meter();
    expect(m.run('a', () => (tick(600), 7))).toBe(7);
    expect(() =>
      m.run('b', () => {
        tick(800);
        throw new Error('x');
      }),
    ).toThrow('x');
    const done = m.since('c', 1 << 20);
    tick(1000);
    done();
    expect(m.take(0, 0).busy).toBe('c 1.0 s x1 (max 1.0 s, 1.0 MB), b 0.8 s x1 (max 0.8 s), a 0.6 s x1 (max 0.6 s)');
  });
});
