import { describe, expect, it } from 'vitest';
import { findIssueUrl } from '../feedback';

const LINEAR = 30;

function per(fn: () => void): number {
  let n = 1;
  for (;;) {
    const t = performance.now();
    for (let i = 0; i < n; i++) fn();
    const ms = performance.now() - t;
    if (ms >= 5 || n >= 4096) return ms / n;
    n *= 2;
  }
}

function ratio(small: string, big: string): number {
  let best = Infinity;
  for (let i = 0; i < 5 && best >= LINEAR; i++) best = Math.min(best, per(() => findIssueUrl(big)) / per(() => findIssueUrl(small)));
  return best;
}

describe('ISSUE_URL scan scaling', () => {
  it('stays linear when a marker is followed by many short lines', () => {
    const text = (n: number) => `ISSUE_URL=${'a\n'.repeat(n)}`;
    expect(ratio(text(300), text(3000))).toBeLessThan(LINEAR);
  });

  it('stays linear with many markers', () => {
    const text = (n: number) => 'ISSUE_URL=https://github.com/o/r/is\n'.repeat(n);
    expect(ratio(text(100), text(1000))).toBeLessThan(LINEAR);
  });
});
